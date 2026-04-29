use crate::loader::reader::ByteReader;
use crate::loader::types::{
    DataBuffer, LoadedBinary, LoadedBinaryBuilder, PdbDebugInfo, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use fission_core::architecture::select_pe_load_spec;

mod coff;
mod imports;
mod pdata;

pub struct PeLoader;

const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;
const PE_SIGNATURE_SIZE: usize = 4;
const PE_FILE_HEADER_SIZE: usize = 20;
const PE_SECTION_HEADER_SIZE: usize = 40;
const PE32_MAGIC: u16 = 0x10b;
const PE32_PLUS_MAGIC: u16 = 0x20b;
const IMAGE_DEBUG_DIRECTORY_SIZE: usize = 28;

#[derive(Clone, Debug)]
struct RawPeFile {
    file_header: PeFileHeader,
    optional_header: PeOptionalHeader,
    section_headers: Vec<PeSectionHeader>,
}

#[derive(Clone, Debug)]
struct PeFileHeader {
    machine: u16,
    pointer_to_symbol_table: u32,
    number_of_symbols: u32,
}

#[derive(Clone, Debug)]
enum PeOptionalHeader {
    Pe32(PeOptionalHeaderData),
    Pe32Plus(PeOptionalHeaderData),
}

#[derive(Clone, Debug)]
struct PeOptionalHeaderData {
    image_base: u64,
    address_of_entry_point: u32,
    section_alignment: u32,
    data_directories: Vec<DataDirectory>,
}

#[derive(Clone, Debug)]
struct PeSectionHeader {
    name: String,
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    characteristics: u32,
}

#[derive(Clone, Debug)]
struct DataDirectory {
    virtual_address: u32,
    size: u32,
}

#[derive(Clone, Debug)]
struct ExportDirectory {
    number_of_functions: u32,
    number_of_names: u32,
    address_of_functions: u32,
    address_of_names: u32,
    address_of_name_ordinals: u32,
}

#[derive(Clone, Debug)]
struct ImportDescriptor {
    original_first_thunk: u32,
    name: u32,
    first_thunk: u32,
}

#[derive(Clone, Debug)]
struct ImageDebugDirectory {
    debug_type: u32,
    size_of_data: u32,
    address_of_raw_data: u32,
    pointer_to_raw_data: u32,
}

#[derive(Clone, Debug)]
struct CoffSymbol {
    name: SymbolName,
    value: u32,
    section_number: i16,
    symbol_type: u16,
    storage_class: u8,
    number_of_aux_symbols: u8,
}

#[derive(Clone, Debug)]
enum SymbolName {
    ShortName(String),
    LongName(u32),
}

mod storage_class {
    pub const C_EXT: u8 = 2;
    pub const C_STAT: u8 = 3;
}

mod symbol_type {
    pub const DT_FCN: u16 = 2;
}

impl PeLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let pe_file = parse_pe_file(bytes)?;

        // Extract basic info
        let is_64bit = match pe_file.optional_header {
            PeOptionalHeader::Pe32(_) => false,
            PeOptionalHeader::Pe32Plus(_) => true,
        };

        let (image_base, entry_point, _section_alignment) = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => (
                opt.image_base,
                opt.image_base + opt.address_of_entry_point as u64,
                opt.section_alignment,
            ),
        };

        let (architecture, load_spec) =
            select_pe_load_spec(pe_file.file_header.machine, is_64bit, image_base)
                .map_err(|e| err!(loader, "{}", e))?;

        // Sections
        let mut sections_info = Vec::new();
        for section in pe_file.section_headers {
            let ch = section.characteristics;
            sections_info.push(SectionInfo {
                name: section.name,
                virtual_address: image_base + section.virtual_address as u64,
                virtual_size: section.virtual_size as u64,
                file_offset: section.pointer_to_raw_data as u64,
                file_size: section.size_of_raw_data as u64,
                is_executable: (ch & 0x20000000) != 0,
                is_readable: (ch & 0x40000000) != 0,
                is_writable: (ch & 0x80000000) != 0,
            });
        }

        // Build the binary
        let loader = PeLoaderImpl {
            data: bytes,
            sections: &sections_info,
            is_64bit,
        };

        let mut functions_info = Vec::new();
        let mut iat_symbols = std::collections::HashMap::new();
        let mut global_symbols = std::collections::HashMap::new();

        // Parse Exports
        // DataDirectory[0] is Export Table
        let export_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(0)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
        };

        if export_dir_rva != 0 {
            if let Ok(mut exports) = loader.parse_exports(export_dir_rva, image_base) {
                functions_info.append(&mut exports);
            }
        }

        // Parse Imports
        // DataDirectory[1] is Import Table
        let import_dir_rva = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => opt
                .data_directories
                .get(1)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
        };

        if import_dir_rva != 0 {
            if let Ok((mut imports, symbols)) = loader.parse_imports(import_dir_rva, image_base) {
                functions_info.append(&mut imports);
                iat_symbols = symbols;
            }
        }

        // Parse COFF Symbol Table (if present)
        let file_header = &pe_file.file_header;
        if file_header.pointer_to_symbol_table != 0 && file_header.number_of_symbols > 0 {
            if let Ok(coff_functions) = loader.parse_coff_symbols(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                // Merge COFF symbols with existing functions, preferring COFF names over generated ones
                for coff_func in coff_functions {
                    if let Some(existing) = functions_info
                        .iter_mut()
                        .find(|f| f.address == coff_func.address)
                    {
                        // Replace generated name with real COFF symbol name
                        if existing.name.starts_with("FUN_0x") || existing.name.starts_with("sub_")
                        {
                            existing.name = coff_func.name;
                        }
                    } else {
                        functions_info.push(coff_func);
                    }
                }
            }

            if let Ok(coff_data_symbols) = loader.parse_coff_data_symbols(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                global_symbols = coff_data_symbols;
            }
        }

        // Add entry point if not exists
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(crate::loader::types::FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
                origin: Some("pe-entry".to_string()),
                kind: Some("entry".to_string()),
                source_section: None,
                external_library: None,
                is_thunk_like: false,
            });
        }

        // Parse Exception Directory (PDATA) for x64 binaries - contains function metadata
        // DataDirectory[3] is Exception Table (.pdata section)
        if is_64bit {
            let exception_dir_rva = match &pe_file.optional_header {
                PeOptionalHeader::Pe32Plus(opt) => opt
                    .data_directories
                    .get(3)
                    .map(|d| (d.virtual_address, d.size))
                    .unwrap_or((0, 0)),
                _ => (0, 0),
            };

            if exception_dir_rva.0 != 0 && exception_dir_rva.1 > 0 {
                if let Ok(pdata_functions) =
                    loader.parse_pdata(exception_dir_rva.0, exception_dir_rva.1, image_base)
                {
                    // Merge with existing functions, avoiding duplicates
                    for pdata_func in pdata_functions {
                        if !functions_info
                            .iter()
                            .any(|f| f.address == pdata_func.address)
                        {
                            functions_info.push(pdata_func);
                        }
                    }
                }
            }
        }

        let pdb_debug_info = match &pe_file.optional_header {
            PeOptionalHeader::Pe32(opt) | PeOptionalHeader::Pe32Plus(opt) => {
                opt.data_directories.get(6).and_then(|dir| {
                    loader.parse_pdb_debug_info(dir.virtual_address, dir.size, image_base)
                })
            }
        };

        LoadedBinaryBuilder::new(path, data)
            .format("PE")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .pdb_debug_info(pdb_debug_info)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .add_global_symbols(global_symbols)
            .build()
    }
}

