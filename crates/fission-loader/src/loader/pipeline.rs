use super::{
    DataBuffer, LoadedBinary, coff, containers, elf,
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
    PefLoader,
    SomLoader,
    OmfLoader,
    Omf51Loader,
    GzfLoader,
    DbgLoader,
    DefLoader,
    MapLoader,
    GdtLoader,
    XmlLoader,
    DecompileDebugXmlLoader,
}

impl KnownUnsupportedLoaderFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::DyldCacheLoader => "DyldCacheLoader",
            Self::PefLoader => "PefLoader",
            Self::SomLoader => "SomLoader",
            Self::OmfLoader => "OmfLoader",
            Self::Omf51Loader => "Omf51Loader",
            Self::GzfLoader => "GzfLoader",
            Self::DbgLoader => "DbgLoader",
            Self::DefLoader => "DefLoader",
            Self::MapLoader => "MapLoader",
            Self::GdtLoader => "GdtLoader",
            Self::XmlLoader => "XmlLoader",
            Self::DecompileDebugXmlLoader => "DecompileDebugXmlLoader",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderRoute {
    Executable(DetectedFormat),
    Container(containers::ContainerFormat),
    KnownUnsupported(KnownUnsupportedLoaderFamily),
}

/// Ghidra-style loader pipeline entrypoint.
///
/// The pipeline owns detection and routing. Individual format loaders own
/// parsing, mapping, symbol classification, and final `LoadedBinary` creation.
pub struct LoaderPipeline;

impl LoaderPipeline {
    pub fn load(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let format = match Self::route(data.as_slice())? {
            LoaderRoute::Executable(format) => format,
            LoaderRoute::Container(container) => {
                return Err(FissionError::loader(format!(
                    "ContainerRequiresExtraction({})",
                    container.as_str()
                )));
            }
            LoaderRoute::KnownUnsupported(loader_family) => {
                return Err(FissionError::loader(format!(
                    "UnsupportedLoaderFamily({})",
                    loader_family.as_str()
                )));
            }
        };
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
        match Self::route(bytes)? {
            LoaderRoute::Executable(format) => Ok(format),
            LoaderRoute::Container(container) => Err(FissionError::loader(format!(
                "ContainerRequiresExtraction({})",
                container.as_str()
            ))),
            LoaderRoute::KnownUnsupported(loader_family) => Err(FissionError::loader(format!(
                "UnsupportedLoaderFamily({})",
                loader_family.as_str()
            ))),
        }
    }

    pub fn route(bytes: &[u8]) -> Result<LoaderRoute> {
        if bytes.len() < 4 {
            return Err(FissionError::loader("Binary too small"));
        }
        if looks_like_pe(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::Pe));
        }
        if bytes.starts_with(b"\x7fELF") {
            return Ok(LoaderRoute::Executable(DetectedFormat::Elf));
        }
        if coff::CoffLoader::looks_like_coff_object(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::Coff));
        }
        if looks_like_macho(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::MachO));
        }
        if hex::IntelHexLoader::looks_like(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::IntelHex));
        }
        if hex::MotorolaHexLoader::looks_like(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::MotorolaHex));
        }
        if mz_ne::NeLoader::looks_like(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::Ne));
        }
        if mz_ne::MzLoader::looks_like(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::Mz));
        }
        if aout::UnixAoutLoader::looks_like(bytes) {
            return Ok(LoaderRoute::Executable(DetectedFormat::UnixAout));
        }
        if let Some(container) = containers::detect_container(bytes)? {
            return Ok(LoaderRoute::Container(container));
        }
        if let Some(loader_family) = known_unsupported_loader_family(bytes) {
            return Ok(LoaderRoute::KnownUnsupported(loader_family));
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
    if looks_like_gzf(bytes) {
        return Some(KnownUnsupportedLoaderFamily::GzfLoader);
    }
    if bytes.starts_with(b"dyld_v1") {
        return Some(KnownUnsupportedLoaderFamily::DyldCacheLoader);
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

fn looks_like_gzf(bytes: &[u8]) -> bool {
    const ITEM_SERIALIZER_MAGIC_OFFSET: usize = 6;
    const ITEM_SERIALIZER_MAGIC: [u8; 8] = 0x2e30212634e92c20u64.to_be_bytes();
    bytes
        .get(
            ITEM_SERIALIZER_MAGIC_OFFSET
                ..ITEM_SERIALIZER_MAGIC_OFFSET + ITEM_SERIALIZER_MAGIC.len(),
        )
        .is_some_and(|magic| magic == ITEM_SERIALIZER_MAGIC)
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
        assert_eq!(
            LoaderPipeline::route(&bytes).unwrap(),
            LoaderRoute::Executable(DetectedFormat::Pe)
        );
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
    fn rejects_gzip_container_with_typed_message() {
        let err = LoaderPipeline::detect(&[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0])
            .expect_err("gzip containers are not executable images");
        assert!(format!("{err}").contains("ContainerRequiresExtraction(Gzip)"));
    }

    #[test]
    fn routes_xz_and_unix_archive_as_containers() {
        let err = LoaderPipeline::detect(&[0xfd, b'7', b'z', b'X', b'Z', 0x00])
            .expect_err("xz containers are not executable images");
        assert!(format!("{err}").contains("ContainerRequiresExtraction(Xz)"));

        let err = LoaderPipeline::detect(b"!<arch>\nmember")
            .expect_err("archives are not direct executable images");
        assert!(format!("{err}").contains("ContainerRequiresExtraction(UnixArchive)"));
    }

    #[test]
    fn routes_gzf_as_known_unsupported_loader_family() {
        let mut bytes = vec![0u8; 14];
        bytes[6..14].copy_from_slice(&0x2e30212634e92c20u64.to_be_bytes());
        let err = LoaderPipeline::detect(&bytes).expect_err("gzf is not executable image input");
        assert!(format!("{err}").contains("UnsupportedLoaderFamily(GzfLoader)"));
    }

    #[test]
    fn routes_compound_document_as_container() {
        let mut bytes = vec![0u8; 1536];
        bytes[0..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
        bytes[0x18..0x1a].copy_from_slice(&0x003eu16.to_le_bytes());
        bytes[0x1a..0x1c].copy_from_slice(&3u16.to_le_bytes());
        bytes[0x1c..0x1e].copy_from_slice(&0xfffeu16.to_le_bytes());
        bytes[0x1e..0x20].copy_from_slice(&9u16.to_le_bytes());
        bytes[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        bytes[0x2c..0x30].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&4096u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&0xffff_fffeu32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&0xffff_fffeu32.to_le_bytes());

        assert_eq!(
            LoaderPipeline::route(&bytes).unwrap(),
            LoaderRoute::Container(containers::ContainerFormat::CompoundDocument)
        );
        let err = LoaderPipeline::detect(&bytes).expect_err("CFB is not executable");
        assert!(format!("{err}").contains("ContainerRequiresExtraction(CompoundDocument)"));
    }
}
