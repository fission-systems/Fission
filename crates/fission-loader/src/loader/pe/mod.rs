use crate::loader::types::{
    DataBuffer, LoadedBinary, LoadedBinaryBuilder, SectionInfo, extract_cstring,
};
use crate::prelude::*;
use binrw::BinRead;
use std::io::Cursor;

mod coff;
mod imports;
mod pdata;
pub mod schema;
use schema::*;

pub struct PeLoader;

impl PeLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        let mut cursor = Cursor::new(bytes);
        let pe_file = PeFile::read_le(&mut cursor)
            .map_err(|e| err!(loader, "binrw PE parse error: {}", e))?;

        // Extract basic info
        let is_64bit = match pe_file.nt_headers.optional_header {
            OptionalHeader::Pe32(_) => false,
            OptionalHeader::Pe32Plus(_) => true,
        };

        let (image_base, entry_point, _section_alignment) =
            match &pe_file.nt_headers.optional_header {
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
            data: bytes,
            sections: &sections_info,
            is_64bit,
        };

        let mut functions_info = Vec::new();
        let mut iat_symbols = std::collections::HashMap::new();
        let mut global_symbols = std::collections::HashMap::new();

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

        // Linear sweep: if we only found the entry point (stripped binary with no
        // COFF symbols, no PDATA, no exports), scan executable sections for function
        // prologues so the decompiler has something to work with.
        let non_import_count = functions_info.iter().filter(|f| !f.is_import).count();
        if non_import_count <= 1 {
            let known_addrs: std::collections::HashSet<u64> =
                functions_info.iter().map(|f| f.address).collect();

            for section in &sections_info {
                if !section.is_executable {
                    continue;
                }
                let file_start = section.file_offset as usize;
                let file_end = (section.file_offset + section.file_size) as usize;
                if file_end > bytes.len() || file_start >= file_end {
                    continue;
                }
                let sec_bytes = &bytes[file_start..file_end];
                let sec_va = section.virtual_address;

                let swept = scan_prologue_functions(sec_bytes, sec_va, is_64bit, &known_addrs);
                for func in swept {
                    functions_info.push(func);
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
            .add_global_symbols(global_symbols)
            .build()
    }
}

// ---------------------------------------------------------------------------
// Linear-sweep function prologue detection
// ---------------------------------------------------------------------------

/// x86 (32-bit) function prologue byte patterns.
/// Each entry is (pattern_bytes, description).
/// Sorted from most-specific (longest) to least-specific to reduce false positives.
const X86_PROLOGUES: &[(&[u8], &str)] = &[
    // ---- 4-byte patterns (MSVC callee-save variants) ----
    (&[0x55, 0x57, 0x8B, 0xEC], "push ebp; push edi; mov ebp,esp"),
    (&[0x55, 0x56, 0x8B, 0xEC], "push ebp; push esi; mov ebp,esp"),
    (&[0x55, 0x53, 0x8B, 0xEC], "push ebp; push ebx; mov ebp,esp"),
    (&[0x57, 0x55, 0x8B, 0xEC], "push edi; push ebp; mov ebp,esp"),
    (&[0x56, 0x55, 0x8B, 0xEC], "push esi; push ebp; mov ebp,esp"),
    (&[0x53, 0x55, 0x8B, 0xEC], "push ebx; push ebp; mov ebp,esp"),
    // ---- 3-byte patterns (baseline MSVC/GCC) ----
    (&[0x55, 0x8B, 0xEC], "push ebp; mov ebp,esp (MSVC)"),
    (&[0x55, 0x89, 0xE5], "push ebp; mov ebp,esp (GCC)"),
];

/// x86-64 function prologue byte patterns.
const X64_PROLOGUES: &[(&[u8], &str)] = &[
    // MSVC shadow-store variants (most common in MSVC x64 output)
    (&[0x48, 0x89, 0x5C, 0x24], "mov [rsp+N],rbx"),
    (&[0x48, 0x89, 0x4C, 0x24], "mov [rsp+N],rcx"),
    (&[0x48, 0x89, 0x54, 0x24], "mov [rsp+N],rdx"),
    (&[0x48, 0x89, 0x44, 0x24], "mov [rsp+N],rax"),
    // push rbp variants (REX prefix)
    (&[0x40, 0x55], "REX push rbp"),
    (&[0x48, 0x55], "REX.W push rbp"),
    // sub rsp variants
    (&[0x48, 0x83, 0xEC], "sub rsp,imm8"),
    (&[0x48, 0x81, 0xEC], "sub rsp,imm32"),
    // non-REX push rbp
    (&[0x55, 0x48, 0x8B, 0xEC], "push rbp; mov rbp,rsp"),
    (&[0x55, 0x48, 0x89, 0xE5], "push rbp; mov rbp,rsp (GCC)"),
];

/// Scan a raw section byte slice for function prologues.
///
/// Returns a list of `FunctionInfo` for each hit whose VA is not already
/// in `known_addrs`.  Named `sub_{va:08x}` (Ghidra convention).
fn scan_prologue_functions(
    sec_bytes: &[u8],
    sec_va: u64,
    is_64bit: bool,
    known_addrs: &std::collections::HashSet<u64>,
) -> Vec<crate::loader::types::FunctionInfo> {
    let prologues: &[(&[u8], &str)] = if is_64bit {
        X64_PROLOGUES
    } else {
        X86_PROLOGUES
    };

    // Standard PE/disk sector size in bytes; used to skip zero-filled padding blocks.
    const PE_SECTOR_SIZE: usize = 512;

    // Minimum prologue length needed (used to avoid out-of-bounds)
    let min_pat_len = prologues.iter().map(|(p, _)| p.len()).min().unwrap_or(2);

    let mut results = Vec::new();
    // Build a dedup set for this batch as we go
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Walk every byte; restrict to 4-byte-aligned offsets for x86 (MSVC aligns
    // most functions to 16 bytes, but 4-byte alignment catches everything without
    // being too slow on a 10MB section).
    let step = if is_64bit { 1 } else { 4 };

    let len = sec_bytes.len();
    if len < min_pat_len {
        return results;
    }

    // Pre-build a zero page for quick skip
    let zero_block = [0u8; PE_SECTOR_SIZE];

    let mut i = 0usize;
    while i + min_pat_len <= len {
        // Skip 512-byte zero blocks (common in padding areas)
        if i + PE_SECTOR_SIZE <= len && sec_bytes[i..i + PE_SECTOR_SIZE] == zero_block[..] {
            i += PE_SECTOR_SIZE;
            continue;
        }

        // Try each prologue pattern at this offset
        let mut matched = false;
        for (pat, _desc) in prologues {
            let pat_len = pat.len();
            if i + pat_len > len {
                continue;
            }
            if sec_bytes[i..i + pat_len] == **pat {
                let va = sec_va + i as u64;
                if !known_addrs.contains(&va) && seen.insert(va) {
                    results.push(crate::loader::types::FunctionInfo {
                        name: format!("sub_{:08x}", va),
                        address: va,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }
                matched = true;
                break; // one pattern per offset is enough
            }
        }

        // After a match, advance by 4 to catch adjacent (unlikely but safe)
        i += if matched { 4 } else { step };
    }

    results
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
    ) -> Result<Vec<crate::loader::types::FunctionInfo>> {
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
                                functions.push(crate::loader::types::FunctionInfo {
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
        assert_eq!(bin.format, "PE (binrw)");
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].name, ".text");
        assert_eq!(bin.sections[0].is_executable, true);
    }
}