pub fn detect_pe_is_64bit(bytes: &[u8]) -> bool {
    if bytes.len() < 0x40 {
        return true;
    }

    let pe_offset = if bytes.len() > 0x3F {
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize
    } else {
        return true;
    };

    if bytes.len() > pe_offset + 6 {
        let machine = u16::from_le_bytes([bytes[pe_offset + 4], bytes[pe_offset + 5]]);
        machine == 0x8664
    } else {
        true
    }
}

fn parse_pe_file(bytes: &[u8]) -> Result<RawPeFile> {
    let reader = ByteReader::little(bytes);
    if reader.slice(0, 2)? != b"MZ" {
        return Err(err!(loader, "MalformedHeader: missing DOS MZ signature"));
    }
    let pe_offset = reader.u32(0x3c)? as usize;
    if reader.slice(pe_offset, PE_SIGNATURE_SIZE)? != b"PE\0\0" {
        return Err(err!(loader, "MalformedHeader: missing PE signature"));
    }

    let file_header_offset = pe_offset + PE_SIGNATURE_SIZE;
    let machine = reader.u16(file_header_offset)?;
    let number_of_sections = reader.u16(file_header_offset + 2)?;
    let pointer_to_symbol_table = reader.u32(file_header_offset + 8)?;
    let number_of_symbols = reader.u32(file_header_offset + 12)?;
    let size_of_optional_header = reader.u16(file_header_offset + 16)? as usize;
    let optional_header_offset = file_header_offset + PE_FILE_HEADER_SIZE;
    let magic = reader.u16(optional_header_offset)?;

    let optional_header = match magic {
        PE32_MAGIC => {
            let data = parse_optional_header_data(
                &reader,
                optional_header_offset,
                false,
                size_of_optional_header,
            )?;
            PeOptionalHeader::Pe32(data)
        }
        PE32_PLUS_MAGIC => {
            let data = parse_optional_header_data(
                &reader,
                optional_header_offset,
                true,
                size_of_optional_header,
            )?;
            PeOptionalHeader::Pe32Plus(data)
        }
        _ => {
            return Err(err!(
                loader,
                "MalformedHeader: unsupported PE optional header magic 0x{magic:x}"
            ));
        }
    };

    let section_table_offset = optional_header_offset
        .checked_add(size_of_optional_header)
        .ok_or_else(|| err!(loader, "MalformedHeader: PE section table offset overflow"))?;
    let mut section_headers = Vec::with_capacity(number_of_sections as usize);
    for idx in 0..number_of_sections as usize {
        let offset = section_table_offset + idx * PE_SECTION_HEADER_SIZE;
        section_headers.push(PeSectionHeader {
            name: reader.fixed_string(offset, 8)?,
            virtual_size: reader.u32(offset + 8)?,
            virtual_address: reader.u32(offset + 12)?,
            size_of_raw_data: reader.u32(offset + 16)?,
            pointer_to_raw_data: reader.u32(offset + 20)?,
            characteristics: reader.u32(offset + 36)?,
        });
    }

    Ok(RawPeFile {
        file_header: PeFileHeader {
            machine,
            pointer_to_symbol_table,
            number_of_symbols,
        },
        optional_header,
        section_headers,
    })
}

