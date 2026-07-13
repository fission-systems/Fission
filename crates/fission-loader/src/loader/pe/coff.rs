use super::*;

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

    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = symbols_offset + (i as u64 * 18);

        let symbol = match loader.read_coff_symbol(symbol_pos) {
            Ok(s) => s,
            Err(_) => break,
        };

        let aux_count = symbol.number_of_aux_symbols;

        i += 1;

        if symbol.storage_class != storage_class::C_EXT
            && symbol.storage_class != storage_class::C_STAT
        {
            i += aux_count as u32;
            continue;
        }

        let is_function = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
        if !is_function {
            i += aux_count as u32;
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
            i += aux_count as u32;
            continue;
        }

        if symbol.section_number <= 0 {
            i += aux_count as u32;
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        if section_idx >= loader.sections.len() {
            i += aux_count as u32;
            continue;
        }

        let section = &loader.sections[section_idx];
        let func_addr = section.virtual_address + symbol.value as u64;

        let mut func_size = 0u64;
        if aux_count > 0 {
            let aux_pos = symbol_pos + 18;
            if aux_pos + 8 <= loader.data.len() as u64 {
                func_size = u32::from_le_bytes([
                    loader.data[(aux_pos + 4) as usize],
                    loader.data[(aux_pos + 5) as usize],
                    loader.data[(aux_pos + 6) as usize],
                    loader.data[(aux_pos + 7) as usize],
                ]) as u64;
            }
        }

        functions.push(crate::loader::types::FunctionInfo {
            name,
            address: func_addr,
            size: func_size,
            is_export: false,
            is_import: false,
            origin: Some("pe-coff-symbol-table".to_string()),
            kind: Some("code".to_string()),
            source_section: Some(section.name.clone()),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        });

        i += aux_count as u32;
    }

    Ok(functions)
}

pub(super) fn parse_coff_function_candidates(
    loader: &PeLoaderImpl<'_>,
    symbol_table_offset: u32,
    symbol_count: u32,
) -> Result<Vec<crate::loader::types::FunctionCandidateInfo>> {
    let symbols_offset = u64::from(symbol_table_offset);
    let symbols_end = symbols_offset.saturating_add(u64::from(symbol_count) * 18);
    if symbols_end > loader.data.len() as u64 {
        return Ok(Vec::new());
    }

    let string_table_offset = symbols_end;
    let mut candidates = Vec::new();
    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = symbols_offset + u64::from(i) * 18;
        let symbol = match loader.read_coff_symbol(symbol_pos) {
            Ok(symbol) => symbol,
            Err(_) => break,
        };
        let aux_count = symbol.number_of_aux_symbols;
        i += 1;

        let has_symbol_storage = matches!(
            symbol.storage_class,
            storage_class::C_EXT | storage_class::C_STAT
        );
        let has_function_type = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
        if !has_symbol_storage || has_function_type || symbol.section_number <= 0 {
            i += u32::from(aux_count);
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        let Some(section) = loader.sections.get(section_idx) else {
            i += u32::from(aux_count);
            continue;
        };
        if !section.is_executable {
            i += u32::from(aux_count);
            continue;
        }

        let name = match &symbol.name {
            SymbolName::ShortName(name) => name.clone(),
            SymbolName::LongName(offset) => {
                let str_offset = string_table_offset + u64::from(*offset);
                if str_offset >= loader.data.len() as u64 {
                    i += u32::from(aux_count);
                    continue;
                }
                loader.read_string_at(str_offset)
            }
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            i += u32::from(aux_count);
            continue;
        }

        candidates.push(crate::loader::types::FunctionCandidateInfo {
            address: section.virtual_address + u64::from(symbol.value),
            name: name.to_string(),
            origin: "pe-coff-untyped-executable-symbol".to_string(),
            source_section: Some(section.name.clone()),
        });
        i += u32::from(aux_count);
    }

    candidates.sort_by_key(|candidate| candidate.address);
    candidates.dedup_by(|left, right| left.address == right.address);
    Ok(candidates)
}

