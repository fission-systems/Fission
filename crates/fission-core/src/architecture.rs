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

#[derive(Debug, Clone, Copy)]
struct LoadSpecMapping {
    machine: u32,
    bitness: u8,
    endian: &'static str,
    processor: &'static str,
    variant: &'static str,
    abi: Option<&'static str>,
    language_id: &'static str,
    compiler_spec_id: &'static str,
    flags_mask: u32,
    flags_value: u32,
}

impl LoadSpecMapping {
    const fn exact(
        machine: u32,
        bitness: u8,
        endian: &'static str,
        processor: &'static str,
        variant: &'static str,
        abi: Option<&'static str>,
        language_id: &'static str,
        compiler_spec_id: &'static str,
    ) -> Self {
        Self {
            machine,
            bitness,
            endian,
            processor,
            variant,
            abi,
            language_id,
            compiler_spec_id,
            flags_mask: 0,
            flags_value: 0,
        }
    }

    const fn with_flags(
        machine: u32,
        bitness: u8,
        endian: &'static str,
        processor: &'static str,
        variant: &'static str,
        abi: Option<&'static str>,
        language_id: &'static str,
        compiler_spec_id: &'static str,
        flags_mask: u32,
        flags_value: u32,
    ) -> Self {
        Self {
            machine,
            bitness,
            endian,
            processor,
            variant,
            abi,
            language_id,
            compiler_spec_id,
            flags_mask,
            flags_value,
        }
    }

    fn matches(self, machine: u32, bitness: u8, endian: &str, flags: u32) -> bool {
        self.machine == machine
            && self.bitness == bitness
            && self.endian == endian
            && (flags & self.flags_mask) == self.flags_value
    }
}

const PE_LOAD_SPEC_MAPPINGS: &[LoadSpecMapping] = &[
    LoadSpecMapping::exact(
        IMAGE_FILE_MACHINE_AMD64 as u32,
        64,
        "little",
        "x86",
        "default",
        Some("windows"),
        "x86:LE:64:default",
        "windows",
    ),
    LoadSpecMapping::exact(
        IMAGE_FILE_MACHINE_I386 as u32,
        32,
        "little",
        "x86",
        "default",
        Some("windows"),
        "x86:LE:32:default",
        "windows",
    ),
    LoadSpecMapping::exact(
        IMAGE_FILE_MACHINE_ARM as u32,
        32,
        "little",
        "ARM",
        "v7",
        Some("windows"),
        "ARM:LE:32:v7",
        "windows",
    ),
    LoadSpecMapping::exact(
        IMAGE_FILE_MACHINE_ARM64 as u32,
        64,
        "little",
        "AARCH64",
        "v8A",
        Some("windows"),
        "AARCH64:LE:64:v8A",
        "windows",
    ),
];

const ELF_LOAD_SPEC_MAPPINGS: &[LoadSpecMapping] = &[
    LoadSpecMapping::exact(
        EM_X86_64 as u32,
        64,
        "little",
        "x86",
        "default",
        Some("gcc"),
        "x86:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_386 as u32,
        32,
        "little",
        "x86",
        "default",
        Some("gcc"),
        "x86:LE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_AARCH64 as u32,
        64,
        "little",
        "AARCH64",
        "v8A",
        Some("gcc"),
        "AARCH64:LE:64:v8A",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_AARCH64 as u32,
        64,
        "big",
        "AARCH64",
        "v8A",
        Some("gcc"),
        "AARCH64:BE:64:v8A",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_ARM as u32,
        32,
        "little",
        "ARM",
        "v7",
        Some("gcc"),
        "ARM:LE:32:v7",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_ARM as u32,
        32,
        "big",
        "ARM",
        "v7",
        Some("gcc"),
        "ARM:BE:32:v7",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_RISCV as u32,
        32,
        "little",
        "RISCV",
        "default",
        Some("gcc"),
        "RISCV:LE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_RISCV as u32,
        64,
        "little",
        "RISCV",
        "default",
        Some("gcc"),
        "RISCV:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::with_flags(
        EM_MIPS as u32,
        32,
        "little",
        "MIPS",
        "R6",
        Some("gcc"),
        "MIPS:LE:32:R6",
        "gcc",
        0x8000_0000,
        0x8000_0000,
    ),
    LoadSpecMapping::with_flags(
        EM_MIPS as u32,
        32,
        "big",
        "MIPS",
        "R6",
        Some("gcc"),
        "MIPS:BE:32:R6",
        "gcc",
        0x8000_0000,
        0x8000_0000,
    ),
    LoadSpecMapping::exact(
        EM_MIPS as u32,
        32,
        "little",
        "MIPS",
        "default",
        Some("gcc"),
        "MIPS:LE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_MIPS as u32,
        32,
        "big",
        "MIPS",
        "default",
        Some("gcc"),
        "MIPS:BE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_MIPS as u32,
        64,
        "little",
        "MIPS",
        "default",
        Some("gcc"),
        "MIPS:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_MIPS as u32,
        64,
        "big",
        "MIPS",
        "default",
        Some("gcc"),
        "MIPS:BE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_PPC as u32,
        32,
        "little",
        "PowerPC",
        "default",
        Some("gcc"),
        "PowerPC:LE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_PPC as u32,
        32,
        "big",
        "PowerPC",
        "default",
        Some("gcc"),
        "PowerPC:BE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_PPC64 as u32,
        64,
        "little",
        "PowerPC",
        "default",
        Some("gcc"),
        "PowerPC:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_PPC64 as u32,
        64,
        "big",
        "PowerPC",
        "default",
        Some("gcc"),
        "PowerPC:BE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_SPARCV9 as u32,
        64,
        "big",
        "sparc",
        "default",
        Some("gcc"),
        "sparc:BE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_BPF as u32,
        64,
        "little",
        "eBPF",
        "default",
        Some("gcc"),
        "eBPF:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_BPF as u32,
        64,
        "big",
        "eBPF",
        "default",
        Some("gcc"),
        "eBPF:BE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        EM_LOONGARCH as u32,
        32,
        "little",
        "Loongarch",
        "ilp32d",
        Some("gcc"),
        "Loongarch:LE:32:ilp32d",
        "gcc",
    ),
    LoadSpecMapping::with_flags(
        EM_LOONGARCH as u32,
        64,
        "little",
        "Loongarch",
        "lp64f",
        Some("gcc"),
        "Loongarch:LE:64:lp64f",
        "gcc",
        u32::MAX,
        0x42,
    ),
    LoadSpecMapping::exact(
        EM_LOONGARCH as u32,
        64,
        "little",
        "Loongarch",
        "lp64d",
        Some("gcc"),
        "Loongarch:LE:64:lp64d",
        "gcc",
    ),
];

