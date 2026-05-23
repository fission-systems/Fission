use super::*;

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
            let mut idx = 0;
            loop {
                let thunk_entry_offset = thunk_offset + (idx * if loader.is_64bit { 8 } else { 4 });
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
                    format!("{}:Ordinal_{}", dll_name, raw_thunk & 0xFFFF)
                } else {
                    let name_rva = (raw_thunk & 0x7FFFFFFF) as u32;
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
                let iat_addr =
                    image_base + iat_base_rva as u64 + (idx * if loader.is_64bit { 8 } else { 4 });

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

        let thunk_rva = if int_rva != 0 {
            int_rva
        } else {
            iat_rva
        };
        let thunk_offset = loader
            .rva_to_file_offset(thunk_rva, image_base)
            .unwrap_or(0);

        let iat_base_rva = iat_rva;

        if thunk_offset != 0 {
            let mut idx = 0;
            loop {
                let thunk_entry_offset = thunk_offset + (idx * if loader.is_64bit { 8 } else { 4 });
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
                    format!("{}:Ordinal_{}", dll_name, raw_thunk & 0xFFFF)
                } else {
                    let name_rva = (raw_thunk & 0x7FFFFFFF) as u32;
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
                let iat_addr =
                    image_base + iat_base_rva as u64 + (idx * if loader.is_64bit { 8 } else { 4 });

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
                let iat_entry_rva = iat_base_rva + (idx * if loader.is_64bit { 8 } else { 4 }) as u32;
                let iat_entry_offset = loader.rva_to_file_offset(iat_entry_rva, image_base).unwrap_or(0);
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

