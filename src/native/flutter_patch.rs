//! Flutter SSL Pinning Bypass — patches Flutter's libflutter.so to disable
//! SSL certificate verification. Works by finding and patching the
//! ssl_crypto_x509_session_verify_cert_chain return path.
//!
//! This module is for authorized security testing and research purposes.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Result of a Flutter SSL pinning bypass attempt.
#[derive(Clone, Debug)]
pub struct FlutterPatchResult {
    pub patched: bool,
    pub target_file: PathBuf,
    pub arch: String,
    pub patches_applied: Vec<PatchSite>,
    pub message: String,
}

/// A single patch location.
#[derive(Clone, Debug)]
pub struct PatchSite {
    pub offset: u64,
    pub original: Vec<u8>,
    pub patched: Vec<u8>,
    pub description: String,
}

pub struct FlutterPatcher;

impl FlutterPatcher {
    /// Detect Flutter in the APK by checking for libflutter.so.
    pub fn detect_flutter(native_dir: &Path) -> Vec<PathBuf> {
        let mut flutter_libs = Vec::new();
        if !native_dir.exists() {
            return flutter_libs;
        }
        for entry in walkdir::WalkDir::new(native_dir).into_iter().filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "libflutter.so" {
                flutter_libs.push(entry.into_path());
            }
        }
        flutter_libs
    }

    /// Find libapp.so (Dart AOT snapshot) inside a native libs directory.
    pub fn find_libapp(native_dir: &Path) -> Option<PathBuf> {
        for entry in walkdir::WalkDir::new(native_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_name().to_string_lossy() == "libapp.so" {
                return Some(entry.into_path());
            }
        }
        None
    }

    /// Collect all .so files under native_dir.
    pub fn all_native_libs(native_dir: &Path) -> Vec<PathBuf> {
        if !native_dir.exists() { return Vec::new(); }
        walkdir::WalkDir::new(native_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "so"))
            .map(|e| e.into_path())
            .collect()
    }

    /// Check whether a .so file appears to be a Dart AOT snapshot (libapp.so).
    /// Detects the presence of Dart snapshot symbols or the kDart magic header.
    pub fn is_dart_snapshot(lib_path: &Path) -> bool {
        let data = match std::fs::read(lib_path) { Ok(d) => d, Err(_) => return false };
        // Check for known Dart snapshot symbol names in the binary
        let sentinel = b"_kDartIsolateSnapshotInstructions";
        data.windows(sentinel.len()).any(|w| w == sentinel)
            || data.windows(19).any(|w| w == b"kDartVmSnapshotData")
    }

    /// Extract the Flutter engine version from a libflutter.so binary.
    /// Scans for null-terminated ASCII strings matching a semver pattern near "FLUTTER" markers.
    pub fn extract_version_from_lib(lib_path: &Path) -> Option<String> {
        let data = std::fs::read(lib_path).ok()?;
        // Flutter embeds version strings adjacent to known keywords:
        // "Flutter/" "flutterVersion" "engineRevision"
        let markers: &[&[u8]] = &[b"Flutter/", b"flutterVersion", b"engineVersion", b"FLUTTER_VERSION"];
        for marker in markers {
            if let Some(idx) = data.windows(marker.len()).position(|w| w == *marker) {
                // Look for a semver-like string (digit.digit.digit) within the next 128 bytes
                let search_start = idx + marker.len();
                let search_end = (search_start + 128).min(data.len());
                let window = &data[search_start..search_end];
                if let Some(ver) = Self::find_semver_in_bytes(window) {
                    return Some(ver);
                }
            }
        }
        // Fallback: scan whole binary for standalone version strings
        // Look for "\0x.y.z\0" patterns (x is 1-9, total length 5-20)
        for i in 0..data.len().saturating_sub(6) {
            if data[i] == 0 && data[i + 1].is_ascii_digit() && data[i + 1] != b'0' {
                let end = data[i + 1..].iter().position(|&b| b == 0).unwrap_or(0);
                if end >= 4 && end <= 20 {
                    let s = &data[i + 1..i + 1 + end];
                    if let Ok(ver) = std::str::from_utf8(s) {
                        let dot_count = ver.chars().filter(|&c| c == '.').count();
                        if dot_count >= 1 && dot_count <= 3
                            && ver.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
                        {
                            return Some(ver.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn find_semver_in_bytes(data: &[u8]) -> Option<String> {
        for i in 0..data.len() {
            if data[i].is_ascii_digit() {
                let end = data[i..].iter().position(|&b| b == 0 || b == b' ' || b == b'\n').unwrap_or(data.len() - i);
                let s = std::str::from_utf8(&data[i..i + end]).ok()?;
                let dot_count = s.chars().filter(|&c| c == '.').count();
                if dot_count >= 1 && s.len() >= 3 && s.len() <= 20
                    && s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
                {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    /// Apply SSL pinning bypass to a Flutter libflutter.so.
    pub fn bypass_ssl_pinning(lib_path: &Path) -> Result<FlutterPatchResult> {
        let data = std::fs::read(lib_path)?;
        let elf = goblin::elf::Elf::parse(&data)
            .map_err(|e| anyhow::anyhow!("Failed to parse ELF: {}", e))?;

        let arch = Self::detect_arch_from_elf(&elf);

        // Determine .text section bounds for safe pattern search
        let text_bounds: Option<(usize, usize)> = elf.section_headers.iter().find_map(|sh| {
            let name = elf.shdr_strtab.get_at(sh.sh_name).unwrap_or("");
            if name == ".text" && sh.sh_size > 0 {
                Some((sh.sh_offset as usize, (sh.sh_offset + sh.sh_size) as usize))
            } else {
                None
            }
        });

        // Pattern-based search — constrained to .text section to avoid false positives
        let patterns = Self::get_bypass_patterns(&arch);
        let mut patches = Vec::new();
        let mut patched_data = data.clone();

        let (search_start, search_end) = text_bounds.unwrap_or((0, data.len()));

        for (pattern, replacement, desc) in &patterns {
            if pattern.len() != replacement.len() { continue; } // must be same length
            let search_region = &data[search_start..search_end.min(data.len())];
            for rel_offset in Self::find_pattern(search_region, pattern) {
                let abs_offset = search_start + rel_offset;
                if abs_offset + replacement.len() <= patched_data.len() {
                    let original = patched_data[abs_offset..abs_offset + replacement.len()].to_vec();
                    patched_data[abs_offset..abs_offset + replacement.len()].copy_from_slice(replacement);
                    patches.push(PatchSite {
                        offset: abs_offset as u64,
                        original,
                        patched: replacement.clone(),
                        description: desc.to_string(),
                    });
                }
            }
        }

        // Symbol-based patching (works on non-stripped builds)
        let ssl_symbols: Vec<_> = elf.dynsyms.iter().filter(|sym| {
            let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("");
            name.contains("ssl_crypto_x509_session_verify_cert_chain")
                || name.contains("SSL_CTX_set_verify")
                || name.contains("ssl_verify_cert_chain")
                || name.contains("ssl3_connect")
        }).collect();

        for sym in &ssl_symbols {
            let sym_name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("");
            if sym.st_value > 0 {
                if let Some(offset) = Self::vaddr_to_offset(&elf, sym.st_value) {
                    let nop_patch = Self::get_return_true_patch(&arch);
                    if offset + nop_patch.len() <= patched_data.len() {
                        let original = patched_data[offset..offset + nop_patch.len()].to_vec();
                        patched_data[offset..offset + nop_patch.len()].copy_from_slice(&nop_patch);
                        patches.push(PatchSite {
                            offset: offset as u64,
                            original,
                            patched: nop_patch,
                            description: format!("Symbol patch: {} → return true", sym_name),
                        });
                    }
                }
            }
        }

        if patches.is_empty() {
            return Ok(FlutterPatchResult {
                patched: false,
                target_file: lib_path.to_path_buf(),
                arch: arch.clone(),
                patches_applied: Vec::new(),
                message: format!(
                    "No SSL patterns found in {} (arch: {}). For stripped builds, use the Frida SSL bypass script in Runtime → Frida.",
                    lib_path.file_name().unwrap_or_default().to_string_lossy(), arch
                ),
            });
        }

        // Backup + write
        let backup_path = lib_path.with_extension("so.bak");
        std::fs::copy(lib_path, &backup_path)?;
        std::fs::write(lib_path, &patched_data)?;

        let patch_count = patches.len();
        Ok(FlutterPatchResult {
            patched: true,
            target_file: lib_path.to_path_buf(),
            arch,
            patches_applied: patches,
            message: format!(
                "SSL pinning bypassed ({} patch sites). Backup: {}",
                patch_count, backup_path.display()
            ),
        })
    }

    fn detect_arch_from_elf(elf: &goblin::elf::Elf) -> String {
        match elf.header.e_machine {
            0x28 => "arm".to_string(),
            0xB7 => "arm64".to_string(),
            0x03 => "x86".to_string(),
            0x3E => "x86_64".to_string(),
            other => format!("unknown({})", other),
        }
    }

    /// Architecture-specific SSL bypass patterns.
    /// Each entry: (pattern_to_find, replacement, description)
    /// ALL patterns have equal-length replacement so we don't shift code.
    fn get_bypass_patterns(arch: &str) -> Vec<(Vec<u8>, Vec<u8>, String)> {
        match arch {
            "arm64" => vec![
                // Pattern: MOV W0, #0 (= 00 00 80 52) followed immediately by RET (= C0 03 5F D6)
                // This is the "return failure" path in ssl_crypto_x509_session_verify_cert_chain.
                // Replace with: MOV W0, #1; RET → bypass the verification.
                (
                    vec![0x00, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6],
                    vec![0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6],
                    "ARM64: ssl_verify ret(false) → ret(true)".to_string(),
                ),
                // Pattern: MOV W0, WZR (another way to zero W0) + RET
                // Encoding: E0 03 1F 2A = MOV W0, WZR
                (
                    vec![0xE0, 0x03, 0x1F, 0x2A, 0xC0, 0x03, 0x5F, 0xD6],
                    vec![0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6],
                    "ARM64: ssl_verify WZR ret → ret(true)".to_string(),
                ),
                // Pattern: CSET W0, EQ (set 1 if equal, 0 if not) + RET where the preceding
                // comparison determines ssl verify result. Often appears as "return (error == 0)".
                // E0 07 9F 1A = CSET W0, EQ ; followed by RET
                // Patch: skip the CSET and directly return 1
                (
                    vec![0xE0, 0x07, 0x9F, 0x1A, 0xC0, 0x03, 0x5F, 0xD6],
                    vec![0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6],
                    "ARM64: ssl_verify CSET+RET → ret(true)".to_string(),
                ),
            ],
            "arm" => vec![
                // ARM 32-bit: MOV R0, #0 (= 00 00 A0 E3); BX LR (= 1E FF 2F E1)
                // Replace with: MOV R0, #1; BX LR
                (
                    vec![0x00, 0x00, 0xA0, 0xE3, 0x1E, 0xFF, 0x2F, 0xE1],
                    vec![0x01, 0x00, 0xA0, 0xE3, 0x1E, 0xFF, 0x2F, 0xE1],
                    "ARM: ssl_verify ret(false) → ret(true)".to_string(),
                ),
                // ARM: EOR R0,R0,R0 (= 00 00 00 E0); BX LR — another way to zero R0
                (
                    vec![0x00, 0x00, 0x00, 0xE0, 0x1E, 0xFF, 0x2F, 0xE1],
                    vec![0x01, 0x00, 0xA0, 0xE3, 0x1E, 0xFF, 0x2F, 0xE1],
                    "ARM: ssl_verify EOR ret → ret(true)".to_string(),
                ),
            ],
            _ => vec![],
        }
    }

    /// Return a "return 1 immediately" bytecode for the given architecture.
    fn get_return_true_patch(arch: &str) -> Vec<u8> {
        match arch {
            "arm64" => vec![
                0x20, 0x00, 0x80, 0x52, // MOV W0, #1
                0xC0, 0x03, 0x5F, 0xD6, // RET
            ],
            "arm" => vec![
                0x01, 0x00, 0xA0, 0xE3, // MOV R0, #1
                0x1E, 0xFF, 0x2F, 0xE1, // BX LR
            ],
            "x86_64" | "x86" => vec![
                0xB8, 0x01, 0x00, 0x00, 0x00, // MOV EAX, 1
                0xC3,                          // RET
            ],
            _ => vec![],
        }
    }

    fn find_pattern(data: &[u8], pattern: &[u8]) -> Vec<usize> {
        if pattern.is_empty() || pattern.len() > data.len() {
            return Vec::new();
        }
        (0..=(data.len() - pattern.len()))
            .filter(|&i| &data[i..i + pattern.len()] == pattern)
            .collect()
    }

    fn vaddr_to_offset(elf: &goblin::elf::Elf, vaddr: u64) -> Option<usize> {
        for ph in &elf.program_headers {
            if ph.p_type == goblin::elf::program_header::PT_LOAD
                && vaddr >= ph.p_vaddr
                && vaddr < ph.p_vaddr + ph.p_memsz
            {
                return Some((vaddr - ph.p_vaddr + ph.p_offset) as usize);
            }
        }
        None
    }
}