const MACHO_LOAD_SPEC_MAPPINGS: &[LoadSpecMapping] = &[
    LoadSpecMapping::exact(
        MACHO_CPU_TYPE_X86_64 as u32,
        64,
        "little",
        "x86",
        "default",
        Some("macosx"),
        "x86:LE:64:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        MACHO_CPU_TYPE_X86 as u32,
        32,
        "little",
        "x86",
        "default",
        Some("macosx"),
        "x86:LE:32:default",
        "gcc",
    ),
    LoadSpecMapping::exact(
        MACHO_CPU_TYPE_ARM64 as u32,
        64,
        "little",
        "AARCH64",
        "AppleSilicon",
        Some("macosx"),
        "AARCH64:LE:64:AppleSilicon",
        "default",
    ),
    LoadSpecMapping::exact(
        MACHO_CPU_TYPE_ARM as u32,
        32,
        "little",
        "ARM",
        "v7",
        Some("macosx"),
        "ARM:LE:32:v7",
        "default",
    ),
];

fn lookup_mapping(
    mappings: &[LoadSpecMapping],
    machine: u32,
    bitness: u8,
    endian: &str,
    flags: u32,
) -> Option<LoadSpecMapping> {
    mappings
        .iter()
        .copied()
        .find(|mapping| mapping.matches(machine, bitness, endian, flags))
}

pub fn select_pe_load_spec(
    machine: u16,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = if is_64bit { 64 } else { 32 };
    if let Some(mapping) = lookup_mapping(
        PE_LOAD_SPEC_MAPPINGS,
        u32::from(machine),
        bitness,
        "little",
        0,
    ) {
        Ok(selection_from_mapping(
            "PE",
            image_base,
            mapping,
            format!("PE Machine=0x{machine:04x}"),
        ))
    } else {
        Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "PE".to_string(),
            machine: format!("Machine=0x{machine:04x}, is_64bit={is_64bit}"),
        })
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

    if let Some(mapping) = lookup_mapping(
        ELF_LOAD_SPEC_MAPPINGS,
        u32::from(machine),
        bitness,
        endian,
        flags,
    ) {
        Ok(selection_from_mapping(
            "ELF",
            image_base,
            mapping,
            format!("ELF e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"),
        ))
    } else {
        Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "ELF".to_string(),
            machine: format!(
                "class={class}, data_encoding={data_encoding}, machine=0x{machine:04x}, flags=0x{flags:08x}"
            ),
        })
    }
}

pub fn select_macho_load_spec(
    cputype: i32,
    cpusubtype: i32,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = if is_64bit { 64 } else { 32 };
    if let Some(mapping) = lookup_mapping(
        MACHO_LOAD_SPEC_MAPPINGS,
        cputype as u32,
        bitness,
        "little",
        0,
    ) {
        Ok(selection_from_mapping(
            "Mach-O",
            image_base,
            mapping,
            format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}"),
        ))
    } else {
        Err(ArchitectureSelectionError::UnsupportedMachine {
            format: "Mach-O".to_string(),
            machine: format!(
                "cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}, is_64bit={is_64bit}"
            ),
        })
    }
}

fn selection_from_mapping(
    format: &str,
    image_base: u64,
    mapping: LoadSpecMapping,
    raw_machine: String,
) -> (ArchitectureDescriptor, BinaryLoadSpec) {
    let architecture = ArchitectureDescriptor::new(
        mapping.processor,
        mapping.endian,
        mapping.bitness,
        mapping.variant,
        mapping.abi.map(str::to_string),
        raw_machine.clone(),
    );
    let load_spec = BinaryLoadSpec::new(
        format,
        image_base,
        mapping.language_id,
        mapping.compiler_spec_id,
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
