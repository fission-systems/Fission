use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::select_coff_load_spec;
use fission_core::core_constants::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM, IMAGE_FILE_MACHINE_ARM64,
    IMAGE_FILE_MACHINE_I386,
};

pub struct CoffLoader;

const COFF_HEADER_SIZE: usize = 20;
const COFF_SECTION_HEADER_SIZE: usize = 40;
const COFF_SYMBOL_SIZE: usize = 18;
const COFF_OBJECT_IMAGE_BASE: u64 = 0x2000;
const COFF_EXTERNAL_IMAGE_BASE: u64 = 0xffff_1000_0000_0000;

const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;
const IMAGE_SCN_CNT_UNINITIALIZED_DATA: u32 = 0x0000_0080;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

const C_EXT: u8 = 2;
const C_STAT: u8 = 3;
const DT_FCN: u16 = 2;

#[derive(Clone, Copy, Debug)]
struct CoffHeader {
    machine: u16,
    number_of_sections: u16,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
    size_of_optional_header: u16,
}

#[derive(Clone, Debug)]
struct CoffSection {
    name: String,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    characteristics: u32,
    assigned_address: u64,
}

#[derive(Clone, Debug)]
struct CoffSymbol {
    name: String,
    value: u32,
    section_number: i16,
    symbol_type: u16,
    storage_class: u8,
    number_of_aux_symbols: u8,
}

impl CoffLoader {
    pub fn looks_like_coff_object(bytes: &[u8]) -> bool {
        if bytes.len() < COFF_HEADER_SIZE {
            return false;
        }
        if bytes.starts_with(b"MZ") || bytes.starts_with(b"\x7fELF") {
            return false;
        }
        let Some(header) = parse_header(bytes) else {
            return false;
        };
        if !matches!(
            header.machine,
            IMAGE_FILE_MACHINE_AMD64
                | IMAGE_FILE_MACHINE_I386
                | IMAGE_FILE_MACHINE_ARM
                | IMAGE_FILE_MACHINE_ARM64
        ) {
            return false;
        }
        if header.number_of_sections == 0 || header.number_of_sections > 512 {
            return false;
        }
        let section_table_end = COFF_HEADER_SIZE
            .saturating_add(header.size_of_optional_header as usize)
            .saturating_add(header.number_of_sections as usize * COFF_SECTION_HEADER_SIZE);
        if section_table_end > bytes.len() {
            return false;
        }
        if header.pointer_to_symbol_table != 0 || header.number_of_symbols != 0 {
            let sym_start = header.pointer_to_symbol_table as usize;
            let sym_size = header.number_of_symbols as usize * COFF_SYMBOL_SIZE;
            if sym_start == 0 || sym_start.saturating_add(sym_size) > bytes.len() {
                return false;
            }
        }
        true
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let header = parse_header(bytes).ok_or_else(|| err!(loader, "Invalid COFF header"))?;
        let is_64bit = header.machine == IMAGE_FILE_MACHINE_AMD64
            || header.machine == IMAGE_FILE_MACHINE_ARM64;

        let string_table = parse_string_table(bytes, &header);
        let mut sections = parse_sections(bytes, &header, &string_table)?;
        assign_section_addresses(&mut sections);
        let section_infos: Vec<SectionInfo> = sections
            .iter()
            .map(|section| SectionInfo {
                name: section.name.clone(),
                virtual_address: section.assigned_address,
                virtual_size: section.size_of_raw_data as u64,
                file_offset: section.pointer_to_raw_data as u64,
                file_size: section.size_of_raw_data as u64,
                is_executable: section.is_executable(),
                is_readable: section.is_readable(),
                is_writable: section.is_writable(),
            })
            .collect();

        let symbols = parse_symbols(bytes, &header, &string_table);
        let functions = classify_symbols(&symbols, &sections);
        let entry_point = functions
            .iter()
            .find(|function| !function.is_import)
            .map(|function| function.address)
            .unwrap_or(0);
        let (architecture, load_spec) =
            select_coff_load_spec(header.machine, is_64bit, COFF_OBJECT_IMAGE_BASE)
                .map_err(|e| err!(loader, "{}", e))?;

        LoadedBinaryBuilder::new(path, data)
            .format("COFF Object")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(COFF_OBJECT_IMAGE_BASE)
            .is_64bit(is_64bit)
            .add_sections(section_infos)
            .add_functions(functions)
            .build()
    }
}