pub(super) fn parse_coff_cfg_label_leaders(
    loader: &PeLoaderImpl<'_>,
    symbol_table_offset: u32,
    symbol_count: u32,
    _image_base: u64,
) -> Result<Vec<u64>> {
    let mut leaders = Vec::new();

    let symbols_offset = symbol_table_offset as u64;
    let symbols_end = symbols_offset + (symbol_count as u64 * 18);

    if symbols_end > loader.data.len() as u64 {
        return Ok(leaders);
    }

    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = symbols_offset + (i as u64 * 18);

        let symbol = match loader.read_coff_symbol(symbol_pos) {
            Ok(s) => s,
            Err(_) => break,
        };

        let aux_count = symbol.number_of_aux_symbols;
        i += 1;

        if symbol.storage_class != storage_class::C_LABEL || symbol.section_number <= 0 {
            i += aux_count as u32;
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        if section_idx >= loader.sections.len() {
            i += aux_count as u32;
            continue;
        }

        let section = &loader.sections[section_idx];
        if !section.is_executable {
            i += aux_count as u32;
            continue;
        }

        leaders.push(section.virtual_address + symbol.value as u64);
        i += aux_count as u32;
    }

    leaders.sort_unstable();
    leaders.dedup();
    Ok(leaders)
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

    let mut i = 0;
    while i < symbol_count {
        let symbol_pos = symbols_offset + (i as u64 * 18);

        let symbol = match loader.read_coff_symbol(symbol_pos) {
            Ok(s) => s,
            Err(_) => break,
        };

        let aux_count = symbol.number_of_aux_symbols;
        i += 1;

        if symbol.storage_class != storage_class::C_EXT
            && symbol.storage_class != storage_class::C_STAT
        {
            i += aux_count as u32;
            continue;
        }

        let is_function = (symbol.symbol_type >> 4) == symbol_type::DT_FCN;
        if is_function {
            i += aux_count as u32;
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
            i += aux_count as u32;
            continue;
        }

        if symbol.section_number <= 0 {
            i += aux_count as u32;
            continue;
        }

        let section_idx = (symbol.section_number - 1) as usize;
        if section_idx >= loader.sections.len() {
            i += aux_count as u32;
            continue;
        }

        let section = &loader.sections[section_idx];
        let data_addr = section.virtual_address + symbol.value as u64;

        let normalized = normalize_global_symbol_name(name);
        if normalized.is_empty() {
            i += aux_count as u32;
            continue;
        }

        symbols.insert(data_addr, normalized);

        i += aux_count as u32;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol_table(name: &str, value: u32, symbol_type: u16) -> Vec<u8> {
        let mut data = vec![0u8; 22];
        let name_bytes = name.as_bytes();
        data[..name_bytes.len().min(8)].copy_from_slice(&name_bytes[..name_bytes.len().min(8)]);
        data[8..12].copy_from_slice(&value.to_le_bytes());
        data[12..14].copy_from_slice(&1i16.to_le_bytes());
        data[14..16].copy_from_slice(&symbol_type.to_le_bytes());
        data[16] = storage_class::C_EXT;
        data[17] = 0;
        data[18..22].copy_from_slice(&4u32.to_le_bytes());
        data
    }

    fn executable_section() -> SectionInfo {
        SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x100,
            file_offset: 0,
            file_size: 0x100,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        }
    }

    #[test]
    fn untyped_external_symbol_in_executable_section_is_a_validation_seed() {
        let data = symbol_table("seed_fn", 0x20, 0);
        let sections = [executable_section()];
        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };

        let candidates = parse_coff_function_candidates(&loader, 0, 1).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].address, 0x401020);
        assert_eq!(candidates[0].name, "seed_fn");
    }

    #[test]
    fn typed_coff_function_is_not_duplicated_as_a_validation_seed() {
        let data = symbol_table("function", 0x20, symbol_type::DT_FCN << 4);
        let sections = [executable_section()];
        let loader = PeLoaderImpl {
            data: &data,
            sections: &sections,
            is_64bit: true,
            language_id: "x86:LE:64:default".to_string(),
        };

        assert!(
            parse_coff_function_candidates(&loader, 0, 1)
                .unwrap()
                .is_empty()
        );
    }
}
