use super::BinaryParser;
use crate::analysis::loader::types::{
    FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::core::prelude::*;

pub struct StaticParser;

impl StaticParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse PE (Windows executable)
    fn parse_pe(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Use the new binrw PE Loader as the primary parser
        use crate::analysis::loader::pe::PeLoader;

        PeLoader::parse(data, path)
    }

    /// Parse ELF (Linux executable)
    fn parse_elf(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        use crate::analysis::loader::elf::ElfLoader;
        ElfLoader::parse(data, path)
    }

    /// Parse Mach-O (macOS executable)
    fn parse_macho(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        use crate::analysis::loader::macho::MachoLoader;
        MachoLoader::parse(data, path)
    }
}

impl BinaryParser for StaticParser {
    fn parse(&self, data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        // Check magic bytes to determine format
        if data.len() < 4 {
            return Err(err!(loader, "File too small"));
        }

        // Check for PE (MZ header)
        if data.len() > 2 && data[0] == 0x4D && data[1] == 0x5A {
            let mut binary = Self::parse_pe(data, path)?;
            binary.sort_sections();
            return Ok(binary);
        }

        // Check for ELF
        if data.len() > 4 && data[0..4] == [0x7F, b'E', b'L', b'F'] {
            let mut binary = Self::parse_elf(data, path)?;
            binary.sort_sections();
            return Ok(binary);
        }

        // Check for Mach-O
        if data.len() > 4 {
            let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if magic == 0xFEEDFACE
                || magic == 0xFEEDFACF
                || magic == 0xCEFAEDFE
                || magic == 0xCFFAEDFE
            {
                let mut binary = Self::parse_macho(data, path)?;
                binary.sort_sections();
                return Ok(binary);
            }
        }

        Err(err!(loader, "Unknown binary format"))
    }
}