impl CoffSection {
    fn is_executable(&self) -> bool {
        (self.characteristics & (IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE)) != 0
    }

    fn is_readable(&self) -> bool {
        (self.characteristics & IMAGE_SCN_MEM_READ) != 0 || !self.is_writable()
    }

    fn is_writable(&self) -> bool {
        (self.characteristics & IMAGE_SCN_MEM_WRITE) != 0
    }
}

fn parse_header(bytes: &[u8]) -> Option<CoffHeader> {
    if bytes.len() < COFF_HEADER_SIZE {
        return None;
    }
    Some(CoffHeader {
        machine: read_u16(bytes, 0)?,
        number_of_sections: read_u16(bytes, 2)?,
        pointer_to_symbol_table: read_u32(bytes, 8)?,
        number_of_symbols: read_u32(bytes, 12)?,
        size_of_optional_header: read_u16(bytes, 16)?,
    })
}

fn parse_sections(
    bytes: &[u8],
    header: &CoffHeader,
    string_table: &[u8],
) -> Result<Vec<CoffSection>> {
    let mut sections = Vec::new();
    let mut offset = COFF_HEADER_SIZE + header.size_of_optional_header as usize;
    for _ in 0..header.number_of_sections {
        if offset + COFF_SECTION_HEADER_SIZE > bytes.len() {
            return Err(err!(loader, "COFF section table out of bounds"));
        }
        let name = parse_coff_name(&bytes[offset..offset + 8], string_table);
        let virtual_size = read_u32(bytes, offset + 8).unwrap_or(0);
        let size_of_raw_data = read_u32(bytes, offset + 16).unwrap_or(0);
        let pointer_to_raw_data = read_u32(bytes, offset + 20).unwrap_or(0);
        let characteristics = read_u32(bytes, offset + 36).unwrap_or(0);
        let size =
            if size_of_raw_data == 0 && (characteristics & IMAGE_SCN_CNT_UNINITIALIZED_DATA) != 0 {
                virtual_size
            } else {
                size_of_raw_data
            };
        sections.push(CoffSection {
            name,
            size_of_raw_data: size,
            pointer_to_raw_data,
            characteristics,
            assigned_address: 0,
        });
        offset += COFF_SECTION_HEADER_SIZE;
    }
    Ok(sections)
}

fn assign_section_addresses(sections: &mut [CoffSection]) {
    let mut next = COFF_OBJECT_IMAGE_BASE;
    for section in sections {
        let alignment = if section.is_executable() { 0x10 } else { 0x8 };
        next = align_up(next, alignment);
        section.assigned_address = next;
        next = next.saturating_add(section.size_of_raw_data.max(1) as u64);
    }
}

fn parse_symbols(bytes: &[u8], header: &CoffHeader, string_table: &[u8]) -> Vec<CoffSymbol> {
    let mut symbols = Vec::new();
    let mut index = 0u32;
    let mut offset = header.pointer_to_symbol_table as usize;
    while index < header.number_of_symbols {
        if offset + COFF_SYMBOL_SIZE > bytes.len() {
            break;
        }
        let name = parse_coff_name(&bytes[offset..offset + 8], string_table);
        let value = read_u32(bytes, offset + 8).unwrap_or(0);
        let section_number = read_i16(bytes, offset + 12).unwrap_or(0);
        let symbol_type = read_u16(bytes, offset + 14).unwrap_or(0);
        let storage_class = bytes[offset + 16];
        let number_of_aux_symbols = bytes[offset + 17];
        symbols.push(CoffSymbol {
            name,
            value,
            section_number,
            symbol_type,
            storage_class,
            number_of_aux_symbols,
        });
        let advance = 1 + number_of_aux_symbols as u32;
        index = index.saturating_add(advance);
        offset = offset.saturating_add(advance as usize * COFF_SYMBOL_SIZE);
    }
    symbols
}