fn parse_optional_header_data(
    reader: &ByteReader<'_>,
    optional_header_offset: usize,
    is_pe32_plus: bool,
    size_of_optional_header: usize,
) -> Result<PeOptionalHeaderData> {
    let address_of_entry_point = reader.u32(optional_header_offset + 16)?;
    let image_base = if is_pe32_plus {
        reader.u64(optional_header_offset + 24)?
    } else {
        u64::from(reader.u32(optional_header_offset + 28)?)
    };
    let section_alignment = reader.u32(optional_header_offset + 32)?;
    let (number_offset, directories_offset) = if is_pe32_plus {
        (optional_header_offset + 108, optional_header_offset + 112)
    } else {
        (optional_header_offset + 92, optional_header_offset + 96)
    };
    let number_of_rva_and_sizes = reader.u32(number_offset).unwrap_or(0) as usize;
    let max_dirs_by_size = size_of_optional_header
        .saturating_sub(directories_offset.saturating_sub(optional_header_offset))
        / 8;
    let directory_count = number_of_rva_and_sizes.min(max_dirs_by_size).min(32);
    let mut data_directories = Vec::with_capacity(directory_count);
    for idx in 0..directory_count {
        let offset = directories_offset + idx * 8;
        data_directories.push(DataDirectory {
            virtual_address: reader.u32(offset)?,
            size: reader.u32(offset + 4)?,
        });
    }

    Ok(PeOptionalHeaderData {
        image_base,
        address_of_entry_point,
        section_alignment,
        data_directories,
    })
}

