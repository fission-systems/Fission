use super::{
    DataBuffer, LoadedBinary, coff, elf,
    formats::{aout, hex, mz_ne},
    macho, pe,
};
use crate::prelude::*;
use fission_core::constants::binary_format::{
    MACHO_FAT_CIGAM, MACHO_FAT_MAGIC, MACHO_MAGIC_32_BE, MACHO_MAGIC_32_LE, MACHO_MAGIC_64_BE,
    MACHO_MAGIC_64_LE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedFormat {
    Pe,
    Coff,
    Elf,
    MachO,
    IntelHex,
    MotorolaHex,
    Mz,
    Ne,
    UnixAout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownUnsupportedLoaderFamily {
    DyldCacheLoader,
    GzfLoader,
    PefLoader,
    SomLoader,
    OmfLoader,
}

impl KnownUnsupportedLoaderFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::DyldCacheLoader => "DyldCacheLoader",
            Self::GzfLoader => "GzfLoader",
            Self::PefLoader => "PefLoader",
            Self::SomLoader => "SomLoader",
            Self::OmfLoader => "OmfLoader",
        }
    }
}

/// Ghidra-style loader pipeline entrypoint.
///
/// The pipeline owns detection and routing. Individual format loaders own
/// parsing, mapping, symbol classification, and final `LoadedBinary` creation.
pub struct LoaderPipeline;

impl LoaderPipeline {
    pub fn load(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let format = Self::detect(data.as_slice())?;
        match format {
            DetectedFormat::Pe => pe::PeLoader::parse(data, path),
            DetectedFormat::Coff => coff::CoffLoader::parse(data, path),
            DetectedFormat::Elf => elf::ElfLoader::parse(data, path),
            DetectedFormat::MachO => macho::MachoLoader::parse(data, path),
            DetectedFormat::IntelHex => hex::IntelHexLoader::parse(data, path),
            DetectedFormat::MotorolaHex => hex::MotorolaHexLoader::parse(data, path),
            DetectedFormat::Mz => mz_ne::MzLoader::parse(data, path),
            DetectedFormat::Ne => mz_ne::NeLoader::parse(data, path),
            DetectedFormat::UnixAout => aout::UnixAoutLoader::parse(data, path),
        }
    }

    pub fn detect(bytes: &[u8]) -> Result<DetectedFormat> {
        if bytes.len() < 4 {
            return Err(FissionError::loader("Binary too small"));
        }
        if looks_like_pe(bytes) {
            return Ok(DetectedFormat::Pe);
        }
        if bytes.starts_with(b"\x7fELF") {
            return Ok(DetectedFormat::Elf);
        }
        if coff::CoffLoader::looks_like_coff_object(bytes) {
            return Ok(DetectedFormat::Coff);
        }
        if looks_like_macho(bytes) {
            return Ok(DetectedFormat::MachO);
        }
        if hex::IntelHexLoader::looks_like(bytes) {
            return Ok(DetectedFormat::IntelHex);
        }
        if hex::MotorolaHexLoader::looks_like(bytes) {
            return Ok(DetectedFormat::MotorolaHex);
        }
        if mz_ne::NeLoader::looks_like(bytes) {
            return Ok(DetectedFormat::Ne);
        }
        if mz_ne::MzLoader::looks_like(bytes) {
            return Ok(DetectedFormat::Mz);
        }
        if aout::UnixAoutLoader::looks_like(bytes) {
            return Ok(DetectedFormat::UnixAout);
        }
        if let Some(loader_family) = known_unsupported_loader_family(bytes) {
            return Err(FissionError::loader(format!(
                "UnsupportedLoaderFamily({})",
                loader_family.as_str()
            )));
        }
        Err(FissionError::loader(
            "UnsupportedFormat: unknown binary format",
        ))
    }
}

fn looks_like_pe(bytes: &[u8]) -> bool {
    if bytes.len() <= 0x3f || !bytes.starts_with(b"MZ") {
        return false;
    }
    let pe_offset =
        u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
    pe_offset
        .checked_add(4)
        .and_then(|end| bytes.get(pe_offset..end))
        .is_some_and(|sig| sig == b"PE\0\0")
}

fn looks_like_macho(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let magic = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    matches!(
        magic,
        MACHO_MAGIC_32_BE
            | MACHO_MAGIC_64_BE
            | MACHO_MAGIC_32_LE
            | MACHO_MAGIC_64_LE
            | MACHO_FAT_MAGIC
            | MACHO_FAT_CIGAM
    )
}

fn known_unsupported_loader_family(bytes: &[u8]) -> Option<KnownUnsupportedLoaderFamily> {
    if bytes.starts_with(b"dyld_v1") {
        return Some(KnownUnsupportedLoaderFamily::DyldCacheLoader);
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Some(KnownUnsupportedLoaderFamily::GzfLoader);
    }
    if bytes.starts_with(&[0x4a, 0x6f, 0x79, 0x21]) {
        return Some(KnownUnsupportedLoaderFamily::PefLoader);
    }
    if bytes.starts_with(&[0x02, 0x10, 0x01, 0x07]) || bytes.starts_with(&[0x07, 0x01, 0x10, 0x02])
    {
        return Some(KnownUnsupportedLoaderFamily::SomLoader);
    }
    if bytes.starts_with(b"OMF") {
        return Some(KnownUnsupportedLoaderFamily::OmfLoader);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pe_before_coff() {
        let mut bytes = vec![0u8; 0x80];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
        assert_eq!(LoaderPipeline::detect(&bytes).unwrap(), DetectedFormat::Pe);
    }

    #[test]
    fn rejects_unknown_format_with_typed_message() {
        let err = LoaderPipeline::detect(b"not-a-binary").expect_err("unknown must fail");
        assert!(format!("{err}").contains("UnsupportedFormat"));
    }

    #[test]
    fn detects_intel_hex_as_implemented_format() {
        assert_eq!(
            LoaderPipeline::detect(b":00000001FF\n").unwrap(),
            DetectedFormat::IntelHex
        );
    }

    #[test]
    fn rejects_known_unsupported_loader_family_with_typed_message() {
        let err = LoaderPipeline::detect(&[0x1f, 0x8b, 0x08, 0x00])
            .expect_err("gzip-backed GzfLoader is not loaded yet");
        assert!(format!("{err}").contains("UnsupportedLoaderFamily(GzfLoader)"));
    }
}
