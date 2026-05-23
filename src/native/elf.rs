//! ELF parser — parse .so native libraries using goblin.

use anyhow::Result;
use std::path::Path;

/// Parsed information from an ELF binary.
#[derive(Debug, Clone)]
pub struct ElfInfo {
    pub path: String,
    pub machine: String,
    pub elf_type: String,
    pub entry_point: u64,
    pub sections: Vec<SectionInfo>,
    pub symbols: Vec<SymbolInfo>,
    pub dynamic_libs: Vec<String>,
    pub imports: Vec<ImportExport>,
    pub exports: Vec<ImportExport>,
    /// True when the library is a Dart AOT snapshot (libapp.so, contains _kDartIsolateSnapshotInstructions).
    pub is_dart_snapshot: bool,
    /// True when the library is the Flutter engine (libflutter.so).
    pub is_flutter_engine: bool,
}

#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub name: String,
    pub section_type: String,
    pub addr: u64,
    pub size: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub addr: u64,
    pub size: u64,
    pub bind: String,
    pub sym_type: String,
}

/// Import or export entry with optional demangled name.
#[derive(Debug, Clone)]
pub struct ImportExport {
    pub name: String,
    pub addr: u64,
    pub demangled: Option<String>,
}

pub struct ElfParser;

impl ElfParser {
    /// Parse an ELF file and return structured info.
    pub fn parse(path: &Path) -> Result<ElfInfo> {
        let data = std::fs::read(path)?;
        let elf = goblin::elf::Elf::parse(&data)?;

        let machine = format!("{:?}", elf.header.e_machine);
        let elf_type = format!("{:?}", elf.header.e_type);
        let entry_point = elf.header.e_entry;

        // Sections
        let sections: Vec<SectionInfo> = elf
            .section_headers
            .iter()
            .map(|sh| {
                let name = elf
                    .shdr_strtab
                    .get_at(sh.sh_name)
                    .unwrap_or("<unknown>")
                    .to_string();
                SectionInfo {
                    name,
                    section_type: format!("{:#x}", sh.sh_type),
                    addr: sh.sh_addr,
                    size: sh.sh_size,
                    offset: sh.sh_offset,
                }
            })
            .collect();

        // Symbols (dynsyms + syms)
        let mut symbols: Vec<SymbolInfo> = Vec::new();
        for sym in &elf.dynsyms {
            let name = elf
                .dynstrtab
                .get_at(sym.st_name)
                .unwrap_or("<unknown>")
                .to_string();
            if !name.is_empty() && name != "<unknown>" {
                symbols.push(SymbolInfo {
                    name,
                    addr: sym.st_value,
                    size: sym.st_size,
                    bind: format!("{:?}", sym.st_bind()),
                    sym_type: format!("{:?}", sym.st_type()),
                });
            }
        }

        // Dynamic libraries
        let dynamic_libs: Vec<String> = elf
            .libraries
            .iter()
            .map(|l| l.to_string())
            .collect();

        // Classify symbols into imports (undefined, addr=0) and exports (defined, addr!=0)
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        for sym in &elf.dynsyms {
            let name = elf
                .dynstrtab
                .get_at(sym.st_name)
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let demangled = Self::try_demangle(&name);
            let ie = ImportExport {
                name: name.clone(),
                addr: sym.st_value,
                demangled,
            };
            if sym.st_value == 0 || sym.is_import() {
                imports.push(ie);
            } else {
                exports.push(ie);
            }
        }

        let is_dart_snapshot = exports.iter().any(|e| e.name.contains("kDartIsolateSnapshotInstructions"));
        let is_flutter_engine = path.file_name()
            .map(|f| f.to_string_lossy().to_lowercase().contains("libflutter"))
            .unwrap_or(false);

        Ok(ElfInfo {
            path: path.display().to_string(),
            machine,
            elf_type,
            entry_point,
            sections,
            symbols,
            dynamic_libs,
            imports,
            exports,
            is_dart_snapshot,
            is_flutter_engine,
        })
    }

    /// Get a human-readable summary of an ELF file.
    pub fn summary(info: &ElfInfo) -> String {
        let mut out = String::new();
        out.push_str(&format!("ELF: {}\n", info.path));
        out.push_str(&format!("Machine: {}\n", info.machine));
        out.push_str(&format!("Type: {}\n", info.elf_type));
        out.push_str(&format!("Entry: {:#x}\n", info.entry_point));
        out.push_str(&format!("Sections: {}\n", info.sections.len()));
        out.push_str(&format!("Symbols: {}\n", info.symbols.len()));
        out.push_str(&format!("Imports: {} | Exports: {}\n", info.imports.len(), info.exports.len()));
        out.push_str(&format!("Linked libs: {}\n", info.dynamic_libs.join(", ")));
        out
    }

    /// Attempt to demangle a C++ symbol name.
    fn try_demangle(name: &str) -> Option<String> {
        // Simple Itanium ABI demangling for _Z prefixed names
        if !name.starts_with("_Z") {
            return None;
        }
        // Use a basic demangler — strip _Z prefix and decode the mangled name
        // This is a simplified version; full demangling would need cpp_demangle crate
        let mangled = name;
        // Try to extract the basic name from common patterns
        // _ZN<len><name>... for nested names
        if mangled.starts_with("_ZN") {
            let rest = &mangled[3..];
            let mut result = Vec::new();
            let mut pos = 0;
            let bytes = rest.as_bytes();
            while pos < bytes.len() {
                // Read length
                let mut len = 0_usize;
                while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    len = len * 10 + (bytes[pos] - b'0') as usize;
                    pos += 1;
                }
                if len == 0 || pos + len > bytes.len() {
                    break;
                }
                result.push(&rest[pos..pos + len]);
                pos += len;
                // Check for end marker 'E'
                if pos < bytes.len() && bytes[pos] == b'E' {
                    break;
                }
            }
            if !result.is_empty() {
                return Some(result.join("::"));
            }
        }
        None
    }
}
