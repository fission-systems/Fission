//! Ghidra-style language/compiler selection contracts.
//!
//! This module mirrors Ghidra's loader-facing `LoadSpec` boundary: binary
//! format metadata is resolved to a language/compiler pair before any SLEIGH
//! runtime is selected.

use crate::constants::binary_format::{
    MACHO_CPU_TYPE_ARM, MACHO_CPU_TYPE_ARM64, MACHO_CPU_TYPE_X86, MACHO_CPU_TYPE_X86_64,
};
use crate::core_constants::{
    ELFCLASS32, ELFCLASS64, ELFDATA2LSB, ELFDATA2MSB, EM_386, EM_AARCH64, EM_ARM, EM_BPF,
    EM_LOONGARCH, EM_MIPS, EM_PPC, EM_PPC64, EM_RISCV, EM_SPARCV9, EM_X86_64,
    IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM, IMAGE_FILE_MACHINE_ARM64,
    IMAGE_FILE_MACHINE_I386,
};
use rkyv::{Archive, Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct GhidraLanguageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct CompilerSpecId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct LanguageCompilerSpecPair {
    pub language_id: GhidraLanguageId,
    pub compiler_spec_id: CompilerSpecId,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct BinaryLoadSpec {
    pub format: String,
    pub image_base: u64,
    pub pair: LanguageCompilerSpecPair,
    pub preferred: bool,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct ArchitectureDescriptor {
    pub processor: String,
    pub endian: String,
    pub bitness: u8,
    pub variant: String,
    pub abi: Option<String>,
    pub raw_machine: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ArchitectureSelectionError {
    #[error("unsupported machine for {format}: {machine}")]
    UnsupportedMachine { format: String, machine: String },
    #[error("ambiguous load spec for {format}: {machine}")]
    AmbiguousLoadSpec { format: String, machine: String },
    #[error("missing language for {format}: {machine}")]
    MissingLanguage { format: String, machine: String },
}

impl GhidraLanguageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CompilerSpecId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl LanguageCompilerSpecPair {
    pub fn new(language_id: impl Into<String>, compiler_spec_id: impl Into<String>) -> Self {
        Self {
            language_id: GhidraLanguageId(language_id.into()),
            compiler_spec_id: CompilerSpecId(compiler_spec_id.into()),
        }
    }
}

impl BinaryLoadSpec {
    pub fn new(
        format: impl Into<String>,
        image_base: u64,
        language_id: impl Into<String>,
        compiler_spec_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            format: format.into(),
            image_base,
            pair: LanguageCompilerSpecPair::new(language_id, compiler_spec_id),
            preferred: true,
            source: source.into(),
        }
    }

    pub fn compatibility_from_language_id(
        format: impl Into<String>,
        image_base: u64,
        language_id: impl Into<String>,
    ) -> Self {
        Self::new(
            format,
            image_base,
            language_id,
            "default",
            "compatibility-arch-spec",
        )
    }
}

impl ArchitectureDescriptor {
    pub fn new(
        processor: impl Into<String>,
        endian: impl Into<String>,
        bitness: u8,
        variant: impl Into<String>,
        abi: Option<String>,
        raw_machine: impl Into<String>,
    ) -> Self {
        Self {
            processor: processor.into(),
            endian: endian.into(),
            bitness,
            variant: variant.into(),
            abi,
            raw_machine: raw_machine.into(),
        }
    }
}

pub type ArchitectureSelectionResult =
    Result<(ArchitectureDescriptor, BinaryLoadSpec), ArchitectureSelectionError>;

pub fn select_pe_load_spec(
    machine: u16,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    match machine {
        IMAGE_FILE_MACHINE_AMD64 if is_64bit => Ok(selection(
            "PE",
            image_base,
            "x86",
            "little",
            64,
            "default",
            Some("windows".to_string()),
            "x86:LE:64:default",
            "windows",
            format!("PE Machine=0x{machine:04x}"),
        )),
        IMAGE_FILE_MACHINE_I386 if !is_64bit => Ok(selection(
            "PE",
            image_base,
            "x86",
            "little",
            32,
            "default",
            Some("windows".to_string()),
            "x86:LE:32:default",
            "windows",
            format!("PE Machine=0x{machine:04x}"),
        )),
        IMAGE_FILE_MACHINE_ARM if !is_64bit => Ok(selection(
            "PE",
            image_base,
            "ARM",
            "little",
            32,
            "v7",
            Some("windows".to_string()),
            "ARM:LE:32:v7",
            "windows",
            format!("PE Machine=0x{machine:04x}"),
        )),
        IMAGE_FILE_MACHINE_ARM64 if is_64bit => Ok(selection(
            "PE",
            image_base,
            "AARCH64",
            "little",
            64,
            "v8A",
            Some("windows".to_string()),
            "AARCH64:LE:64:v8A",
            "windows",
            format!("PE Machine=0x{machine:04x}"),
        )),
        _ => Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "PE".to_string(),
            machine: format!("Machine=0x{machine:04x}, is_64bit={is_64bit}"),
        }),
    }
}

pub fn select_coff_load_spec(
    machine: u16,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let (mut architecture, mut load_spec) = select_pe_load_spec(machine, is_64bit, image_base)
        .map_err(|_| ArchitectureSelectionError::UnsupportedMachine {
            format: "COFF".to_string(),
            machine: format!("Machine=0x{machine:04x}, is_64bit={is_64bit}"),
        })?;
    architecture.abi = Some("coff".to_string());
    architecture.raw_machine = format!("COFF Machine=0x{machine:04x}");
    load_spec.format = "COFF".to_string();
    load_spec.source = format!("COFF Machine=0x{machine:04x}");
    Ok((architecture, load_spec))
}

pub fn select_elf_load_spec(
    machine: u16,
    class: u8,
    data_encoding: u8,
    flags: u32,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = match class {
        ELFCLASS32 => 32,
        ELFCLASS64 => 64,
        _ => {
            return Err(ArchitectureSelectionError::UnsupportedMachine {
                format: "ELF".to_string(),
                machine: format!("class={class}, machine=0x{machine:04x}"),
            });
        }
    };
    let endian = match data_encoding {
        ELFDATA2LSB => "little",
        ELFDATA2MSB => "big",
        _ => {
            return Err(ArchitectureSelectionError::UnsupportedMachine {
                format: "ELF".to_string(),
                machine: format!("data_encoding={data_encoding}, machine=0x{machine:04x}"),
            });
        }
    };

    match (machine, bitness, endian) {
        (EM_X86_64, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "x86",
            "little",
            64,
            "default",
            Some("gcc".to_string()),
            "x86:LE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_386, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "x86",
            "little",
            32,
            "default",
            Some("gcc".to_string()),
            "x86:LE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_AARCH64, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "AARCH64",
            "little",
            64,
            "v8A",
            Some("gcc".to_string()),
            "AARCH64:LE:64:v8A",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_AARCH64, 64, "big") => Ok(selection(
            "ELF",
            image_base,
            "AARCH64",
            "big",
            64,
            "v8A",
            Some("gcc".to_string()),
            "AARCH64:BE:64:v8A",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_ARM, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "ARM",
            "little",
            32,
            "v7",
            Some("gcc".to_string()),
            "ARM:LE:32:v7",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_ARM, 32, "big") => Ok(selection(
            "ELF",
            image_base,
            "ARM",
            "big",
            32,
            "v7",
            Some("gcc".to_string()),
            "ARM:BE:32:v7",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_RISCV, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "RISCV",
            "little",
            32,
            "default",
            Some("gcc".to_string()),
            "RISCV:LE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_RISCV, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "RISCV",
            "little",
            64,
            "default",
            Some("gcc".to_string()),
            "RISCV:LE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 32, "little") if (flags & 0x8000_0000) != 0 => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "little",
            32,
            "R6",
            Some("gcc".to_string()),
            "MIPS:LE:32:R6",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 32, "big") if (flags & 0x8000_0000) != 0 => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "big",
            32,
            "R6",
            Some("gcc".to_string()),
            "MIPS:BE:32:R6",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "little",
            32,
            "default",
            Some("gcc".to_string()),
            "MIPS:LE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 32, "big") => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "big",
            32,
            "default",
            Some("gcc".to_string()),
            "MIPS:BE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "little",
            64,
            "default",
            Some("gcc".to_string()),
            "MIPS:LE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_MIPS, 64, "big") => Ok(selection(
            "ELF",
            image_base,
            "MIPS",
            "big",
            64,
            "default",
            Some("gcc".to_string()),
            "MIPS:BE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_PPC, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "PowerPC",
            "little",
            32,
            "default",
            Some("gcc".to_string()),
            "PowerPC:LE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_PPC, 32, "big") => Ok(selection(
            "ELF",
            image_base,
            "PowerPC",
            "big",
            32,
            "default",
            Some("gcc".to_string()),
            "PowerPC:BE:32:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_PPC64, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "PowerPC",
            "little",
            64,
            "default",
            Some("gcc".to_string()),
            "PowerPC:LE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_PPC64, 64, "big") => Ok(selection(
            "ELF",
            image_base,
            "PowerPC",
            "big",
            64,
            "default",
            Some("gcc".to_string()),
            "PowerPC:BE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_SPARCV9, 64, "big") => Ok(selection(
            "ELF",
            image_base,
            "sparc",
            "big",
            64,
            "default",
            Some("gcc".to_string()),
            "sparc:BE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_BPF, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "eBPF",
            "little",
            64,
            "default",
            Some("gcc".to_string()),
            "eBPF:LE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_BPF, 64, "big") => Ok(selection(
            "ELF",
            image_base,
            "eBPF",
            "big",
            64,
            "default",
            Some("gcc".to_string()),
            "eBPF:BE:64:default",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_LOONGARCH, 32, "little") => Ok(selection(
            "ELF",
            image_base,
            "Loongarch",
            "little",
            32,
            "ilp32d",
            Some("gcc".to_string()),
            "Loongarch:LE:32:ilp32d",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_LOONGARCH, 64, "little") if flags == 0x42 => Ok(selection(
            "ELF",
            image_base,
            "Loongarch",
            "little",
            64,
            "lp64f",
            Some("gcc".to_string()),
            "Loongarch:LE:64:lp64f",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        (EM_LOONGARCH, 64, "little") => Ok(selection(
            "ELF",
            image_base,
            "Loongarch",
            "little",
            64,
            "lp64d",
            Some("gcc".to_string()),
            "Loongarch:LE:64:lp64d",
            "gcc",
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        )),
        _ => Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "ELF".to_string(),
            machine: format!(
                "class={class}, data_encoding={data_encoding}, machine=0x{machine:04x}, flags=0x{flags:08x}"
            ),
        }),
    }
}

pub fn select_macho_load_spec(
    cputype: i32,
    cpusubtype: i32,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    match (cputype, is_64bit) {
        (MACHO_CPU_TYPE_X86_64, true) => Ok(selection(
            "Mach-O",
            image_base,
            "x86",
            "little",
            64,
            "default",
            Some("macosx".to_string()),
            "x86:LE:64:default",
            "gcc",
            format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}"),
        )),
        (MACHO_CPU_TYPE_X86, false) => Ok(selection(
            "Mach-O",
            image_base,
            "x86",
            "little",
            32,
            "default",
            Some("macosx".to_string()),
            "x86:LE:32:default",
            "gcc",
            format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}"),
        )),
        (MACHO_CPU_TYPE_ARM64, true) => Ok(selection(
            "Mach-O",
            image_base,
            "AARCH64",
            "little",
            64,
            "AppleSilicon",
            Some("macosx".to_string()),
            "AARCH64:LE:64:AppleSilicon",
            "default",
            format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}"),
        )),
        (MACHO_CPU_TYPE_ARM, false) => Ok(selection(
            "Mach-O",
            image_base,
            "ARM",
            "little",
            32,
            "v7",
            Some("macosx".to_string()),
            "ARM:LE:32:v7",
            "default",
            format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}"),
        )),
        _ => Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "Mach-O".to_string(),
            machine: format!(
                "cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}, is_64bit={is_64bit}"
            ),
        }),
    }
}

