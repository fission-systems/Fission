use crate::analysis::loader::types::{
    extract_cstring, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use binrw::BinRead;
use std::io::Cursor;

pub mod schema;
use schema::*;

pub struct PeLoader;

impl PeLoader {
    pub fn parse(data: Vec<u8>, path: String) -> Result<LoadedBinary> {
        let mut cursor = Cursor::new(&data);
        let pe_file = PeFile::read_le(&mut cursor)
            .map_err(|e| err!(loader, "binrw PE parse error: {}", e))?;

        // Extract basic info
        let is_64bit = match pe_file.nt_headers.optional_header {
            OptionalHeader::Pe32(_) => false,
            OptionalHeader::Pe32Plus(_) => true,
        };

        let (image_base, entry_point, section_alignment) = match &pe_file.nt_headers.optional_header
        {
            OptionalHeader::Pe32(opt) => (
                opt.image_base as u64,
                opt.image_base as u64 + opt.address_of_entry_point as u64,
                opt.section_alignment,
            ),
            OptionalHeader::Pe32Plus(opt) => (
                opt.image_base,
                opt.image_base + opt.address_of_entry_point as u64,
                opt.section_alignment,
            ),
        };

        let arch_spec = match pe_file.nt_headers.file_header.machine {
            0x8664 => "x86:LE:64:default", // AMD64
            0x014c => "x86:LE:32:default", // I386
            _ => {
                if is_64bit {
                    "x86:LE:64:default"
                } else {
                    "x86:LE:32:default"
                }
            }
        };

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
            data: &data,
            sections: &sections_info,
            is_64bit,
        };

        let mut functions_info = Vec::new();
        let mut iat_symbols = std::collections::HashMap::new();

        // Parse Exports
        // DataDirectory[0] is Export Table
        let export_dir_rva = match &pe_file.nt_headers.optional_header {
            OptionalHeader::Pe32(opt) => opt
                .data_directories
                .get(0)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
            OptionalHeader::Pe32Plus(opt) => opt
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
        let import_dir_rva = match &pe_file.nt_headers.optional_header {
            OptionalHeader::Pe32(opt) => opt
                .data_directories
                .get(1)
                .map(|d| d.virtual_address)
                .unwrap_or(0),
            OptionalHeader::Pe32Plus(opt) => opt
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
        let file_header = &pe_file.nt_headers.file_header;
        if file_header.pointer_to_symbol_table != 0 && file_header.number_of_symbols > 0 {
            if let Ok(coff_functions) = loader.parse_coff_symbols(
                file_header.pointer_to_symbol_table,
                file_header.number_of_symbols,
                image_base,
            ) {
                // Merge COFF symbols with existing functions, preferring COFF names over generated ones
                for coff_func in coff_functions {
                    if let Some(existing) = functions_info.iter_mut().find(|f| f.address == coff_func.address) {
                        // Replace generated name with real COFF symbol name
                        if existing.name.starts_with("FUN_0x") || existing.name.starts_with("sub_") {
                            existing.name = coff_func.name;
                        }
                    } else {
                        functions_info.push(coff_func);
                    }
                }
            }
        }

        // Add entry point if not exists
        if entry_point != 0 && !functions_info.iter().any(|f| f.address == entry_point) {
            functions_info.push(crate::analysis::loader::types::FunctionInfo {
                name: "_start".to_string(),
                address: entry_point,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        // Parse Exception Directory (PDATA) for x64 binaries - contains function metadata
        // DataDirectory[3] is Exception Table (.pdata section)
        if is_64bit {
            let exception_dir_rva = match &pe_file.nt_headers.optional_header {
                OptionalHeader::Pe32Plus(opt) => opt
                    .data_directories
                    .get(3)
                    .map(|d| (d.virtual_address, d.size))
                    .unwrap_or((0, 0)),
                _ => (0, 0),
            };

            if exception_dir_rva.0 != 0 && exception_dir_rva.1 > 0 {
                if let Ok(pdata_functions) = loader.parse_pdata(exception_dir_rva.0, exception_dir_rva.1, image_base) {
                    // Merge with existing functions, avoiding duplicates
                    for pdata_func in pdata_functions {
                        if !functions_info.iter().any(|f| f.address == pdata_func.address) {
                            functions_info.push(pdata_func);
                        }
                    }
                }
            }
        }

        LoadedBinaryBuilder::new(path, data)
            .format("PE (binrw)")
            .arch_spec(arch_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_iat_symbols(iat_symbols)
            .build()
    }
}

struct PeLoaderImpl<'a> {
    data: &'a [u8],
    sections: &'a [SectionInfo],
    is_64bit: bool,
}

impl<'a> PeLoaderImpl<'a> {
    // Simplified version - main logic is in rva_to_file_offset
    fn rva_to_offset(&self, _rva: u32) -> Option<u64> {
        None
    }

    // Proper helpers utilizing raw data access
    fn read_at<T: BinRead>(&self, offset: u64) -> Result<T>
    where
        for<'b> T::Args<'b>: Default,
    {
        let mut cursor = Cursor::new(self.data);
        cursor.set_position(offset);
        T::read_le(&mut cursor).map_err(|e| err!(loader, "binrw read error: {}", e))
    }

    fn read_string_at(&self, offset: u64) -> String {
        extract_cstring(self.data, offset as usize)
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
    ) -> Result<Vec<crate::analysis::loader::types::FunctionInfo>> {
        let offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Export Dir RVA"))?;
        let export_dir: ExportDirectory = self.read_at(offset)?;

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
                let mut names_cursor = Cursor::new(self.data);
                names_cursor.set_position(names_offset);

                let mut ords_cursor = Cursor::new(self.data);
                ords_cursor.set_position(ordinals_offset);

                for _ in 0..export_dir.number_of_names.min(10000) {
                    // Safety limit
                    let name_rva = u32::read_le(&mut names_cursor).unwrap_or(0);
                    let ordinal = u16::read_le(&mut ords_cursor).unwrap_or(0);

                    if name_rva != 0 {
                        let name_offset =
                            self.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                        let name = self.read_string_at(name_offset);

                        // Lookup function RVA using ordinal
                        // AddressOfFunctions is indexed by Ordinal (Base subtracted)
                        let func_idx = ordinal as u64; // Indices are 0-based from table start
                        if func_idx < export_dir.number_of_functions as u64 {
                            let entry_offset = funcs_offset + func_idx * 4;
                            let func_rva = self.read_at::<u32>(entry_offset).unwrap_or(0);

                            if func_rva != 0 {
                                functions.push(crate::analysis::loader::types::FunctionInfo {
                                    name,
                                    address: image_base + func_rva as u64,
                                    size: 0,
                                    is_export: true,
                                    is_import: false,
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
        Vec<crate::analysis::loader::types::FunctionInfo>,
        std::collections::HashMap<u64, String>,
    )> {
        let offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Import Dir RVA"))?;
        let mut functions = Vec::new();
        let mut symbol_map = std::collections::HashMap::new();

        let mut cursor = Cursor::new(self.data);
        cursor.set_position(offset);

        loop {
            let desc = ImportDescriptor::read_le(&mut cursor).unwrap_or(ImportDescriptor {
                original_first_thunk: 0,
                time_date_stamp: 0,
                forwarder_chain: 0,
                name: 0,
                first_thunk: 0,
            });

            if desc.original_first_thunk == 0 && desc.first_thunk == 0 {
                break;
            } // null terminator

            // Read DLL Name
            let name_offset = self.rva_to_file_offset(desc.name, image_base).unwrap_or(0);
            let dll_name = {
                let name = self.read_string_at(name_offset);
                if name.is_empty() {
                    "unknown.dll".to_string()
                } else {
                    name
                }
            };

            // Read Thunks (use OriginalFirstThunk (ILT) preferably, else FirstThunk (IAT))
            let thunk_rva = if desc.original_first_thunk != 0 {
                desc.original_first_thunk
            } else {
                desc.first_thunk
            };
            let thunk_offset = self.rva_to_file_offset(thunk_rva, image_base).unwrap_or(0);

            let iat_base_rva = desc.first_thunk;

            if thunk_offset != 0 {
                let mut thunk_cursor = Cursor::new(self.data);
                thunk_cursor.set_position(thunk_offset);

                let mut idx = 0;
                loop {
                    let raw_thunk = if self.is_64bit {
                        u64::read_le(&mut thunk_cursor).unwrap_or(0)
                    } else {
                        u32::read_le(&mut thunk_cursor).unwrap_or(0) as u64
                    };

                    if raw_thunk == 0 {
                        break;
                    }

                    let is_ordinal = if self.is_64bit {
                        (raw_thunk & 0x8000000000000000) != 0
                    } else {
                        (raw_thunk & 0x80000000) != 0
                    };

                    let func_name = if is_ordinal {
                        format!("{}:Ordinal_{}", dll_name, raw_thunk & 0xFFFF)
                    } else {
                        // Import by Name
                        let name_rva = (raw_thunk & 0x7FFFFFFF) as u32; // mask out ordinal bit just in case
                        let name_offset =
                            self.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                        // Hint (2 bytes) + Name (ASCIIZ)
                        if name_offset != 0 {
                            let name = self.read_string_at(name_offset + 2);
                            if name.is_empty() {
                                format!("func_{}", idx)
                            } else {
                                name
                            }
                        } else {
                            format!("func_{}", idx)
                        }
                    };

                    let full_name = format!("{}!{}", dll_name, func_name);
                    let iat_addr = image_base
                        + iat_base_rva as u64
                        + (idx * if self.is_64bit { 8 } else { 4 });

                    functions.push(crate::analysis::loader::types::FunctionInfo {
                        name: full_name.clone(),
                        address: iat_addr, // For imports, address is IAT entry
                        size: 0,
                        is_export: false,
                        is_import: true,
                    });

                    symbol_map.insert(iat_addr, full_name);

                    idx += 1;
                }
            }
        }

        Ok((functions, symbol_map))
    }

    fn parse_pdata(
        &self,
        pdata_rva: u32,
        pdata_size: u32,
        image_base: u64,
    ) -> Result<Vec<crate::analysis::loader::types::FunctionInfo>> {
        let mut functions = Vec::new();

        // PDATA contains RUNTIME_FUNCTION structures (12 bytes each for x64)
        // struct RUNTIME_FUNCTION {
        //     DWORD BeginAddress;  // RVA of function start
        //     DWORD EndAddress;    // RVA of function end
        //     DWORD UnwindInfoAddress; // RVA of unwind info
        // }
        
        let pdata_offset = match self.rva_to_file_offset(pdata_rva, image_base) {
            Some(off) => off,
            None => return Ok(functions),
        };

        let entry_count = (pdata_size / 12) as usize; // 12 bytes per entry
        
        for i in 0..entry_count {
            let entry_offset = pdata_offset + (i * 12) as u64;
            
            if entry_offset + 12 > self.data.len() as u64 {
                break;
            }

            // Read 3 DWORDs (little-endian)
            let begin_rva = u32::from_le_bytes([
                self.data[entry_offset as usize],
                self.data[(entry_offset + 1) as usize],
                self.data[(entry_offset + 2) as usize],
                self.data[(entry_offset + 3) as usize],
            ]);

            let end_rva = u32::from_le_bytes([
                self.data[(entry_offset + 4) as usize],
                self.data[(entry_offset + 5) as usize],
                self.data[(entry_offset + 6) as usize],
                self.data[(entry_offset + 7) as usize],
            ]);

            if begin_rva == 0 || begin_rva >= end_rva {
                continue;
            }

            let func_addr = image_base + begin_rva as u64;
            let func_size = (end_rva - begin_rva) as u64;

            functions.push(crate::analysis::loader::types::FunctionInfo {
                name: format!("FUN_0x{:x}", func_addr),
                address: func_addr,
                size: func_size,
                is_export: false,
                is_import: false,
            });
        }

        Ok(functions)
    }

    fn parse_coff_symbols(
        &self,
        symbol_table_offset: u32,
        symbol_count: u32,
        image_base: u64,
    ) -> Result<Vec<crate::analysis::loader::types::FunctionInfo>> {
        let mut functions = Vec::new();
        
        // COFF Symbol Table starts at file offset
        let symbols_offset = symbol_table_offset as u64;
        let symbols_end = symbols_offset + (symbol_count as u64 * 18); // 18 bytes per symbol
        
        if symbols_end > self.data.len() as u64 {
            return Ok(functions);
        }
        
        // String table starts immediately after symbol table
        let string_table_offset = symbols_end;
        
        // Read string table size (first 4 bytes)
        let string_table_size = if string_table_offset + 4 <= self.data.len() as u64 {
            u32::from_le_bytes([
                self.data[string_table_offset as usize],
                self.data[(string_table_offset + 1) as usize],
                self.data[(string_table_offset + 2) as usize],
                self.data[(string_table_offset + 3) as usize],
            ])
        } else {
            0
        };
        
        let mut cursor = Cursor::new(self.data);
        cursor.set_position(symbols_offset);
        
        let mut total_processed = 0;
        let mut skipped_class = 0;
        let mut skipped_type = 0;
        let mut skipped_section = 0;
        
        let mut i = 0;
        while i < symbol_count {
            let symbol_pos = cursor.position();
            
            let symbol = match CoffSymbol::read_le(&mut cursor) {
                Ok(s) => s,
                Err(_) => break,
            };
            
            // Remember aux count before processing
            let aux_count = symbol.number_of_aux_symbols;
            
            i += 1; // Count this symbol
            
            total_processed += 1;
            
            // Only process external symbols (C_EXT = 2) and static symbols (C_STAT = 3) with function type
            if symbol.storage_class != storage_class::C_EXT && 
               symbol.storage_class != storage_class::C_STAT {
                skipped_class += 1;
                // Skip aux symbols for this symbol too
                if aux_count > 0 {
                    cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                    i += aux_count as u32;
                }
                continue;
            }
            
            // Check if it's a function (DT_FCN in high byte of type)
            let is_function = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
            if !is_function {
                skipped_type += 1;
                // Skip aux symbols for this symbol too
                if aux_count > 0 {
                    cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                    i += aux_count as u32;
                }
                continue;
            }
            
            // Get symbol name
            let name = match &symbol.name {
                SymbolName::ShortName(n) => n.clone(),
                SymbolName::LongName(offset) => {
                    let str_offset = string_table_offset + *offset as u64;
                    if str_offset < self.data.len() as u64 {
                        self.read_string_at(str_offset)
                    } else {
                        continue;
                    }
                }
            };
            
            if name.is_empty() {
                // Skip aux symbols
                if aux_count > 0 {
                    cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                    i += aux_count as u32;
                }
                continue;
            }
            
            // Section number is 1-based, 0 = undefined, -1 = absolute, -2 = debug
            if symbol.section_number <= 0 {
                skipped_section += 1;
                // Skip aux symbols
                if aux_count > 0 {
                    cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                    i += aux_count as u32;
                }
                continue;
            }
            
            // Find section to calculate actual address
            let section_idx = (symbol.section_number - 1) as usize;
            if section_idx >= self.sections.len() {
                skipped_section += 1;
                // Skip aux symbols
                if aux_count > 0 {
                    cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                    i += aux_count as u32;
                }
                continue;
            }
            
            let section = &self.sections[section_idx];
            let func_addr = section.virtual_address + symbol.value as u64;
            
            functions.push(crate::analysis::loader::types::FunctionInfo {
                name,
                address: func_addr,
                size: 0, // COFF symbols don't provide size
                is_export: false,
                is_import: false,
            });
            
            // Skip auxiliary symbols AFTER processing this symbol
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32; // Skip aux symbols in counter
            }
        }
        
        Ok(functions)
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
        let result = PeLoader::parse(data, path);

        if let Err(e) = &result {
            println!("Parse error: {}", e);
        }

        assert!(result.is_ok());
        let bin = result.unwrap();
        assert_eq!(bin.format, "PE (binrw)");
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].name, ".text");
        assert_eq!(bin.sections[0].is_executable, true);
    }
}
