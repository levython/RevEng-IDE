//! Disassembler — disassemble native code using Capstone.

use anyhow::Result;
use capstone::prelude::*;

/// A single disassembled instruction.
#[derive(Debug, Clone)]
pub struct DisasmInstruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operands: String,
}

/// Architecture configuration for disassembly.
#[derive(Debug, Clone, Copy)]
pub enum DisasmArch {
    Arm,
    Arm64,
    X86,
    X86_64,
}

pub struct Disassembler;

impl Disassembler {
    /// Disassemble a buffer of raw bytes at a given base address.
    pub fn disassemble(
        data: &[u8],
        base_addr: u64,
        arch: DisasmArch,
        max_instructions: usize,
    ) -> Result<Vec<DisasmInstruction>> {
        let cs = Self::create_capstone(arch)?;

        let insns = cs
            .disasm_count(data, base_addr, max_instructions)
            .map_err(|e| anyhow::anyhow!("Disassembly failed: {}", e))?;

        let result: Vec<DisasmInstruction> = insns
            .iter()
            .map(|insn| DisasmInstruction {
                address: insn.address(),
                bytes: insn.bytes().to_vec(),
                mnemonic: insn.mnemonic().unwrap_or("???").to_string(),
                operands: insn.op_str().unwrap_or("").to_string(),
            })
            .collect();

        Ok(result)
    }

    /// Disassemble an entire section from an ELF file.
    pub fn disassemble_section(
        elf_data: &[u8],
        section_offset: u64,
        section_size: u64,
        section_addr: u64,
        arch: DisasmArch,
    ) -> Result<Vec<DisasmInstruction>> {
        let start = section_offset as usize;
        let end = start + section_size as usize;

        if end > elf_data.len() {
            anyhow::bail!(
                "Section out of bounds: offset={:#x} size={:#x} file_len={:#x}",
                section_offset,
                section_size,
                elf_data.len()
            );
        }

        let section_data = &elf_data[start..end];
        Self::disassemble(section_data, section_addr, arch, 10_000)
    }

    /// Detect architecture from ELF machine type string.
    pub fn detect_arch(machine: &str) -> DisasmArch {
        let m = machine.to_uppercase();
        if m.contains("AARCH64") || m.contains("ARM64") || m.contains("183") {
            DisasmArch::Arm64
        } else if m.contains("ARM") || m.contains("40") {
            DisasmArch::Arm
        } else if m.contains("X86_64") || m.contains("62") {
            DisasmArch::X86_64
        } else {
            DisasmArch::X86
        }
    }

    fn create_capstone(arch: DisasmArch) -> Result<Capstone> {
        let cs = match arch {
            DisasmArch::Arm => Capstone::new()
                .arm()
                .mode(arch::arm::ArchMode::Arm)
                .build(),
            DisasmArch::Arm64 => Capstone::new()
                .arm64()
                .mode(arch::arm64::ArchMode::Arm)
                .build(),
            DisasmArch::X86 => Capstone::new()
                .x86()
                .mode(arch::x86::ArchMode::Mode32)
                .build(),
            DisasmArch::X86_64 => Capstone::new()
                .x86()
                .mode(arch::x86::ArchMode::Mode64)
                .build(),
        }
        .map_err(|e| anyhow::anyhow!("Capstone init failed: {}", e))?;

        Ok(cs)
    }

    /// Format a single instruction for display.
    pub fn format_instruction(insn: &DisasmInstruction) -> String {
        let hex_bytes: String = insn
            .bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "{:#010x}  {:20}  {} {}",
            insn.address, hex_bytes, insn.mnemonic, insn.operands
        )
    }
}