fn selection(
    format: &str,
    image_base: u64,
    processor: &str,
    endian: &str,
    bitness: u8,
    variant: &str,
    abi: Option<String>,
    language_id: &str,
    compiler_spec_id: &str,
    raw_machine: String,
) -> (ArchitectureDescriptor, BinaryLoadSpec) {
    let architecture = ArchitectureDescriptor::new(
        processor,
        endian,
        bitness,
        variant,
        abi,
        raw_machine.clone(),
    );
    let load_spec = BinaryLoadSpec::new(
        format,
        image_base,
        language_id,
        compiler_spec_id,
        raw_machine,
    );
    (architecture, load_spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_pe_amd64() {
        let (_, spec) = select_pe_load_spec(IMAGE_FILE_MACHINE_AMD64, true, 0x140000000)
            .expect("select PE amd64");
        assert_eq!(spec.pair.language_id.as_str(), "x86:LE:64:default");
        assert_eq!(spec.pair.compiler_spec_id.as_str(), "windows");
    }

    #[test]
    fn rejects_unknown_pe_machine() {
        assert!(matches!(
            select_pe_load_spec(0xffff, true, 0),
            Err(ArchitectureSelectionError::UnsupportedMachine { .. })
        ));
    }

    #[test]
    fn selects_elf_aarch64_little_endian() {
        let (_, spec) =
            select_elf_load_spec(EM_AARCH64, ELFCLASS64, ELFDATA2LSB, 0, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "AARCH64:LE:64:v8A");
    }

    #[test]
    fn selects_elf_riscv64_little_endian() {
        let (_, spec) =
            select_elf_load_spec(EM_RISCV, ELFCLASS64, ELFDATA2LSB, 0x5, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "RISCV:LE:64:default");
    }

    #[test]
    fn selects_elf_mips32_r6_little_endian() {
        let (_, spec) = select_elf_load_spec(EM_MIPS, ELFCLASS32, ELFDATA2LSB, 0x9000_1405, 0)
            .expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "MIPS:LE:32:R6");
    }

    #[test]
    fn selects_elf_ppc64_little_endian() {
        let (_, spec) =
            select_elf_load_spec(EM_PPC64, ELFCLASS64, ELFDATA2LSB, 0x2, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "PowerPC:LE:64:default");
    }

    #[test]
    fn selects_elf_loongarch64_lp64d() {
        let (_, spec) = select_elf_load_spec(EM_LOONGARCH, ELFCLASS64, ELFDATA2LSB, 0x43, 0)
            .expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "Loongarch:LE:64:lp64d");
    }

    #[test]
    fn rejects_endian_mismatch_for_x86_64_elf() {
        assert!(matches!(
            select_elf_load_spec(EM_X86_64, ELFCLASS64, ELFDATA2MSB, 0, 0),
            Err(ArchitectureSelectionError::UnsupportedMachine { .. })
        ));
    }

    #[test]
    fn selects_macho_apple_silicon() {
        let (_, spec) =
            select_macho_load_spec(MACHO_CPU_TYPE_ARM64, 0, true, 0).expect("select Mach-O");
        assert_eq!(spec.pair.language_id.as_str(), "AARCH64:LE:64:AppleSilicon");
    }

    #[test]
    fn checked_in_ghidra_manifest_keeps_expected_coverage() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../fission-sleigh/specs/ghidra_language_manifest.json"
        ))
        .expect("parse Ghidra language manifest");
        assert_eq!(manifest["processor_count"], 38);
        assert_eq!(manifest["variant_count"], 146);

        let entries = manifest["entries"].as_array().expect("entries array");
        for language_id in [
            "x86:LE:64:default",
            "x86:LE:32:default",
            "AARCH64:LE:64:v8A",
            "AARCH64:LE:64:AppleSilicon",
            "ARM:LE:32:v7",
        ] {
            assert!(
                entries.iter().any(|entry| {
                    entry["language_id"].as_str() == Some(language_id)
                        || entry["language_ids"]
                            .as_array()
                            .map(|ids| ids.iter().any(|id| id.as_str() == Some(language_id)))
                            .unwrap_or(false)
                }),
                "missing selectable language {language_id}"
            );
        }
    }
}
