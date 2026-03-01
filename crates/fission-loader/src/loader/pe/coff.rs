use super::*;
use std::io::Cursor;

pub(super) fn parse_coff_symbols(
    loader: &PeLoaderImpl<'_>,
    symbol_table_offset: u32,
    symbol_count: u32,
    _image_base: u64,
) -> Result<Vec<crate::loader::types::FunctionInfo>> {
    let mut functions = Vec::new();

    let symbols_offset = symbol_table_offset as u64;
    let symbols_end = symbols_offset + (symbol_count as u64 * 18);

    if symbols_end > loader.data.len() as u64 {
        return Ok(functions);
    }

    let string_table_offset = symbols_end;

    let _string_table_size = if string_table_offset + 4 <= loader.data.len() as u64 {
        u32::from_le_bytes([
            loader.data[string_table_offset as usize],
            loader.data[(string_table_offset + 1) as usize],
            loader.data[(string_table_offset + 2) as usize],
            loader.data[(string_table_offset + 3) as usize],
        ])
    } else {
        0
    };

    let mut cursor = Cursor::new(loader.data);
    cursor.set_position(symbols_offset);

    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = cursor.position();

        let symbol = match CoffSymbol::read_le(&mut cursor) {
            Ok(s) => s,
            Err(_) => break,
        };

        let aux_count = symbol.number_of_aux_symbols;

        i += 1;

        if symbol.storage_class != storage_class::C_EXT
            && symbol.storage_class != storage_class::C_STAT
        {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let is_function = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
        if !is_function {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let name = match &symbol.name {
            SymbolName::ShortName(n) => n.clone(),
            SymbolName::LongName(offset) => {
                let str_offset = string_table_offset + *offset as u64;
                if str_offset < loader.data.len() as u64 {
                    loader.read_string_at(str_offset)
                } else {
                    continue;
                }
            }
        };

        if name.is_empty() {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        if symbol.section_number <= 0 {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        if section_idx >= loader.sections.len() {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let section = &loader.sections[section_idx];
        let func_addr = section.virtual_address + symbol.value as u64;

        functions.push(crate::loader::types::FunctionInfo {
            name,
            address: func_addr,
            size: 0,
            is_export: false,
            is_import: false,
        });

        if aux_count > 0 {
            cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
            i += aux_count as u32;
        }
    }

    Ok(functions)
}

pub(super) fn parse_coff_data_symbols(
    loader: &PeLoaderImpl<'_>,
    symbol_table_offset: u32,
    symbol_count: u32,
    _image_base: u64,
) -> Result<std::collections::HashMap<u64, String>> {
    let mut symbols = std::collections::HashMap::new();

    let symbols_offset = symbol_table_offset as u64;
    let symbols_end = symbols_offset + (symbol_count as u64 * 18);

    if symbols_end > loader.data.len() as u64 {
        return Ok(symbols);
    }

    let string_table_offset = symbols_end;

    let mut cursor = Cursor::new(loader.data);
    cursor.set_position(symbols_offset);

    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = cursor.position();

        let symbol = match CoffSymbol::read_le(&mut cursor) {
            Ok(s) => s,
            Err(_) => break,
        };

        let aux_count = symbol.number_of_aux_symbols;
        i += 1;

        if symbol.storage_class != storage_class::C_EXT
            && symbol.storage_class != storage_class::C_STAT
        {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let is_function = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
        if is_function {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let name = match &symbol.name {
            SymbolName::ShortName(n) => n.clone(),
            SymbolName::LongName(offset) => {
                let str_offset = string_table_offset + *offset as u64;
                if str_offset < loader.data.len() as u64 {
                    loader.read_string_at(str_offset)
                } else {
                    String::new()
                }
            }
        };

        let name = name.trim();
        if name.is_empty() || !should_collect_global_symbol(name) {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        if symbol.section_number <= 0 {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        if section_idx >= loader.sections.len() {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        let section = &loader.sections[section_idx];
        let data_addr = section.virtual_address + symbol.value as u64;

        let normalized = normalize_global_symbol_name(name);
        if normalized.is_empty() {
            if aux_count > 0 {
                cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
                i += aux_count as u32;
            }
            continue;
        }

        symbols.insert(data_addr, normalized);

        if aux_count > 0 {
            cursor.set_position(symbol_pos + 18 + (aux_count as u64 * 18));
            i += aux_count as u32;
        }
    }

    Ok(symbols)
}

fn should_collect_global_symbol(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("refptr") || lower.starts_with("__imp_") || lower.starts_with("__imp__")
}

fn normalize_global_symbol_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }

    let mut normalized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }

    if normalized.is_empty() {
        return normalized;
    }

    if normalized
        .as_bytes()
        .first()
        .map(|b| b.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("g_{}", normalized);
    }

    normalized
}
