use crate::cli::args::parse_hex_address;
use fission_loader::loader::function_view::{
    canonical_functions_sorted, is_runtime_wrapper_zero_size, prefer_function_name,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::{build_external_symbol_index, build_function_provenance_index};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BatchSelectionAccounting {
    pub(crate) functions_discovered_total: usize,
    pub(crate) functions_selected_total: usize,
    pub(crate) functions_excluded_import_count: usize,
    pub(crate) functions_excluded_runtime_wrapper_count: usize,
    pub(crate) functions_excluded_provenance_count: usize,
    pub(crate) include_nonuser_functions: bool,
}

impl BatchSelectionAccounting {
    pub(crate) fn exact(selected_total: usize, include_nonuser_functions: bool) -> Self {
        Self {
            functions_discovered_total: selected_total,
            functions_selected_total: selected_total,
            functions_excluded_import_count: 0,
            functions_excluded_runtime_wrapper_count: 0,
            functions_excluded_provenance_count: 0,
            include_nonuser_functions,
        }
    }
}

#[derive(Debug)]
pub(crate) struct BatchFunctionSelection {
    pub(crate) functions: Vec<FunctionInfo>,
    pub(crate) accounting: BatchSelectionAccounting,
}

pub(crate) fn select_batch_functions(
    binary: &LoadedBinary,
    include_nonuser_functions: bool,
    limit: Option<usize>,
) -> BatchFunctionSelection {
    let canonical = canonical_functions_sorted(binary);
    let ext = build_external_symbol_index(binary);
    let prov = build_function_provenance_index(binary, Some(&ext));
    let mut accounting = BatchSelectionAccounting {
        functions_discovered_total: canonical.len(),
        include_nonuser_functions,
        ..BatchSelectionAccounting::default()
    };

    let mut selected = Vec::with_capacity(canonical.len());
    for func in canonical {
        if !include_nonuser_functions {
            if func.is_import || func.is_thunk_like {
                accounting.functions_excluded_import_count += 1;
                continue;
            }
            if is_runtime_wrapper_zero_size(func) {
                accounting.functions_excluded_runtime_wrapper_count += 1;
                continue;
            }
            if let Some(rec) = prov.records.get(&func.address)
                && rec.exclude_from_default_batch_decompile()
            {
                accounting.functions_excluded_provenance_count += 1;
                continue;
            }
        }
        selected.push(func.clone());
    }

    if let Some(limit) = limit {
        selected.truncate(limit);
    }
    accounting.functions_selected_total = selected.len();

    BatchFunctionSelection {
        functions: selected,
        accounting,
    }
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

/// The functions named by an address file, one hex address per line.
///
/// An address the caller asked for that discovery never found is still
/// decompiled, under a synthesized `sub_<addr>`. `--addr` has always done
/// this; only the file form silently dropped such an address, which made the
/// two flags disagree about the same address on the same binary. That is
/// most visible on a stripped image whose discovery is weak -- an eval kit
/// hands you the addresses precisely because the binary does not name them.
pub(crate) fn select_functions_from_addresses_file(
    binary: &LoadedBinary,
    address_file: &Path,
) -> io::Result<Vec<FunctionInfo>> {
    let contents = fs::read_to_string(address_file)?;
    let canonical = canonical_functions_sorted(binary);
    let mut selected = Vec::new();
    for (line_number, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Naming the line is the whole point: the file is written by hand or
        // by a harness, and "one of these is wrong" is not actionable.
        let address = parse_hex_address(trimmed).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{}:{}: `{trimmed}` is not an address ({e})",
                    address_file.display(),
                    line_number + 1
                ),
            )
        })?;
        match canonical
            .iter()
            .copied()
            .find(|func| func.address == address)
            .or_else(|| binary.function_at(address))
        {
            Some(func) => selected.push(func.clone()),
            None => selected.push(FunctionInfo {
                name: format!("sub_{address:x}"),
                address,
                size: 0,
                is_export: false,
                is_import: false,
                ..Default::default()
            }),
        }
    }
    Ok(selected)
}

