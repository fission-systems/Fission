use crate::loader::formats::hex::generic_unknown_load_spec;
use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;

pub struct MzLoader;
pub struct NeLoader;

impl MzLoader {
    pub fn looks_like(bytes: &[u8]) -> bool {
        bytes.starts_with(b"MZ") && !NeLoader::looks_like(bytes) && !looks_like_pe(bytes)
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        if !Self::looks_like(bytes) {
            return Err(err!(loader, "MalformedHeader: invalid MZ executable"));
        }
        let reader = ByteReader::little(bytes);
        let last_page_bytes = reader.u16(0x02)? as u64;
        let pages = reader.u16(0x04)? as u64;
        let header_paragraphs = reader.u16(0x08)? as u64;
        let ip = reader.u16(0x14)? as u64;
        let cs = reader.u16(0x16)? as u64;
        let header_size = header_paragraphs.saturating_mul(16);
        let file_size = mz_file_size(bytes.len() as u64, pages, last_page_bytes);
        let image_size = file_size.saturating_sub(header_size);
        let entry_point = cs.saturating_mul(16).saturating_add(ip);
        let (architecture, load_spec) = generic_unknown_load_spec("MZ", 0);

        let section = SectionInfo {
            name: "dos_image".to_string(),
            virtual_address: 0,
            virtual_size: image_size,
            file_offset: header_size,
            file_size: image_size.min(bytes.len().saturating_sub(header_size as usize) as u64),
            is_executable: true,
            is_readable: true,
            is_writable: true,
        };
        let function = FunctionInfo {
            name: "entry".to_string(),
            address: entry_point,
            size: 0,
            is_export: false,
            is_import: false,
            origin: Some("mz-entry".to_string()),
            kind: Some("entry".to_string()),
            source_section: Some("dos_image".to_string()),
            external_library: None,
            is_thunk_like: false,
        };

        LoadedBinaryBuilder::new(path, data)
            .format("MZ")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(0)
            .is_64bit(false)
            .add_section(section)
            .add_function(function)
            .build()
    }
}

impl NeLoader {
    pub fn looks_like(bytes: &[u8]) -> bool {
        has_dos_extended_header(bytes, b"NE")
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        if !Self::looks_like(bytes) {
            return Err(err!(loader, "MalformedHeader: invalid NE executable"));
        }
        let reader = ByteReader::little(bytes);
        let ne_offset = reader.u32(0x3c)? as usize;
        let segment_count = reader.u16(ne_offset + 0x1c)? as usize;
        let segment_table_offset = reader.u16(ne_offset + 0x22)? as usize;
        let alignment_shift = reader.u16(ne_offset + 0x32).unwrap_or(0).min(31);
        let cs = reader.u16(ne_offset + 0x14).unwrap_or(0) as u64;
        let ip = reader.u16(ne_offset + 0x16).unwrap_or(0) as u64;
        let entry_point = cs
            .saturating_sub(1)
            .saturating_mul(0x10000)
            .saturating_add(ip);
        let segments = parse_ne_segments(
            &reader,
            ne_offset + segment_table_offset,
            segment_count,
            alignment_shift,
        )?;
        let (architecture, load_spec) = generic_unknown_load_spec("NE", 0);
        let functions = if entry_point != 0 {
            vec![FunctionInfo {
                name: "entry".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
                origin: Some("ne-entry".to_string()),
                kind: Some("entry".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
            }]
        } else {
            Vec::new()
        };

        LoadedBinaryBuilder::new(path, data)
            .format("NE")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(0)
            .is_64bit(false)
            .add_sections(segments)
            .add_functions(functions)
            .build()
    }
}

fn parse_ne_segments(
    reader: &ByteReader<'_>,
    mut offset: usize,
    count: usize,
    alignment_shift: u16,
) -> Result<Vec<SectionInfo>> {
    let mut sections = Vec::new();
    for index in 0..count {
        let sector = reader.u16(offset)? as u64;
        let length = reader.u16(offset + 2)? as u64;
        let flags = reader.u16(offset + 4)?;
        let file_offset = sector << alignment_shift;
        let size = if length == 0 { 0x10000 } else { length };
        sections.push(SectionInfo {
            name: format!("segment_{:02}", index + 1),
            virtual_address: (index as u64) << 16,
            virtual_size: size,
            file_offset,
            file_size: size,
            is_executable: (flags & 0x0008) == 0,
            is_readable: true,
            is_writable: (flags & 0x0001) != 0,
        });
        offset += 8;
    }
    Ok(sections)
}

fn mz_file_size(actual_size: u64, pages: u64, last_page_bytes: u64) -> u64 {
    if pages == 0 {
        return actual_size;
    }
    let declared = (pages - 1)
        .saturating_mul(512)
        .saturating_add(if last_page_bytes == 0 {
            512
        } else {
            last_page_bytes
        });
    declared.min(actual_size)
}

fn has_dos_extended_header(bytes: &[u8], signature: &[u8; 2]) -> bool {
    if bytes.len() <= 0x3f || !bytes.starts_with(b"MZ") {
        return false;
    }
    let reader = ByteReader::new(bytes, Endian::Little);
    let Ok(offset) = reader.u32(0x3c) else {
        return false;
    };
    offset
        .checked_add(2)
        .and_then(|end| bytes.get(offset as usize..end as usize))
        .is_some_and(|sig| sig == signature)
}

fn looks_like_pe(bytes: &[u8]) -> bool {
    has_dos_extended_header(bytes, b"PE")
        && bytes.get(
            ByteReader::little(bytes)
                .u32(0x3c)
                .unwrap_or(usize::MAX as u32) as usize
                + 2,
        ) == Some(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_mz() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x60];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x02..0x04].copy_from_slice(&0x60u16.to_le_bytes());
        bytes[0x04..0x06].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x08..0x0a].copy_from_slice(&4u16.to_le_bytes());
        bytes[0x14..0x16].copy_from_slice(&0x10u16.to_le_bytes());
        bytes[0x16..0x18].copy_from_slice(&0u16.to_le_bytes());
        bytes
    }

    #[test]
    fn mz_loader_maps_entry() {
        let binary = MzLoader::parse(DataBuffer::Heap(minimal_mz()), "test.exe".to_string())
            .expect("load mz");
        assert_eq!(binary.format, "MZ");
        assert_eq!(binary.entry_point, 0x10);
        assert_eq!(binary.sections[0].file_offset, 0x40);
    }

    #[test]
    fn ne_loader_detects_extended_header() {
        let mut bytes = minimal_mz();
        bytes.resize(0x100, 0);
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x82].copy_from_slice(b"NE");
        bytes[0x80 + 0x1c..0x80 + 0x1e].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x80 + 0x22..0x80 + 0x24].copy_from_slice(&0x40u16.to_le_bytes());
        bytes[0x80 + 0x32..0x80 + 0x34].copy_from_slice(&4u16.to_le_bytes());
        bytes[0xc0..0xc2].copy_from_slice(&8u16.to_le_bytes());
        bytes[0xc2..0xc4].copy_from_slice(&0x20u16.to_le_bytes());
        let binary =
            NeLoader::parse(DataBuffer::Heap(bytes), "test.ne".to_string()).expect("load ne");
        assert_eq!(binary.format, "NE");
        assert_eq!(binary.sections.len(), 1);
        assert_eq!(binary.sections[0].file_offset, 0x80);
    }
}
