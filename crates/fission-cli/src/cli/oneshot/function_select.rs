use crate::cli::args::parse_hex_address;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::fs;
use std::io;
use std::path::Path;

fn is_generic_function_name(name: &str) -> bool {
    name.starts_with("FUN_") || name.starts_with("sub_")
}

pub(crate) fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    if is_generic_function_name(candidate) != is_generic_function_name(current) {
        return !is_generic_function_name(candidate);
    }
    candidate.len() > current.len()
}

fn dedupe_exact_functions<'a>(functions: Vec<&'a FunctionInfo>) -> Vec<&'a FunctionInfo> {
    let mut deduped: Vec<&'a FunctionInfo> = Vec::with_capacity(functions.len());
    for func in functions {
        match deduped.last_mut() {
            Some(current) if current.address == func.address => {
                if prefer_function_name(&func.name, &current.name) {
                    *current = func;
                }
            }
            _ => deduped.push(func),
        }
    }
    deduped
}

fn should_filter_internal_candidate(func: &FunctionInfo, covering_end: u64) -> bool {
    if func.is_import || func.is_export || func.address >= covering_end {
        return false;
    }
    if !is_generic_function_name(&func.name) {
        return false;
    }
    let extent_end = func.address.saturating_add(func.size.max(1));
    extent_end <= covering_end
}

pub(crate) fn canonical_functions_sorted<'a>(binary: &'a LoadedBinary) -> Vec<&'a FunctionInfo> {
    let deduped = dedupe_exact_functions(binary.functions_sorted());
    let mut filtered = Vec::with_capacity(deduped.len());
    let mut covering_end = 0u64;

    for func in deduped {
        if should_filter_internal_candidate(func, covering_end) {
            continue;
        }
        if !func.is_import && func.size > 0 {
            covering_end = covering_end.max(func.address.saturating_add(func.size));
        }
        filtered.push(func);
    }

    filtered
}

pub(crate) fn select_function_by_address<'a>(
    binary: &'a LoadedBinary,
    address: u64,
) -> Option<&'a FunctionInfo> {
    let mut best = None;
    for func in canonical_functions_sorted(binary) {
        if func.address != address {
            continue;
        }
        match best {
            None => best = Some(func),
            Some(current) => {
                if prefer_function_name(&func.name, &current.name) {
                    best = Some(func);
                }
            }
        }
    }
    best
}

pub(crate) fn select_functions_from_addresses_file<'a>(
    binary: &'a LoadedBinary,
    address_file: &Path,
) -> io::Result<Vec<&'a FunctionInfo>> {
    let contents = fs::read_to_string(address_file)?;
    let canonical = canonical_functions_sorted(binary);
    let mut selected = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let address = parse_hex_address(trimmed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        if let Some(func) = canonical
            .iter()
            .copied()
            .find(|func| func.address == address)
        {
            selected.push(func);
        }
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{LoadedBinaryBuilder, SectionInfo};

    fn test_binary() -> LoadedBinary {
        LoadedBinaryBuilder::new(
            "test.bin".to_string(),
            fission_loader::loader::DataBuffer::Heap(vec![0; 0x200]),
        )
        .image_base(0x140000000)
        .entry_point(0x140001000)
        .is_64bit(true)
        .format("PE")
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x140001000,
            virtual_size: 0x200,
            file_offset: 0,
            file_size: 0x200,
            is_executable: true,
            is_writable: false,
            is_readable: true,
        })
        .add_function(FunctionInfo {
            name: "FUN_140001000".to_string(),
            address: 0x140001000,
            size: 0x80,
            is_export: false,
            is_import: false,
        })
        .add_function(FunctionInfo {
            name: "sub_140001021".to_string(),
            address: 0x140001021,
            size: 0,
            is_export: false,
            is_import: false,
        })
        .add_function(FunctionInfo {
            name: "meaningful_name".to_string(),
            address: 0x140001080,
            size: 0x40,
            is_export: false,
            is_import: false,
        })
        .add_function(FunctionInfo {
            name: "sub_140001080".to_string(),
            address: 0x140001080,
            size: 0x40,
            is_export: false,
            is_import: false,
        })
        .build()
        .expect("build binary")
    }

    #[test]
    fn canonical_selector_filters_internal_generic_entries() {
        let binary = test_binary();
        let selected = canonical_functions_sorted(&binary);
        let addrs: Vec<u64> = selected.iter().map(|func| func.address).collect();
        assert_eq!(addrs, vec![0x140001000, 0x140001080]);
    }

    #[test]
    fn canonical_selector_prefers_better_name_for_same_address() {
        let binary = test_binary();
        let func = select_function_by_address(&binary, 0x140001080).expect("selected");
        assert_eq!(func.name, "meaningful_name");
    }
}