pub(crate) fn select_explicit_functions(
    functions: Vec<FunctionInfo>,
    include_nonuser_functions: bool,
) -> BatchFunctionSelection {
    let accounting = BatchSelectionAccounting::exact(functions.len(), include_nonuser_functions);
    BatchFunctionSelection {
        functions,
        accounting,
    }
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
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "sub_140001021".to_string(),
            address: 0x140001021,
            size: 0,
            is_export: false,
            is_import: false,
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "meaningful_name".to_string(),
            address: 0x140001080,
            size: 0x40,
            is_export: false,
            is_import: false,
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "sub_140001080".to_string(),
            address: 0x140001080,
            size: 0x40,
            is_export: false,
            is_import: false,
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "register_frame_ctor".to_string(),
            address: 0x1400010c0,
            size: 0,
            is_export: false,
            is_import: false,
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "__security_check_cookie".to_string(),
            address: 0x140001200,
            size: 0x10,
            is_export: false,
            is_import: false,
            ..Default::default()
        })
        .add_function(FunctionInfo {
            name: "puts".to_string(),
            address: 0x140001100,
            size: 0,
            is_export: false,
            is_import: true,
            ..Default::default()
        })
        .build()
        .expect("build binary")
    }

    #[test]
    fn canonical_selector_filters_internal_generic_entries() {
        let binary = test_binary();
        let selected = canonical_functions_sorted(&binary);
        let addrs: Vec<u64> = selected.iter().map(|func| func.address).collect();
        assert_eq!(
            addrs,
            vec![0x140001000, 0x140001080, 0x1400010c0, 0x140001200]
        );
    }

    #[test]
    fn canonical_selector_prefers_better_name_for_same_address() {
        let binary = test_binary();
        let func = select_function_by_address(&binary, 0x140001080).expect("selected");
        assert_eq!(func.name, "meaningful_name");
    }

    #[test]
    fn batch_selector_excludes_imports_and_runtime_wrappers_by_default() {
        let binary = test_binary();
        let selected = select_batch_functions(&binary, false, None);
        let addrs: Vec<u64> = selected.functions.iter().map(|func| func.address).collect();
        assert_eq!(addrs, vec![0x140001000, 0x140001080, 0x140001200]);
        assert_eq!(selected.accounting.functions_discovered_total, 4);
        assert_eq!(selected.accounting.functions_selected_total, 3);
        assert_eq!(selected.accounting.functions_excluded_import_count, 0);
        assert_eq!(
            selected.accounting.functions_excluded_runtime_wrapper_count,
            1
        );
        assert_eq!(selected.accounting.functions_excluded_provenance_count, 0);
        assert!(!selected.accounting.include_nonuser_functions);
    }

    #[test]
    fn batch_selector_can_restore_nonuser_functions() {
        let binary = test_binary();
        let selected = select_batch_functions(&binary, true, None);
        let addrs: Vec<u64> = selected.functions.iter().map(|func| func.address).collect();
        assert_eq!(
            addrs,
            vec![0x140001000, 0x140001080, 0x1400010c0, 0x140001200]
        );
        assert_eq!(selected.accounting.functions_discovered_total, 4);
        assert_eq!(selected.accounting.functions_selected_total, 4);
        assert_eq!(selected.accounting.functions_excluded_import_count, 0);
        assert_eq!(
            selected.accounting.functions_excluded_runtime_wrapper_count,
            0
        );
        assert_eq!(selected.accounting.functions_excluded_provenance_count, 0);
        assert!(selected.accounting.include_nonuser_functions);
    }

    #[test]
    fn explicit_selection_accounting_bypasses_batch_filter() {
        let binary = test_binary();
        let functions = vec![
            select_function_by_address(&binary, 0x140001080)
                .expect("selected")
                .clone(),
            binary
                .function_at(0x140001100)
                .expect("import selected")
                .clone(),
        ];
        let selected = select_explicit_functions(functions, false);
        assert_eq!(selected.accounting.functions_discovered_total, 2);
        assert_eq!(selected.accounting.functions_selected_total, 2);
        assert_eq!(selected.accounting.functions_excluded_import_count, 0);
        assert_eq!(
            selected.accounting.functions_excluded_runtime_wrapper_count,
            0
        );
        assert_eq!(selected.accounting.functions_excluded_provenance_count, 0);
    }
}