struct PeLoaderImpl<'a> {
    data: &'a [u8],
    sections: &'a [SectionInfo],
    is_64bit: bool,
}

impl<'a> PeLoaderImpl<'a> {
    // Simplified version - main logic is in rva_to_file_offset
    #[allow(dead_code)]
    fn rva_to_offset(&self, _rva: u32) -> Option<u64> {
        None
    }

    fn read_string_at(&self, offset: u64) -> String {
        extract_cstring(self.data, offset as usize)
    }

    fn reader(&self) -> ByteReader<'a> {
        ByteReader::little(self.data)
    }

    fn read_u16(&self, offset: u64) -> Result<u16> {
        self.reader().u16(offset as usize)
    }

    fn read_i16(&self, offset: u64) -> Result<i16> {
        self.reader().i16(offset as usize)
    }

    fn read_u32(&self, offset: u64) -> Result<u32> {
        self.reader().u32(offset as usize)
    }

    fn read_u64(&self, offset: u64) -> Result<u64> {
        self.reader().u64(offset as usize)
    }

    fn read_import_descriptor(&self, offset: u64) -> Result<ImportDescriptor> {
        Ok(ImportDescriptor {
            original_first_thunk: self.read_u32(offset)?,
            name: self.read_u32(offset + 12)?,
            first_thunk: self.read_u32(offset + 16)?,
        })
    }

    fn read_export_directory(&self, offset: u64) -> Result<ExportDirectory> {
        Ok(ExportDirectory {
            number_of_functions: self.read_u32(offset + 20)?,
            number_of_names: self.read_u32(offset + 24)?,
            address_of_functions: self.read_u32(offset + 28)?,
            address_of_names: self.read_u32(offset + 32)?,
            address_of_name_ordinals: self.read_u32(offset + 36)?,
        })
    }

    fn read_debug_directory(&self, offset: u64) -> Result<ImageDebugDirectory> {
        Ok(ImageDebugDirectory {
            debug_type: self.read_u32(offset + 12)?,
            size_of_data: self.read_u32(offset + 16)?,
            address_of_raw_data: self.read_u32(offset + 20)?,
            pointer_to_raw_data: self.read_u32(offset + 24)?,
        })
    }

    fn read_coff_symbol(&self, offset: u64) -> Result<CoffSymbol> {
        let name_bytes = self.reader().slice(offset as usize, 8)?;
        let name = if name_bytes[0..4] == [0, 0, 0, 0] {
            SymbolName::LongName(u32::from_le_bytes(name_bytes[4..8].try_into().unwrap()))
        } else {
            let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            SymbolName::ShortName(String::from_utf8_lossy(&name_bytes[..len]).to_string())
        };
        Ok(CoffSymbol {
            name,
            value: self.read_u32(offset + 8)?,
            section_number: self.read_i16(offset + 12)?,
            symbol_type: self.read_u16(offset + 14)?,
            storage_class: self.reader().u8(offset as usize + 16)?,
            number_of_aux_symbols: self.reader().u8(offset as usize + 17)?,
        })
    }

    fn rva_to_file_offset(&self, rva: u32, image_base: u64) -> Option<u64> {
        // rva is relative to image_base.
        // section.virtual_address is image_base + section_rva

        for section in self.sections {
            let section_va = section.virtual_address;
            let section_rva = (section_va - image_base) as u32;
            let section_size = section.virtual_size as u32;

            // Check if RVA is within this section
            if rva >= section_rva && rva < section_rva + section_size {
                let delta = rva - section_rva;
                return Some(section.file_offset + delta as u64);
            }
        }

        // Header fallback: if RVA is small (in headers), direct map
        if rva < 0x1000 {
            return Some(rva as u64);
        }

        None
    }

    fn parse_exports(
        &self,
        dir_rva: u32,
        image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        let offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Export Dir RVA"))?;
        let export_dir = self.read_export_directory(offset)?;

        let mut functions = Vec::new();

        // Parse Names
        if export_dir.number_of_names > 0 && export_dir.address_of_names != 0 {
            let names_offset = self
                .rva_to_file_offset(export_dir.address_of_names, image_base)
                .unwrap_or(0);
            let ordinals_offset = self
                .rva_to_file_offset(export_dir.address_of_name_ordinals, image_base)
                .unwrap_or(0);
            let funcs_offset = self
                .rva_to_file_offset(export_dir.address_of_functions, image_base)
                .unwrap_or(0);

            if names_offset != 0 && ordinals_offset != 0 && funcs_offset != 0 {
                for idx in 0..export_dir.number_of_names.min(10000) {
                    // Safety limit
                    let name_rva = self
                        .read_u32(names_offset + u64::from(idx) * 4)
                        .unwrap_or(0);
                    let ordinal = self
                        .read_u16(ordinals_offset + u64::from(idx) * 2)
                        .unwrap_or(0);

                    if name_rva != 0 {
                        let name_offset =
                            self.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                        let name = self.read_string_at(name_offset);

                        // Lookup function RVA using ordinal
                        // AddressOfFunctions is indexed by Ordinal (Base subtracted)
                        let func_idx = ordinal as u64; // Indices are 0-based from table start
                        if func_idx < export_dir.number_of_functions as u64 {
                            let entry_offset = funcs_offset + func_idx * 4;
                            let func_rva = self.read_u32(entry_offset).unwrap_or(0);

                            if func_rva != 0 {
                                functions.push(crate::loader::types::FunctionInfo {
                                    name,
                                    address: image_base + func_rva as u64,
                                    size: 0,
                                    is_export: true,
                                    is_import: false,
                                    origin: Some("pe-export-table".to_string()),
                                    kind: Some("export".to_string()),
                                    source_section: None,
                                    external_library: None,
                                    is_thunk_like: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(functions)
    }

    fn parse_imports(
        &self,
        dir_rva: u32,
        image_base: u64,
    ) -> Result<(
        Vec<crate::loader::types::FunctionInfo>,
        std::collections::HashMap<u64, String>,
    )> {
        imports::parse_imports(self, dir_rva, image_base)
    }

    fn parse_pdata(
        &self,
        pdata_rva: u32,
        pdata_size: u32,
        image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        pdata::parse_pdata(self, pdata_rva, pdata_size, image_base)
    }

    fn parse_coff_symbols(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        _image_base: u64,
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
        coff::parse_coff_symbols(self, symbol_table_offset, symbol_count, _image_base)
    }

    fn parse_coff_data_symbols(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        _image_base: u64,
    ) -> Result<std::collections::HashMap<u64, String>> {
        coff::parse_coff_data_symbols(self, symbol_table_offset, symbol_count, _image_base)
    }

    fn parse_pdb_debug_info(
        &self,
        dir_rva: u32,
        dir_size: u32,
        image_base: u64,
    ) -> Option<PdbDebugInfo> {
        if dir_rva == 0 || dir_size < 28 {
            return None;
        }
        let dir_offset = self.rva_to_file_offset(dir_rva, image_base)?;
        let entry_count = (dir_size as usize) / IMAGE_DEBUG_DIRECTORY_SIZE;
        for idx in 0..entry_count {
            let entry = self
                .read_debug_directory(dir_offset + (idx * IMAGE_DEBUG_DIRECTORY_SIZE) as u64)
                .ok()?;
            if entry.debug_type != IMAGE_DEBUG_TYPE_CODEVIEW || entry.size_of_data < 4 {
                continue;
            }

            let data_offset = if entry.pointer_to_raw_data != 0 {
                u64::from(entry.pointer_to_raw_data)
            } else {
                self.rva_to_file_offset(entry.address_of_raw_data, image_base)?
            };
            let data_end = data_offset.checked_add(u64::from(entry.size_of_data))? as usize;
            let data_offset = data_offset as usize;
            if data_end > self.data.len() || data_offset >= data_end {
                continue;
            }
            let data = &self.data[data_offset..data_end];
            let signature = data.get(0..4)?;
            match signature {
                b"RSDS" => {
                    if data.len() < 24 {
                        continue;
                    }
                    let guid_hex = data[4..20]
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    let age = u32::from_le_bytes(data[20..24].try_into().ok()?);
                    let path_hint = Some(extract_cstring(data, 24)).filter(|s| !s.is_empty());
                    return Some(PdbDebugInfo {
                        path_hint,
                        guid_hex: Some(guid_hex),
                        age: Some(age),
                        has_codeview: true,
                    });
                }
                b"NB10" => {
                    if data.len() < 16 {
                        continue;
                    }
                    let age = u32::from_le_bytes(data[12..16].try_into().ok()?);
                    let path_hint = Some(extract_cstring(data, 16)).filter(|s| !s.is_empty());
                    return Some(PdbDebugInfo {
                        path_hint,
                        guid_hex: None,
                        age: Some(age),
                        has_codeview: true,
                    });
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_synthetic_pe() {
        let mut data = vec![0u8; 1024];

        // DOS Header
        data[0] = 0x4D;
        data[1] = 0x5A; // MZ
        data[0x3C] = 0x40; // e_lfanew = 0x40

        // PE Header (at 0x40)
        data[0x40] = 0x50;
        data[0x41] = 0x45; // PE\0\0

        // File Header (at 0x44)
        data[0x44] = 0x4C;
        data[0x45] = 0x01; // Machine = 0x14C (x86)
        data[0x46] = 0x01; // NumberOfSections = 1
        data[0x54] = 0xE0; // SizeOfOptionalHeader = 224 (0xE0)
        data[0x55] = 0x00;

        // Optional Header (at 0x58)
        data[0x58] = 0x0B;
        data[0x59] = 0x01; // Magic = 0x10B (PE32)
        // ImageBase (at 0x58 + 28 = 0x74)
        data[0x74] = 0x00;
        data[0x75] = 0x00;
        data[0x76] = 0x40; // 0x400000
        // Data Directories (16 entries)
        data[0x58 + 92] = 16; // NumberOfRvaAndSizes

        // Section Headers (at 0x40 + 4 + 20 + 0xE0 = 0x138)
        let section_offset = 0x138;
        // Name: .text
        data[section_offset] = b'.';
        data[section_offset + 1] = b't';
        data[section_offset + 2] = b'e';
        data[section_offset + 3] = b'x';
        data[section_offset + 4] = b't';

        // Characteristics (at +36)
        data[section_offset + 36] = 0x20;
        data[section_offset + 37] = 0x00;
        data[section_offset + 38] = 0x00;
        data[section_offset + 39] = 0x60; // Executable | Readable (0x60000020)

        let path = "test.exe".to_string();
        let result = PeLoader::parse(DataBuffer::Heap(data), path);

        if let Err(e) = &result {
            println!("Parse error: {}", e);
        }

        assert!(result.is_ok());
        let Ok(bin) = result else {
            panic!("PE parsing should succeed")
        };
        assert_eq!(bin.format, "PE");
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].name, ".text");
        assert_eq!(bin.sections[0].is_executable, true);
        assert_eq!(bin.arch_spec, "x86:LE:32:default");
        assert_eq!(bin.sleigh_language_id(), Some("x86:LE:32:default"));
    }

    #[test]
    fn test_parse_synthetic_pe_arm64_sets_aarch64_arch_spec() {
        let mut data = vec![0u8; 1024];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0x64;
        data[0x45] = 0xAA; // Machine = 0xAA64 (ARM64)
        data[0x46] = 0x01;
        data[0x54] = 0xF0; // SizeOfOptionalHeader = 240 (PE32+)
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x02; // Magic = 0x20B (PE32+)
        data[0x58 + 108] = 16; // NumberOfRvaAndSizes

        let section_offset = 0x148;
        data[section_offset] = b'.';
        data[section_offset + 1] = b't';
        data[section_offset + 2] = b'e';
        data[section_offset + 3] = b'x';
        data[section_offset + 4] = b't';
        data[section_offset + 36] = 0x20;
        data[section_offset + 39] = 0x60;

        let result = PeLoader::parse(DataBuffer::Heap(data), "arm64.exe".to_string());
        assert!(result.is_ok());
        let bin = result.expect("arm64 pe should parse");
        assert_eq!(bin.arch_spec, "AARCH64:LE:64:v8A");
        assert_eq!(bin.sleigh_language_id(), Some("AARCH64:LE:64:v8A"));
        assert!(bin.is_64bit);
    }

    #[test]
    fn test_parse_synthetic_pe_unknown_machine_fails_closed() {
        let mut data = vec![0u8; 1024];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0xff;
        data[0x45] = 0xff; // unknown Machine = 0xffff
        data[0x46] = 0x01;
        data[0x54] = 0xF0;
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x02; // PE32+
        data[0x58 + 108] = 16;

        let result = PeLoader::parse(DataBuffer::Heap(data), "unknown.exe".to_string());
        assert!(result.is_err());
        let err = format!("{}", result.expect_err("unknown machine must fail"));
        assert!(err.contains("unsupported machine"));
        assert!(!err.contains("defaulting to x86"));
    }

    #[test]
    fn test_parse_synthetic_pe_rsds_sets_pdb_debug_info() {
        let mut data = vec![0u8; 2048];

        data[0] = 0x4D;
        data[1] = 0x5A;
        data[0x3C] = 0x40;

        data[0x40] = 0x50;
        data[0x41] = 0x45;

        data[0x44] = 0x4C;
        data[0x45] = 0x01; // x86
        data[0x46] = 0x01; // NumberOfSections = 1
        data[0x54] = 0xE0;
        data[0x55] = 0x00;

        data[0x58] = 0x0B;
        data[0x59] = 0x01; // PE32
        data[0x74] = 0x00;
        data[0x75] = 0x00;
        data[0x76] = 0x40; // ImageBase = 0x400000
        data[0x58 + 92] = 16; // NumberOfRvaAndSizes

        let debug_dir_offset = 0x58 + 96 + (6 * 8);
        data[debug_dir_offset..debug_dir_offset + 4].copy_from_slice(&0x200u32.to_le_bytes());
        data[debug_dir_offset + 4..debug_dir_offset + 8].copy_from_slice(&28u32.to_le_bytes());

        let section_offset = 0x138;
        data[section_offset] = b'.';
        data[section_offset + 1] = b'r';
        data[section_offset + 2] = b'd';
        data[section_offset + 3] = b'a';
        data[section_offset + 4] = b't';
        data[section_offset + 5] = b'a';
        data[section_offset + 8..section_offset + 12].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 12..section_offset + 16].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 16..section_offset + 20].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 20..section_offset + 24].copy_from_slice(&0x200u32.to_le_bytes());
        data[section_offset + 36] = 0x40;
        data[section_offset + 39] = 0x40; // readable data

        let debug_dir_file = 0x200usize;
        data[debug_dir_file + 12..debug_dir_file + 16]
            .copy_from_slice(&IMAGE_DEBUG_TYPE_CODEVIEW.to_le_bytes());
        let rsds_path = b"C:\\symbols\\has_pdb.pdb\0";
        let rsds_size = 24u32 + rsds_path.len() as u32;
        data[debug_dir_file + 16..debug_dir_file + 20].copy_from_slice(&rsds_size.to_le_bytes());
        data[debug_dir_file + 20..debug_dir_file + 24].copy_from_slice(&0x220u32.to_le_bytes());
        data[debug_dir_file + 24..debug_dir_file + 28].copy_from_slice(&0x220u32.to_le_bytes());

        let rsds_offset = 0x220usize;
        data[rsds_offset..rsds_offset + 4].copy_from_slice(b"RSDS");
        for (idx, byte) in data[rsds_offset + 4..rsds_offset + 20]
            .iter_mut()
            .enumerate()
        {
            *byte = (idx as u8) + 1;
        }
        data[rsds_offset + 20..rsds_offset + 24].copy_from_slice(&1u32.to_le_bytes());
        data[rsds_offset + 24..rsds_offset + 24 + rsds_path.len()].copy_from_slice(rsds_path);

        let result = PeLoader::parse(DataBuffer::Heap(data), "has_pdb.exe".to_string());
        assert!(result.is_ok());
        let bin = result.expect("rsds pe should parse");
        let pdb = bin.inner().pdb_debug_info.as_ref().expect("pdb debug info");
        assert!(pdb.has_codeview);
        assert_eq!(pdb.age, Some(1));
        assert!(
            pdb.path_hint
                .as_deref()
                .is_some_and(|path| path.ends_with("has_pdb.pdb"))
        );
    }
}
