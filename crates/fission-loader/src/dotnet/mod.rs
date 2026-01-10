//! .NET / CLR support: metadata reader and IL disassembler.
//!
//! This module builds on the existing PE loader to understand CLR metadata
//! and provide human-readable IL listings for managed methods.

use thiserror::Error;

use crate::loader::LoadedBinary;
use fission_core::errors::{FissionError, Result};

pub mod il_disasm;
pub mod metadata;

pub use il_disasm::{ILInstruction, IlDisassembler};
pub use metadata::{DotNetField, DotNetMetadata, DotNetMethod, DotNetType};

/// Errors produced while parsing CLR metadata or IL.
#[derive(Error, Debug)]
pub enum DotNetError {
    #[error("CLR runtime header not found")]
    MissingClr,
    #[error("Unsupported format for .NET parsing")]
    UnsupportedFormat,
    #[error("Malformed .NET metadata: {0}")]
    Malformed(String),
    #[error("PE parse error: {0}")]
    Pe(String),
}

pub type DotNetResult<T> = std::result::Result<T, DotNetError>;

/// Parse CLR metadata from a loaded PE binary.
pub fn parse_dotnet_metadata(binary: &LoadedBinary) -> Result<DotNetMetadata> {
    if binary.format.starts_with("ELF") || binary.format.starts_with("Mach-O") {
        return Err(FissionError::analysis(
            "Unsupported format for .NET parsing",
        ));
    }
    if !binary.is_dotnet {
        return Err(FissionError::analysis("CLR runtime header not found"));
    }

    // Use binrw to parse PE headers to find CLR directory
    use crate::loader::pe::schema::{OptionalHeader, PeFile};
    use binrw::BinRead;
    use std::io::Cursor;

    let mut cursor = Cursor::new(&binary.data);
    let pe_file = PeFile::read_le(&mut cursor)
        .map_err(|e| FissionError::analysis(format!("Parsing PE headers: {}", e)))?;

    let (clr_rva, clr_size) = match pe_file.nt_headers.optional_header {
        OptionalHeader::Pe32(opt) => {
            if let Some(dir) = opt.data_directories.get(14) {
                (dir.virtual_address, dir.size)
            } else {
                (0, 0)
            }
        }
        OptionalHeader::Pe32Plus(opt) => {
            if let Some(dir) = opt.data_directories.get(14) {
                (dir.virtual_address, dir.size)
            } else {
                (0, 0)
            }
        }
    };

    if clr_rva == 0 || clr_size == 0 {
        return Err(FissionError::analysis("CLR runtime header not found"));
    }

    // Read COR20 header to get metadata RVA
    let cor20_offset = rva_to_offset(binary, clr_rva)
        .ok_or_else(|| FissionError::analysis("Cannot map COR20 header RVA"))?;

    // COR20 header: at offset 8 is the metadata RVA (4 bytes) and size (4 bytes)
    let metadata_rva = u32::from_le_bytes([
        binary.data[cor20_offset + 8],
        binary.data[cor20_offset + 9],
        binary.data[cor20_offset + 10],
        binary.data[cor20_offset + 11],
    ]);
    let metadata_size = u32::from_le_bytes([
        binary.data[cor20_offset + 12],
        binary.data[cor20_offset + 13],
        binary.data[cor20_offset + 14],
        binary.data[cor20_offset + 15],
    ]) as usize;

    let offset = rva_to_offset(binary, metadata_rva)
        .ok_or_else(|| FissionError::analysis("Unable to map metadata RVA"))?;
    let end = offset
        .checked_add(metadata_size)
        .ok_or_else(|| FissionError::analysis("Overflow computing metadata span"))?;
    let bytes = binary
        .data
        .get(offset..end)
        .ok_or_else(|| FissionError::analysis("Metadata span outside file"))?;

    let runtime_version = binary.dotnet_runtime_version.clone();

    metadata::parse_metadata(bytes, runtime_version)
        .map_err(|e| FissionError::analysis(format!("Parsing CLR metadata: {}", e)))
}

/// Disassemble a managed method body starting at the provided RVA (relative virtual address).
pub fn disassemble_method_rva(binary: &LoadedBinary, rva: u32) -> Result<Vec<ILInstruction>> {
    if !binary.is_dotnet {
        return Err(FissionError::analysis("CLR runtime header not found"));
    }

    let offset = rva_to_offset(binary, rva).ok_or_else(|| {
        FissionError::analysis(format!("Cannot map method RVA 0x{rva:x} to file offset"))
    })?;

    let il_data = binary
        .data
        .get(offset..)
        .ok_or_else(|| FissionError::analysis("Method RVA beyond file bounds"))?;

    let dis = IlDisassembler::new();
    dis.disassemble(il_data)
        .map_err(|e| FissionError::analysis(format!("Disassembling IL body: {}", e)))
}

fn rva_to_offset(binary: &LoadedBinary, rva: u32) -> Option<usize> {
    for section in &binary.sections {
        // LoadedBinary sections logic (assuming virtual_address is absolute VA, need to handle RVA)
        // Or LoadedBinary might store ImageBase + RVA?
        // LoadedBinary.sections[i].virtual_address IS the absolute address (ImageBase + RVA).
        // BUT section.virtual_address in `PeLoader` was calculated as `image_base + section.virtual_address`.

        let start = section.virtual_address;
        let size = section.virtual_size.max(section.file_size); // Covering max range

        // Convert RVA to VA
        let rva_va = binary.image_base + rva as u64;

        if rva_va >= start && rva_va < start + size {
            let delta = rva_va - start;
            // Check file bounds
            if delta < section.file_size {
                return Some((section.file_offset + delta) as usize);
            }
        }
    }
    None
}
