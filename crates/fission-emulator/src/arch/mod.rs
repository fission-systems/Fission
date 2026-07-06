pub mod calling_convention;
pub use calling_convention::*;

use anyhow::{Result, bail};
use fission_loader::loader::LoadedBinary;

/// Byte order of the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

/// All architecture-specific facts needed by the emulator.
/// Derived once at startup from the Sleigh Language ID; no runtime branching needed.
pub struct ArchInfo {
    /// Human-readable name ("x86:64", "AARCH64", …)
    pub name: &'static str,
    /// Address/pointer size in bytes (4 or 8).
    pub pointer_size: u8,
    /// Byte order of the target.
    pub endian: Endianness,
    /// Name of the program counter register (as in the Sleigh register map).
    pub pc_reg: &'static str,
    /// Name of the stack pointer register.
    pub sp_reg: &'static str,
    /// Default calling convention for HLE argument parsing.
    pub cc: Box<dyn CallingConvention>,
}

impl ArchInfo {
    // ── x86 ──────────────────────────────────────────────────────────────────

    /// x86-64, Windows (Win64 ABI)
    pub fn x86_64_win() -> Self {
        Self {
            name: "x86:LE:64:default",
            pointer_size: 8,
            endian: Endianness::Little,
            pc_reg: "rip",
            sp_reg: "rsp",
            cc: Box::new(Win64Cc),
        }
    }

    /// x86-64, Linux / SysV
    pub fn x86_64_sysv() -> Self {
        Self {
            name: "x86:LE:64:default",
            pointer_size: 8,
            endian: Endianness::Little,
            pc_reg: "rip",
            sp_reg: "rsp",
            cc: Box::new(SysV64Cc),
        }
    }

    /// x86-32 (cdecl / stdcall)
    pub fn x86_32() -> Self {
        Self {
            name: "x86:LE:32:default",
            pointer_size: 4,
            endian: Endianness::Little,
            pc_reg: "eip",
            sp_reg: "esp",
            cc: Box::new(Cdecl32Cc),
        }
    }

    // ── ARM ──────────────────────────────────────────────────────────────────

    /// AArch64 / ARM64 (AAPCS64)
    pub fn aarch64() -> Self {
        Self {
            name: "AARCH64:LE:64:v8A",
            pointer_size: 8,
            endian: Endianness::Little,
            pc_reg: "pc",
            sp_reg: "sp",
            cc: Box::new(Aarch64Cc),
        }
    }

    /// ARM32 (AAPCS)
    pub fn arm32() -> Self {
        Self {
            name: "ARM:LE:32:v7",
            pointer_size: 4,
            endian: Endianness::Little,
            pc_reg: "r15",
            sp_reg: "r13",
            cc: Box::new(Arm32Cc),
        }
    }

    // ── MIPS ─────────────────────────────────────────────────────────────────

    /// MIPS32 big-endian (o32 ABI)
    pub fn mips32be() -> Self {
        Self {
            name: "MIPS:BE:32:default",
            pointer_size: 4,
            endian: Endianness::Big,
            pc_reg: "pc",
            sp_reg: "sp",
            cc: Box::new(MipsO32Cc),
        }
    }

    /// MIPS32 little-endian (o32 ABI)
    pub fn mips32le() -> Self {
        Self {
            name: "MIPS:LE:32:default",
            pointer_size: 4,
            endian: Endianness::Little,
            pc_reg: "pc",
            sp_reg: "sp",
            cc: Box::new(MipsO32Cc),
        }
    }

    // ── PowerPC ──────────────────────────────────────────────────────────────

    /// PowerPC 32-bit (ELF ABI)
    pub fn ppc32() -> Self {
        Self {
            name: "PowerPC:BE:32:default",
            pointer_size: 4,
            endian: Endianness::Big,
            pc_reg: "pc",
            sp_reg: "r1",
            cc: Box::new(PpcElfCc),
        }
    }

    /// PowerPC 64-bit (ELF ABI)
    pub fn ppc64() -> Self {
        Self {
            name: "PowerPC:BE:64:default",
            pointer_size: 8,
            endian: Endianness::Big,
            pc_reg: "pc",
            sp_reg: "r1",
            cc: Box::new(PpcElfCc),
        }
    }

    // ── Factory ──────────────────────────────────────────────────────────────

    /// Derive `ArchInfo` from a Sleigh Language ID and (optionally) a binary.
    ///
    /// The OS is detected from the binary format + machine type to pick the
    /// correct calling convention variant (Win64 vs. SysV for x86-64).
    pub fn from_language_id(lang_id: &str, binary: Option<&LoadedBinary>) -> Result<ArchInfo> {
        let is_pe = binary.map(|b| b.format == "PE").unwrap_or(false);
        let arch = match lang_id {
            s if s.contains("x86:LE:64") => {
                // Use Win64 CC for PE files, SysV for ELF/Mach-O
                if is_pe { ArchInfo::x86_64_win() } else { ArchInfo::x86_64_sysv() }
            }
            s if s.contains("x86:LE:32") => ArchInfo::x86_32(),
            s if s.contains("AARCH64") || s.contains("AArch64") => ArchInfo::aarch64(),
            s if s.contains("ARM:LE:32") || s.contains("ARM:BE:32") => ArchInfo::arm32(),
            s if s.contains("MIPS:BE:32") => ArchInfo::mips32be(),
            s if s.contains("MIPS:LE:32") => ArchInfo::mips32le(),
            s if s.contains("PowerPC:BE:32") => ArchInfo::ppc32(),
            s if s.contains("PowerPC:BE:64") => ArchInfo::ppc64(),
            _ => bail!("Unsupported Sleigh Language ID: {}", lang_id),
        };
        tracing::debug!("ArchInfo resolved: {} (ptr={}B, cc={:?})", arch.name, arch.pointer_size, arch.cc.arg_regs());
        Ok(arch)
    }
}