fn classify_symbols(symbols: &[CoffSymbol], sections: &[CoffSection]) -> Vec<FunctionInfo> {
    let mut functions = Vec::new();
    let mut external_index = 0u64;
    for symbol in symbols {
        if symbol.name.is_empty() || symbol.name.starts_with('.') {
            continue;
        }
        if symbol.number_of_aux_symbols > 0
            && (symbol.name == ".bf" || symbol.name == ".ef" || symbol.name == ".lf")
        {
            continue;
        }
        if symbol.section_number == 0 && symbol.storage_class == C_EXT {
            functions.push(FunctionInfo {
                name: symbol.name.clone(),
                address: COFF_EXTERNAL_IMAGE_BASE + external_index * 8,
                size: 0,
                is_export: false,
                is_import: true,
                origin: Some("coff-symbol-table".to_string()),
                kind: Some("undefined_external".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
            });
            external_index += 1;
            continue;
        }
        if symbol.section_number <= 0 {
            continue;
        }
        let section_index = (symbol.section_number - 1) as usize;
        let Some(section) = sections.get(section_index) else {
            continue;
        };
        let is_function_type = ((symbol.symbol_type >> 4) & 0x0f) == DT_FCN;
        let is_defined_code = section.is_executable()
            && matches!(symbol.storage_class, C_EXT | C_STAT)
            && (is_function_type || symbol.value < section.size_of_raw_data);
        if !is_defined_code {
            continue;
        }
        functions.push(FunctionInfo {
            name: symbol.name.clone(),
            address: section.assigned_address + symbol.value as u64,
            size: 0,
            is_export: symbol.storage_class == C_EXT,
            is_import: false,
            origin: Some("coff-symbol-table".to_string()),
            kind: Some("code".to_string()),
            source_section: Some(section.name.clone()),
            external_library: None,
            is_thunk_like: false,
        });
    }
    functions.sort_by_key(|function| (function.is_import, function.address, function.name.clone()));
    functions
        .dedup_by(|a, b| a.address == b.address && a.name == b.name && a.is_import == b.is_import);
    functions
}

fn parse_string_table<'a>(bytes: &'a [u8], header: &CoffHeader) -> &'a [u8] {
    let start = header.pointer_to_symbol_table as usize
        + header.number_of_symbols as usize * COFF_SYMBOL_SIZE;
    if start + 4 > bytes.len() {
        return &[];
    }
    let Some(size) = read_u32(bytes, start).map(|value| value as usize) else {
        return &[];
    };
    if size < 4 || start + size > bytes.len() {
        return &[];
    }
    &bytes[start..start + size]
}

fn parse_coff_name(raw: &[u8], string_table: &[u8]) -> String {
    if raw.len() == 8 && raw[0..4] == [0, 0, 0, 0] {
        let offset = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
        return extract_string_table_name(string_table, offset);
    }
    if raw.first() == Some(&b'/') {
        if let Ok(offset_text) = std::str::from_utf8(&raw[1..]) {
            let trimmed = offset_text.trim_end_matches('\0').trim();
            if let Ok(offset) = trimmed.parse::<usize>() {
                return extract_string_table_name(string_table, offset);
            }
        }
    }
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).to_string()
}

fn extract_string_table_name(string_table: &[u8], offset: usize) -> String {
    if offset >= string_table.len() {
        return String::new();
    }
    let end = string_table[offset..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|rel| offset + rel)
        .unwrap_or(string_table.len());
    String::from_utf8_lossy(&string_table[offset..end]).to_string()
}

fn align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    (value + alignment - 1) & !(alignment - 1)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
