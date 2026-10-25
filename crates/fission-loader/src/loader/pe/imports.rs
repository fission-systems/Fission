use super::*;

/// Resolve a PE ordinal-only import to its real exported name via the
/// bundled DLL ordinal corpus (`fission_signatures::OrdinalDatabase`, sourced
/// from RetDec's vendored ordinal tables). Returns `None` when the DLL/
/// ordinal pair isn't in the corpus, letting callers fall back to the
/// synthetic `dll:Ordinal_N` placeholder.
fn resolve_ordinal_name(dll_name: &str, ordinal: u32) -> Option<String> {
    fission_signatures::OrdinalDatabase::try_new()
        .ok()?
        .resolve(dll_name, ordinal)
        .map(str::to_string)
}

/// Upper bound on the number of imported functions per descriptor to prevent
/// memory exhaustion / infinite scanning on corrupted or malformed PE files.
const MAX_IMPORT_ENTRIES: usize = 65_536;

pub(super) fn parse_imports(
    loader: &PeLoaderImpl<'_>,
    dir_rva: u32,
    image_base: u64,
) -> Result<(
    Vec<crate::loader::types::FunctionInfo>,
    std::collections::HashMap<u64, String>,
)> {
    let offset = loader
        .rva_to_file_offset(dir_rva, image_base)
        .ok_or(err!(loader, "Invalid Import Dir RVA"))?;
    let mut functions = Vec::new();
    let mut symbol_map = std::collections::HashMap::new();

    let mut descriptor_offset = offset;

    loop {
        let desc = loader
            .read_import_descriptor(descriptor_offset)
            .unwrap_or(ImportDescriptor {
                original_first_thunk: 0,
                name: 0,
                first_thunk: 0,
            });

        if desc.original_first_thunk == 0 && desc.first_thunk == 0 {
            break;
        }
        descriptor_offset = descriptor_offset.saturating_add(20);

        let name_offset = loader
            .rva_to_file_offset(desc.name, image_base)
            .unwrap_or(0);
        let dll_name = {
            let name = loader.read_string_at(name_offset);
            if name.is_empty() {
                "unknown.dll".to_string()
            } else {
                name
            }
        };

        let thunk_rva = if desc.original_first_thunk != 0 {
            desc.original_first_thunk
        } else {
            desc.first_thunk
        };
        let thunk_offset = loader
            .rva_to_file_offset(thunk_rva, image_base)
            .unwrap_or(0);

        let iat_base_rva = desc.first_thunk;

        if thunk_offset != 0 {
            let thunk_sec = loader.sections.iter().find(|s| {
                thunk_offset >= s.file_offset && thunk_offset < s.file_offset + s.file_size
            });
            let entry_size: u64 = if loader.is_64bit { 8 } else { 4 };
            let mut idx = 0;
            while idx < MAX_IMPORT_ENTRIES {
                let thunk_entry_offset = thunk_offset + (idx as u64 * entry_size);
                if let Some(sec) = thunk_sec {
                    if thunk_entry_offset + entry_size > sec.file_offset + sec.file_size {
                        break;
                    }
                }
                let raw_thunk = if loader.is_64bit {
                    loader.read_u64(thunk_entry_offset).unwrap_or(0)
                } else {
                    loader.read_u32(thunk_entry_offset).unwrap_or(0) as u64
                };

                if raw_thunk == 0 {
                    break;
                }

                let is_ordinal = if loader.is_64bit {
                    (raw_thunk & 0x8000000000000000) != 0
                } else {
                    (raw_thunk & 0x80000000) != 0
                };

                let func_name = if is_ordinal {
                    let ordinal = (raw_thunk & 0xFFFF) as u32;
                    resolve_ordinal_name(&dll_name, ordinal)
                        .unwrap_or_else(|| format!("{}:Ordinal_{}", dll_name, ordinal))
                } else {
                    let name_rva = if loader.is_64bit {
                        (raw_thunk & 0xFFFF_FFFF) as u32
                    } else {
                        (raw_thunk & 0x7FFF_FFFF) as u32
                    };
                    let name_offset = loader.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                    if name_offset != 0 {
                        let name = loader.read_string_at(name_offset + 2);
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
                let iat_addr = image_base + iat_base_rva as u64 + (idx as u64 * entry_size);

                functions.push(crate::loader::types::FunctionInfo {
                    name: full_name.clone(),
                    address: iat_addr,
                    size: 0,
                    is_export: false,
                    is_import: true,
                    origin: Some("pe-import-table".to_string()),
                    kind: Some("import".to_string()),
                    source_section: None,
                    external_library: Some(dll_name.clone()),
                    is_thunk_like: false,
                    thunk_target: None,
                });

                symbol_map.insert(iat_addr, full_name);

                idx += 1;
            }
        }
    }

    Ok((functions, symbol_map))
}

pub(super) fn parse_delay_imports(
    loader: &PeLoaderImpl<'_>,
    dir_rva: u32,
    image_base: u64,
) -> Result<(
    Vec<crate::loader::types::FunctionInfo>,
    std::collections::HashMap<u64, String>,
    Vec<crate::loader::types::FunctionInfo>,
)> {
    let offset = loader
        .rva_to_file_offset(dir_rva, image_base)
        .ok_or(err!(loader, "Invalid Delay Import Dir RVA"))?;
    let mut functions = Vec::new();
    let mut symbol_map = std::collections::HashMap::new();
    let mut delay_proxies = Vec::new();

    let mut descriptor_offset = offset;

    loop {
        let gr_attrs = loader.read_u32(descriptor_offset).unwrap_or(0);
        let rva_dll_name = loader.read_u32(descriptor_offset + 4).unwrap_or(0);
        let rva_iat = loader.read_u32(descriptor_offset + 12).unwrap_or(0);
        let rva_int = loader.read_u32(descriptor_offset + 16).unwrap_or(0);

        if rva_dll_name == 0 {
            break;
        }

        descriptor_offset = descriptor_offset.saturating_add(32);

        let is_rva = (gr_attrs & 1) != 0;
        let get_rva = |val: u32| -> u32 {
            if is_rva || val == 0 {
                val
            } else {
                if val as u64 >= image_base {
                    (val as u64 - image_base) as u32
                } else {
                    val
                }
            }
        };

        let dll_name_rva = get_rva(rva_dll_name);
        let dll_name_offset = loader
            .rva_to_file_offset(dll_name_rva, image_base)
            .unwrap_or(0);
        let dll_name = if dll_name_offset != 0 {
            let name = loader.read_string_at(dll_name_offset);
            if name.is_empty() {
                "unknown.dll".to_string()
            } else {
                name
            }
        } else {
            "unknown.dll".to_string()
        };

        let int_rva = get_rva(rva_int);
        let iat_rva = get_rva(rva_iat);

        let thunk_rva = if int_rva != 0 { int_rva } else { iat_rva };
        let thunk_offset = loader
            .rva_to_file_offset(thunk_rva, image_base)
            .unwrap_or(0);

        let iat_base_rva = iat_rva;

        if thunk_offset != 0 {
            let thunk_sec = loader.sections.iter().find(|s| {
                thunk_offset >= s.file_offset && thunk_offset < s.file_offset + s.file_size
            });
            let entry_size: u64 = if loader.is_64bit { 8 } else { 4 };
            let mut idx = 0;
            while idx < MAX_IMPORT_ENTRIES {
                let thunk_entry_offset = thunk_offset + (idx as u64 * entry_size);
                if let Some(sec) = thunk_sec {
                    if thunk_entry_offset + entry_size > sec.file_offset + sec.file_size {
                        break;
                    }
                }
                let raw_thunk = if loader.is_64bit {
                    loader.read_u64(thunk_entry_offset).unwrap_or(0)
                } else {
                    loader.read_u32(thunk_entry_offset).unwrap_or(0) as u64
                };

                if raw_thunk == 0 {
                    break;
                }

                let is_ordinal = if loader.is_64bit {
                    (raw_thunk & 0x8000000000000000) != 0
                } else {
                    (raw_thunk & 0x80000000) != 0
                };

                let func_name = if is_ordinal {
                    let ordinal = (raw_thunk & 0xFFFF) as u32;
                    resolve_ordinal_name(&dll_name, ordinal)
                        .unwrap_or_else(|| format!("{}:Ordinal_{}", dll_name, ordinal))
                } else {
                    let name_rva = if loader.is_64bit {
                        (raw_thunk & 0xFFFF_FFFF) as u32
                    } else {
                        (raw_thunk & 0x7FFF_FFFF) as u32
                    };
                    let name_offset = loader.rva_to_file_offset(name_rva, image_base).unwrap_or(0);
                    if name_offset != 0 {
                        let name = loader.read_string_at(name_offset + 2);
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
                let iat_addr = image_base + iat_base_rva as u64 + (idx as u64 * entry_size);

                functions.push(crate::loader::types::FunctionInfo {
                    name: full_name.clone(),
                    address: iat_addr,
                    size: 0,
                    is_export: false,
                    is_import: true,
                    origin: Some("pe-delay-import-table".to_string()),
                    kind: Some("import".to_string()),
                    source_section: None,
                    external_library: Some(dll_name.clone()),
                    is_thunk_like: false,
                    thunk_target: None,
                });

                symbol_map.insert(iat_addr, full_name);

                // Try to resolve the initial value in the IAT to create the delay load helper/proxy function
                let iat_entry_rva = iat_base_rva + (idx as u64 * entry_size) as u32;
                let iat_entry_offset = loader
                    .rva_to_file_offset(iat_entry_rva, image_base)
                    .unwrap_or(0);
                let proxy_val = if iat_entry_offset != 0 {
                    if loader.is_64bit {
                        loader.read_u64(iat_entry_offset).unwrap_or(0)
                    } else {
                        loader.read_u32(iat_entry_offset).unwrap_or(0) as u64
                    }
                } else {
                    0
                };

                if proxy_val != 0 {
                    let proxy_addr = if proxy_val >= image_base {
                        proxy_val
                    } else {
                        image_base + proxy_val
                    };

                    if loader.executable_section_contains(proxy_addr) {
                        delay_proxies.push(crate::loader::types::FunctionInfo {
                            name: format!("DelayLoad_{}", func_name),
                            address: proxy_addr,
                            size: 0,
                            is_export: false,
                            is_import: false,
                            origin: Some("pe-delay-import-proxy".to_string()),
                            kind: Some("delay_proxy".to_string()),
                            source_section: None,
                            external_library: Some(dll_name.clone()),
                            is_thunk_like: false,
                            thunk_target: None,
                        });
                    }
                }

                idx += 1;
            }
        }
    }

    Ok((functions, symbol_map, delay_proxies))
}

#[cfg(test)]
mod ordinal_resolution_tests {
    use super::*;

    #[test]
    fn resolves_known_dll_ordinal_to_real_name() {
        assert_eq!(
            resolve_ordinal_name("WS2_32.dll", 1).as_deref(),
            Some("accept")
        );
    }

    #[test]
    fn falls_back_to_none_for_unknown_dll() {
        assert_eq!(resolve_ordinal_name("totally_made_up.dll", 1), None);
    }

    #[test]
    fn parse_imports_preserves_bit_31_rva_for_pe64() {
        use crate::loader::pe::PeLoaderImpl;
        use crate::loader::types::SectionInfo;

        let mut data = vec![0u8; 0x3000];
        let image_base = 0x140000000u64;

        // Import Directory at file offset 0x100 (RVA 0x100)
        let dir_rva = 0x100u32;
        // Descriptor:
        // original_first_thunk (RVA 0x200) -> bytes 0x100..0x104
        data[0x100..0x104].copy_from_slice(&0x200u32.to_le_bytes());
        // name (RVA 0x300) -> bytes 0x10c..0x110
        data[0x10c..0x110].copy_from_slice(&0x300u32.to_le_bytes());
        // first_thunk (RVA 0x400) -> bytes 0x110..0x114
        data[0x110..0x114].copy_from_slice(&0x400u32.to_le_bytes());
        // Trailing null descriptor at 0x114..0x128 is zeros.

        // DLL name at 0x300
        data[0x300..0x30c].copy_from_slice(b"testdll.dll\0");

        // ILT entry at 0x200 (RVA 0x200)
        // High RVA with bit 31 set: 0x8000_1000. Bit 63 is 0 (not an ordinal import).
        let raw_thunk: u64 = 0x0000_0000_8000_1000;
        data[0x200..0x208].copy_from_slice(&raw_thunk.to_le_bytes());
        // Terminating 0 thunk at 0x208..0x210 is zeros.

        // Section ".rdata" containing the IMAGE_IMPORT_BY_NAME structure at RVA 0x8000_1000
        // Section virtual_address = image_base + 0x8000_0000
        let sec = SectionInfo {
            name: ".rdata".to_string(),
            virtual_address: image_base + 0x8000_0000,
            virtual_size: 0x2000,
            file_offset: 0x1000,
            file_size: 0x2000,
            is_readable: true,
            is_writable: false,
            is_executable: false,
        };
        let sections = vec![sec];

        // IMAGE_IMPORT_BY_NAME: file_offset = 0x1000 + (0x8000_1000 - 0x8000_0000) = 0x2000
        // Hint (2 bytes) + "CustomImport\0"
        data[0x2000..0x2002].copy_from_slice(&0u16.to_le_bytes());
        data[0x2002..0x200f].copy_from_slice(b"CustomImport\0");

        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };

        let (functions, symbol_map) =
            parse_imports(&loader, dir_rva, image_base).expect("parse imports");
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "testdll.dll!CustomImport");
        let iat_addr = image_base + 0x400;
        assert_eq!(
            symbol_map.get(&iat_addr).map(String::as_str),
            Some("testdll.dll!CustomImport")
        );
    }

    #[test]
    fn parse_imports_thunk_loop_terminates_at_section_boundary() {
        use crate::loader::pe::PeLoaderImpl;
        use crate::loader::types::SectionInfo;

        let mut data = vec![0xFFu8; 0x2000]; // Fill with 0xFF so unterminated scans would read forever
        let image_base = 0x140000000u64;

        // Import Directory at file offset 0x100 (RVA 0x100)
        let dir_rva = 0x100u32;
        data[0x100..0x104].copy_from_slice(&0x200u32.to_le_bytes());
        data[0x104..0x10c].fill(0);
        data[0x10c..0x110].copy_from_slice(&0x300u32.to_le_bytes());
        data[0x110..0x114].copy_from_slice(&0x400u32.to_le_bytes());
        data[0x114..0x128].fill(0); // Null descriptor

        // DLL name at 0x300
        data[0x300..0x30c].copy_from_slice(b"testdll.dll\0");

        // Section for ILT at RVA 0x200, file_offset = 0x200, file_size = 16 bytes (only 2 thunks!)
        let sec_ilt = SectionInfo {
            name: ".idata".to_string(),
            virtual_address: image_base + 0x200,
            virtual_size: 16,
            file_offset: 0x200,
            file_size: 16,
            is_readable: true,
            is_writable: false,
            is_executable: false,
        };

        // Ordinal imports at 0x200 and 0x208 with bit 63 set
        let ord1: u64 = 0x8000_0000_0000_0001;
        let ord2: u64 = 0x8000_0000_0000_0002;
        data[0x200..0x208].copy_from_slice(&ord1.to_le_bytes());
        data[0x208..0x210].copy_from_slice(&ord2.to_le_bytes());
        // At 0x210, data continues as 0xFF (no zero terminator!)

        let loader = PeLoaderImpl {
            data: &data,
            sections: &[sec_ilt],
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };

        let (functions, _) = parse_imports(&loader, dir_rva, image_base).expect("parse imports");
        // Must have stopped at section boundary (exactly 2 entries), not scanning into the 0xFFs
        assert_eq!(functions.len(), 2);
    }
}
