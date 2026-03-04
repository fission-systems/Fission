use super::*;
use std::io::Cursor;

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

    let mut cursor = Cursor::new(loader.data);
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
        }

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
            let mut thunk_cursor = Cursor::new(loader.data);
            thunk_cursor.set_position(thunk_offset);

            let mut idx = 0;
            loop {
                let raw_thunk = if loader.is_64bit {
                    u64::read_le(&mut thunk_cursor).unwrap_or(0)
                } else {
                    u32::read_le(&mut thunk_cursor).unwrap_or(0) as u64
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
                });

                symbol_map.insert(iat_addr, full_name);

                idx += 1;
            }
        }
    }

    Ok((functions, symbol_map))
}
