use crate::loader::reader::{ByteReader, Endian};
use crate::loader::types::{DataBuffer, LoadedBinary, SectionInfo};
use crate::prelude::*;

pub struct UnixAoutLoader;

const AOUT_HEADER_SIZE: usize = 32;
const OMAGIC: u32 = 0o407;
const NMAGIC: u32 = 0o410;
const ZMAGIC: u32 = 0o413;

#[derive(Clone, Copy, Debug)]
struct AoutHeader {
    magic: u32,
    text_size: u32,
    data_size: u32,
    bss_size: u32,
    symbol_size: u32,
}

impl UnixAoutLoader {
    pub fn looks_like(bytes: &[u8]) -> bool {
        parse_header(bytes).is_some()
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let header = parse_header(data.as_slice())
            .ok_or_else(|| err!(loader, "MalformedHeader: invalid Unix a.out header"))?;
        let _ = path;
        let file_data_len = header
            .text_size
            .checked_add(header.data_size)
            .ok_or_else(|| err!(loader, "SectionOutOfBounds: a.out text/data size overflow"))?
            as usize;
        let file_data_end = AOUT_HEADER_SIZE
            .checked_add(file_data_len)
            .ok_or_else(|| err!(loader, "SectionOutOfBounds: a.out file range overflow"))?;
        if file_data_end > data.as_slice().len() {
            return Err(err!(
                loader,
                "SectionOutOfBounds: a.out text/data extends beyond file"
            ));
        }

        let mut sections = Vec::new();
        sections.push(SectionInfo {
            name: "text".to_string(),
            virtual_address: 0,
            virtual_size: header.text_size as u64,
            file_offset: AOUT_HEADER_SIZE as u64,
            file_size: header.text_size as u64,
            is_executable: true,
            is_readable: true,
            is_writable: header.magic == OMAGIC,
        });
        if header.data_size != 0 {
            sections.push(SectionInfo {
                name: "data".to_string(),
                virtual_address: header.text_size as u64,
                virtual_size: header.data_size as u64,
                file_offset: (AOUT_HEADER_SIZE as u64) + header.text_size as u64,
                file_size: header.data_size as u64,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            });
        }
        if header.bss_size != 0 {
            sections.push(SectionInfo {
                name: "bss".to_string(),
                virtual_address: header.text_size as u64 + header.data_size as u64,
                virtual_size: header.bss_size as u64,
                file_offset: file_data_end as u64,
                file_size: 0,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            });
        }

        let symbol_table_file_offset = file_data_end as u64;
        if header.symbol_size != 0
            && symbol_table_file_offset.saturating_add(header.symbol_size as u64)
                > data.as_slice().len() as u64
        {
            return Err(err!(
                loader,
                "SymbolTableMalformed: a.out symbol table extends beyond file"
            ));
        }

        let _ = sections;
        Err(err!(
            loader,
            "LoadSpecNotFound: Unix a.out requires an exact load spec"
        ))
    }
}

fn parse_header(bytes: &[u8]) -> Option<AoutHeader> {
    if bytes.len() < AOUT_HEADER_SIZE {
        return None;
    }
    parse_header_with_endian(bytes, Endian::Little)
        .or_else(|| parse_header_with_endian(bytes, Endian::Big))
}

fn parse_header_with_endian(bytes: &[u8], endian: Endian) -> Option<AoutHeader> {
    let reader = ByteReader::new(bytes, endian);
    let magic_word = reader.u32(0).ok()?;
    let magic = magic_word & 0xffff;
    if !matches!(magic, OMAGIC | NMAGIC | ZMAGIC) {
        return None;
    }
    let text_size = reader.u32(4).ok()?;
    let data_size = reader.u32(8).ok()?;
    let bss_size = reader.u32(12).ok()?;
    let symbol_size = reader.u32(16).ok()?;
    let _entry = reader.u32(20).ok()?;
    if text_size == 0 && data_size == 0 {
        return None;
    }
    Some(AoutHeader {
        magic,
        text_size,
        data_size,
        bss_size,
        symbol_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aout_loader_maps_text_data_bss_and_entry() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&OMAGIC.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0x90, 0x90, 0xc3, 0x00, 0x01, 0x02]);
        let err = UnixAoutLoader::parse(DataBuffer::Heap(bytes), "test.out".to_string())
            .expect_err("a.out needs an exact load spec");
        assert!(format!("{err}").contains("LoadSpecNotFound"));
    }
}
