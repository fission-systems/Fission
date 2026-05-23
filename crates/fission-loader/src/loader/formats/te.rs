use crate::loader::reader::ByteReader;
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
    extract_cstring, PdbDebugInfo, InferredTypeInfo, InferredFieldInfo,
};
use crate::prelude::*;
use fission_core::architecture::select_pe_load_spec;
use std::collections::HashMap;

const IMAGE_DEBUG_DIRECTORY_SIZE: usize = 28;
const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;

pub struct TeLoader;

struct TeLoaderImpl<'a> {
    data: &'a [u8],
    sections: &'a [SectionInfo],
    stripped_size: u32,
}

impl<'a> TeLoaderImpl<'a> {
    fn rva_to_file_offset(&self, rva: u32, image_base: u64) -> Option<u64> {
        for section in self.sections {
            let section_va = section.virtual_address;
            let section_rva = (section_va - image_base) as u32;
            let section_size = section.virtual_size as u32;

            if rva >= section_rva && rva < section_rva + section_size {
                let delta = rva - section_rva;
                return Some(section.file_offset + delta as u64);
            }
        }

        // Header fallback
        if rva < 0x1000 {
            return Some(rva as u64);
        }

        None
    }

    fn read_u16(&self, offset: u64) -> Result<u16> {
        let bytes = self.data.get(offset as usize..offset as usize + 2)
            .ok_or_else(|| err!(loader, "out of bounds u16"))?;
        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn read_u32(&self, offset: u64) -> Result<u32> {
        let bytes = self.data.get(offset as usize..offset as usize + 4)
            .ok_or_else(|| err!(loader, "out of bounds u32"))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn parse_relocations(&self, dir_rva: u32, dir_size: u32, image_base: u64) -> Result<Vec<crate::loader::types::RelocationEntry>> {
        let mut offset = self
            .rva_to_file_offset(dir_rva, image_base)
            .ok_or(err!(loader, "Invalid Reloc Dir RVA"))?;
        let end_offset = offset.saturating_add(dir_size as u64);
        let mut relocs = Vec::new();

        while offset < end_offset {
            let page_rva = self.read_u32(offset).unwrap_or(0);
            let block_size = self.read_u32(offset + 4).unwrap_or(0);
            if block_size < 8 || block_size > 4096 {
                break;
            }

            let num_entries = (block_size - 8) / 2;
            let mut entry_offset = offset + 8;
            for _ in 0..num_entries {
                let entry = self.read_u16(entry_offset).unwrap_or(0);
                entry_offset += 2;

                let r_type = (entry >> 12) as u32;
                let r_offset = (entry & 0x0FFF) as u32;

                if r_type == 0 {
                    continue;
                }

                let address = image_base + page_rva as u64 + r_offset as u64;
                let size = match r_type {
                    10 => 8, // DIR64
                    3 => 4,  // HIGHLOW
                    1 | 2 => 2, // HIGH / LOW
                    _ => 0,
                };

                relocs.push(crate::loader::types::RelocationEntry {
                    address,
                    r_type,
                    size,
                    addend: 0,
                    symbol_name: None,
                });
            }

            offset = offset.saturating_add(block_size as u64);
        }

        Ok(relocs)
    }

    fn parse_debug_directory(
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
            let offset = dir_offset + (idx * IMAGE_DEBUG_DIRECTORY_SIZE) as u64;
            let debug_type = self.read_u32(offset + 12).ok()?;
            let size_of_data = self.read_u32(offset + 16).ok()?;
            let address_of_raw_data = self.read_u32(offset + 20).ok()?;
            let pointer_to_raw_data = self.read_u32(offset + 24).ok()?;

            if debug_type != IMAGE_DEBUG_TYPE_CODEVIEW || size_of_data < 4 {
                continue;
            }

            let data_offset = if pointer_to_raw_data != 0 {
                // In TE format, we need to adjust pointer_to_raw_data for StrippedSize as well!
                if pointer_to_raw_data >= self.stripped_size {
                    (pointer_to_raw_data - self.stripped_size).saturating_add(40) as u64
                } else {
                    pointer_to_raw_data as u64
                }
            } else {
                self.rva_to_file_offset(address_of_raw_data, image_base)?
            };

            let data_end = data_offset.checked_add(u64::from(size_of_data))? as usize;
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

fn generate_te_header_types(
    image_base: u64,
    section_count: u8,
) -> (Vec<InferredTypeInfo>, HashMap<u64, String>) {
    let mut types = Vec::new();
    let mut symbols = HashMap::new();

    // 1. EFI_TE_IMAGE_HEADER
    let te_fields = vec![
        InferredFieldInfo { name: "Signature".to_string(), type_name: "WORD".to_string(), offset: 0, size: 2 },
        InferredFieldInfo { name: "Machine".to_string(), type_name: "WORD".to_string(), offset: 2, size: 2 },
        InferredFieldInfo { name: "NumberOfSections".to_string(), type_name: "BYTE".to_string(), offset: 4, size: 1 },
        InferredFieldInfo { name: "Subsystem".to_string(), type_name: "BYTE".to_string(), offset: 5, size: 1 },
        InferredFieldInfo { name: "StrippedSize".to_string(), type_name: "WORD".to_string(), offset: 6, size: 2 },
        InferredFieldInfo { name: "AddressOfEntryPoint".to_string(), type_name: "DWORD".to_string(), offset: 8, size: 4 },
        InferredFieldInfo { name: "BaseOfCode".to_string(), type_name: "DWORD".to_string(), offset: 12, size: 4 },
        InferredFieldInfo { name: "ImageBase".to_string(), type_name: "QWORD".to_string(), offset: 16, size: 8 },
        InferredFieldInfo { name: "DataDirectory_BaseReloc_RVA".to_string(), type_name: "DWORD".to_string(), offset: 24, size: 4 },
        InferredFieldInfo { name: "DataDirectory_BaseReloc_Size".to_string(), type_name: "DWORD".to_string(), offset: 28, size: 4 },
        InferredFieldInfo { name: "DataDirectory_Debug_RVA".to_string(), type_name: "DWORD".to_string(), offset: 32, size: 4 },
        InferredFieldInfo { name: "DataDirectory_Debug_Size".to_string(), type_name: "DWORD".to_string(), offset: 36, size: 4 },
    ];
    types.push(InferredTypeInfo {
        name: "EFI_TE_IMAGE_HEADER".to_string(),
        mangled_name: String::new(),
        kind: "struct".to_string(),
        fields: te_fields,
        size: 40,
        metadata_address: image_base,
    });
    symbols.insert(image_base, "TE_HEADER".to_string());

    // 2. EFI_IMAGE_SECTION_HEADER array
    let section_headers_va = image_base + 40;
    for idx in 0..section_count {
        let sec_va = section_headers_va + (idx as u64 * 40);
        let sec_fields = vec![
            InferredFieldInfo { name: "Name".to_string(), type_name: "char[8]".to_string(), offset: 0, size: 8 },
            InferredFieldInfo { name: "VirtualSize".to_string(), type_name: "DWORD".to_string(), offset: 8, size: 4 },
            InferredFieldInfo { name: "VirtualAddress".to_string(), type_name: "DWORD".to_string(), offset: 12, size: 4 },
            InferredFieldInfo { name: "SizeOfRawData".to_string(), type_name: "DWORD".to_string(), offset: 16, size: 4 },
            InferredFieldInfo { name: "PointerToRawData".to_string(), type_name: "DWORD".to_string(), offset: 20, size: 4 },
            InferredFieldInfo { name: "PointerToRelocations".to_string(), type_name: "DWORD".to_string(), offset: 24, size: 4 },
            InferredFieldInfo { name: "PointerToLinenumbers".to_string(), type_name: "DWORD".to_string(), offset: 28, size: 4 },
            InferredFieldInfo { name: "NumberOfRelocations".to_string(), type_name: "WORD".to_string(), offset: 32, size: 2 },
            InferredFieldInfo { name: "NumberOfLinenumbers".to_string(), type_name: "WORD".to_string(), offset: 34, size: 2 },
            InferredFieldInfo { name: "Characteristics".to_string(), type_name: "DWORD".to_string(), offset: 36, size: 4 },
        ];
        types.push(InferredTypeInfo {
            name: format!("EFI_IMAGE_SECTION_HEADER_{idx}"),
            mangled_name: String::new(),
            kind: "struct".to_string(),
            fields: sec_fields,
            size: 40,
            metadata_address: sec_va,
        });
    }
    symbols.insert(section_headers_va, "SECTION_HEADERS".to_string());

    (types, symbols)
}

impl TeLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        if bytes.len() < 40 {
            return Err(err!(loader, "MalformedHeader: Terse Executable header too small"));
        }
        let reader = ByteReader::little(bytes);
        let signature = reader.u16(0)?;
        if signature != 0x5a56 { // "VZ"
            return Err(err!(loader, "MalformedHeader: invalid TE signature"));
        }

        let machine = reader.u16(2)?;
        let num_sections = reader.u8(4)?;
        let _subsystem = reader.u8(5)?;
        let stripped_size = reader.u16(6)?;
        let entry_point_rva = reader.u32(8)?;
        let _base_of_code_rva = reader.u32(12)?;
        let image_base = reader.u64(16)?;
        let reloc_dir_rva = reader.u32(24)?;
        let reloc_dir_size = reader.u32(28)?;
        let debug_dir_rva = reader.u32(32)?;
        let debug_dir_size = reader.u32(36)?;

        // Parse sections (start at offset 40, size 40 each)
        let mut sections_info = Vec::new();
        let section_table_offset = 40;
        if bytes.len() < section_table_offset + (num_sections as usize * 40) {
            return Err(err!(loader, "MalformedHeader: Terse Executable section table truncated"));
        }

        for idx in 0..num_sections as usize {
            let offset = section_table_offset + idx * 40;
            let name = reader.fixed_string(offset, 8)?;
            let virtual_size = reader.u32(offset + 8)?;
            let virtual_address = reader.u32(offset + 12)?;
            let size_of_raw_data = reader.u32(offset + 16)?;
            let pointer_to_raw_data = reader.u32(offset + 20)?;
            let characteristics = reader.u32(offset + 36)?;

            // Adjust PointerToRawData for StrippedSize
            // AdjustedPointerToRawData = PointerToRawData - StrippedSize + 40
            let file_offset = if pointer_to_raw_data >= stripped_size as u32 {
                (pointer_to_raw_data - stripped_size as u32).saturating_add(40) as u64
            } else {
                pointer_to_raw_data as u64
            };

            sections_info.push(SectionInfo {
                name,
                virtual_address: image_base + virtual_address as u64,
                virtual_size: virtual_size as u64,
                file_offset,
                file_size: size_of_raw_data as u64,
                is_executable: (characteristics & 0x20000000) != 0,
                is_readable: (characteristics & 0x40000000) != 0,
                is_writable: (characteristics & 0x80000000) != 0,
            });
        }

        // Auto-detect architecture & load spec
        let is_64bit = match machine {
            0x8664 | 0xaa64 | 0x200 => true,
            _ => false,
        };

        let (architecture, load_spec) = select_pe_load_spec(machine, is_64bit, image_base)
            .map_err(|e| err!(loader, "{}", e))?;

        // Instantiate loader helper
        let loader = TeLoaderImpl {
            data: bytes,
            sections: &sections_info,
            stripped_size: stripped_size as u32,
        };

        // Entry point function
        let mut functions_info = Vec::new();
        let entry_point = image_base.saturating_add(entry_point_rva as u64);
        functions_info.push(FunctionInfo {
            name: "_start".to_string(),
            address: entry_point,
            size: 0,
            is_export: true,
            is_import: false,
            origin: Some("te-entry".to_string()),
            kind: Some("entry".to_string()),
            source_section: None,
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        });

        // Parse base relocations
        let mut relocations = Vec::new();
        if reloc_dir_rva != 0 && reloc_dir_size > 0 {
            if let Ok(entries) = loader.parse_relocations(reloc_dir_rva, reloc_dir_size, image_base) {
                relocations = entries;
            }
        }

        // Parse CodeView PDB debug directory info
        let pdb_debug_info = if debug_dir_rva != 0 && debug_dir_size > 0 {
            loader.parse_debug_directory(debug_dir_rva, debug_dir_size, image_base)
        } else {
            None
        };

        // Inferred header types
        let (header_types, header_symbols) = generate_te_header_types(image_base, num_sections);
        let mut global_symbols = HashMap::new();
        global_symbols.extend(header_symbols);

        let mut builder = LoadedBinaryBuilder::new(path, data)
            .format("TE")
            .architecture(architecture)
            .load_spec(load_spec)
            .entry_point(entry_point)
            .image_base(image_base)
            .is_64bit(is_64bit)
            .add_sections(sections_info)
            .add_functions(functions_info)
            .add_global_symbols(global_symbols)
            .add_inferred_types(header_types)
            .add_relocations(relocations);

        if let Some(pdb) = pdb_debug_info {
            builder = builder.pdb_debug_info(Some(pdb));
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_synthetic_te() {
        let mut data = vec![0u8; 1024];

        // 1. EFI_TE_IMAGE_HEADER
        data[0..2].copy_from_slice(b"VZ"); // Signature
        data[2..4].copy_from_slice(&0x14Cu16.to_le_bytes()); // Machine = x86
        data[4] = 1; // NumberOfSections
        data[5] = 0; // Subsystem
        data[6..8].copy_from_slice(&120u16.to_le_bytes()); // StrippedSize = 120
        data[8..12].copy_from_slice(&0x1000u32.to_le_bytes()); // AddressOfEntryPoint = 0x1000
        data[12..16].copy_from_slice(&0x1000u32.to_le_bytes()); // BaseOfCode = 0x1000
        data[16..24].copy_from_slice(&0x400000u64.to_le_bytes()); // ImageBase = 0x400000

        // DataDirectory[0] (Relocations): RVA = 0x1080, Size = 16
        data[24..28].copy_from_slice(&0x1080u32.to_le_bytes());
        data[28..32].copy_from_slice(&16u32.to_le_bytes());

        // DataDirectory[1] (Debug): RVA = 0x10A0, Size = 28
        data[32..36].copy_from_slice(&0x10A0u32.to_le_bytes());
        data[36..40].copy_from_slice(&28u32.to_le_bytes());

        // 2. Section Header (starts at 40)
        let sec_offset = 40;
        data[sec_offset..sec_offset + 5].copy_from_slice(b".text");
        // virtual_size = 0x100
        data[sec_offset + 8..sec_offset + 12].copy_from_slice(&0x100u32.to_le_bytes());
        // virtual_address = 0x1000
        data[sec_offset + 12..sec_offset + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        // size_of_raw_data = 0x100
        data[sec_offset + 16..sec_offset + 20].copy_from_slice(&0x100u32.to_le_bytes());
        // pointer_to_raw_data = 180
        data[sec_offset + 20..sec_offset + 24].copy_from_slice(&180u32.to_le_bytes());
        // characteristics = Executable | Readable (0x60000020)
        data[sec_offset + 36..sec_offset + 40].copy_from_slice(&0x60000020u32.to_le_bytes());

        // Note on offsets:
        // File offset for .text: pointer_to_raw_data (180) - stripped_size (120) + 40 = 100
        // So .text RVA 0x1000 starts at file offset 100.
        // File offset for Relocations: RVA 0x1080 is at delta 0x80 from .text RVA 0x1000.
        // So file offset of Relocations is 100 + 0x80 = 228 (0xE4).
        // File offset for Debug Directory: RVA 0x10A0 is at delta 0xA0.
        // So file offset of Debug is 100 + 0xA0 = 260 (0x104).

        // 3. Relocations data at file offset 228
        // Page RVA: 0x1000
        data[228..232].copy_from_slice(&0x1000u32.to_le_bytes());
        // Block Size: 16
        data[232..236].copy_from_slice(&16u32.to_le_bytes());
        // Entries (4 entries of 2 bytes each)
        // Entry 0: Type HIGHLOW (3) at offset 0x10
        let entry0 = (3u16 << 12) | 0x10;
        data[236..238].copy_from_slice(&entry0.to_le_bytes());
        // Entry 1: Type HIGHLOW (3) at offset 0x20
        let entry1 = (3u16 << 12) | 0x20;
        data[238..240].copy_from_slice(&entry1.to_le_bytes());

        // 4. Debug Directory at file offset 260
        // debug_type = IMAGE_DEBUG_TYPE_CODEVIEW (2)
        data[260 + 12..260 + 16].copy_from_slice(&2u32.to_le_bytes());
        // size_of_data = 36 (RSDS (4) + GUID (16) + Age (4) + Path (12))
        data[260 + 16..260 + 20].copy_from_slice(&36u32.to_le_bytes());
        // address_of_raw_data = 0x10C0 (delta 0xC0 -> file offset 100 + 0xC0 = 292 (0x124))
        data[260 + 20..260 + 24].copy_from_slice(&0x10C0u32.to_le_bytes());
        // pointer_to_raw_data = 312
        // pointer_to_raw_data (312) - stripped_size (120) + 40 = 232 (but wait, we want RSDS to start at 292,
        // so pointer_to_raw_data = 292 + 120 - 40 = 372)
        data[260 + 24..260 + 28].copy_from_slice(&372u32.to_le_bytes());

        // 5. CodeView RSDS at file offset 292
        data[292..296].copy_from_slice(b"RSDS");
        // GUID: all 0xaa
        for byte in &mut data[296..312] {
            *byte = 0xaa;
        }
        // Age: 1
        data[312..316].copy_from_slice(&1u32.to_le_bytes());
        // Path: "test.pdb\0"
        data[316..325].copy_from_slice(b"test.pdb\0");

        // Parse TE binary
        let result = TeLoader::parse(DataBuffer::Heap(data), "uefi_te_test.efi".to_string());
        assert!(result.is_ok());
        let bin = result.expect("te should parse successfully");

        assert_eq!(bin.format, "TE");
        assert_eq!(bin.entry_point, 0x401000);
        assert_eq!(bin.image_base, 0x400000);
        assert_eq!(bin.sections.len(), 1);
        assert_eq!(bin.sections[0].name, ".text");
        assert_eq!(bin.sections[0].virtual_address, 0x401000);
        assert_eq!(bin.sections[0].file_offset, 100);

        // Verify base relocations
        assert_eq!(bin.relocations.len(), 2);
        assert_eq!(bin.relocations[0].address, 0x401010);
        assert_eq!(bin.relocations[0].r_type, 3);
        assert_eq!(bin.relocations[0].size, 4);

        // Verify PDB info
        let pdb = bin.inner().pdb_debug_info.as_ref().expect("should parse PdbDebugInfo");
        assert_eq!(pdb.path_hint, Some("test.pdb".to_string()));
        assert_eq!(pdb.age, Some(1));
        assert_eq!(pdb.guid_hex, Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()));

        // Verify Inferred Header Types and Symbols
        let te_header_sym = bin.global_symbols.get(&0x400000).expect("should find TE_HEADER symbol");
        assert_eq!(te_header_sym, "TE_HEADER");
        let sec_headers_sym = bin.global_symbols.get(&0x400028).expect("should find SECTION_HEADERS symbol");
        assert_eq!(sec_headers_sym, "SECTION_HEADERS");

        let te_type = bin.inner().inferred_types.iter().find(|t| t.name == "EFI_TE_IMAGE_HEADER");
        assert!(te_type.is_some());
        let sec_type = bin.inner().inferred_types.iter().find(|t| t.name == "EFI_IMAGE_SECTION_HEADER_0");
        assert!(sec_type.is_some());
    }
}
