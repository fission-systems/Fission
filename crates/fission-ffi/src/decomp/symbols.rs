//! Symbol Provider Implementation
//!
//! Manages symbol information for the decompiler, including functions and data symbols.

use super::types::*;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

// ============================================================================
// Symbol Provider Types
// ============================================================================

/// Internal entry for a symbol (function or data)
pub(super) struct SymbolProviderEntry {
    pub name: CString,
    pub size: u32,
    pub flags: u32,
}

/// Range information for binary search
pub(super) struct SymbolProviderRange {
    pub start: u64,
    pub end: u64,
    pub entry_addr: u64,
}

/// Symbol provider state containing all symbols
pub(super) struct SymbolProviderState {
    pub functions: HashMap<u64, SymbolProviderEntry>,
    pub data: HashMap<u64, SymbolProviderEntry>,
    pub function_ranges: Vec<SymbolProviderRange>,
    pub data_ranges: Vec<SymbolProviderRange>,
}

impl SymbolProviderState {
    pub fn new(
        functions: &[fission_loader::loader::FunctionInfo],
        data_symbols: &HashMap<u64, String>,
        sections: &[fission_loader::loader::SectionInfo],
        pointer_size: Option<u32>,
    ) -> Self {
        let mut function_map = HashMap::new();
        let mut function_ranges = Vec::new();
        let mut function_addrs: Vec<u64> = functions
            .iter()
            .filter_map(|func| {
                if func.address == 0 {
                    None
                } else {
                    Some(func.address)
                }
            })
            .collect();
        function_addrs.sort_unstable();
        function_addrs.dedup();

        let mut function_sizes = HashMap::new();
        for (idx, addr) in function_addrs.iter().enumerate() {
            let next_addr = function_addrs.get(idx + 1).copied();
            let section = match find_executable_section_for_address(*addr, sections) {
                Some(section) => section,
                None => continue,
            };
            let (_, end) = match section_range(section) {
                Some(range) => range,
                None => continue,
            };
            if let Some(next) = next_addr {
                if next > *addr && next < end {
                    let size = next - *addr;
                    if let Ok(size_u32) = u32::try_from(size) {
                        if size_u32 > 0 {
                            function_sizes.insert(*addr, size_u32);
                        }
                    }
                }
            }
        }
        for func in functions {
            if func.address == 0 || func.name.is_empty() {
                continue;
            }
            if let Ok(name) = CString::new(func.name.as_str()) {
                let mut size = func.size.min(u32::MAX as u64) as u32;
                if size == 0 {
                    if let Some(estimated) = function_sizes.get(&func.address) {
                        size = *estimated;
                    }
                }
                let mut flags = SYMBOL_FLAG_FUNCTION;
                if func.is_import {
                    flags |= SYMBOL_FLAG_EXTERNAL;
                }
                function_map.insert(func.address, SymbolProviderEntry { name, size, flags });

                if size > 0 {
                    if let Some(range) = build_range(func.address, size as u64) {
                        function_ranges.push(range);
                    }
                }
            }
        }

        let mut data_map = HashMap::new();
        let mut data_ranges = Vec::new();
        let mut data_addrs: Vec<u64> = data_symbols.keys().copied().collect();
        data_addrs.sort_unstable();
        let mut data_sizes = HashMap::new();
        for (idx, addr) in data_addrs.iter().enumerate() {
            let next_addr = data_addrs.get(idx + 1).copied();
            let mut size = estimate_data_size(*addr, next_addr, sections).unwrap_or(1);
            if size == 0 {
                size = 1;
            }
            data_sizes.insert(*addr, size);
        }
        for (addr, name) in data_symbols {
            if *addr == 0 || name.is_empty() {
                continue;
            }
            if let Ok(name_cstr) = CString::new(name.as_str()) {
                let mut flags = data_flags_for_address(*addr, sections);
                let lower = name.to_ascii_lowercase();
                let is_import = lower.starts_with("__imp_") || lower.starts_with("__imp__");
                if is_import {
                    flags |= SYMBOL_FLAG_EXTERNAL;
                }
                let mut size = data_sizes.get(addr).copied().unwrap_or(1);
                if let Some(ptr_size) = pointer_size {
                    if is_import && ptr_size > 0 {
                        size = ptr_size;
                    }
                }
                data_map.insert(
                    *addr,
                    SymbolProviderEntry {
                        name: name_cstr,
                        size,
                        flags,
                    },
                );

                if let Some(range) = build_range(*addr, size as u64) {
                    data_ranges.push(range);
                }
            }
        }

        function_ranges.sort_by_key(|range| range.start);
        data_ranges.sort_by_key(|range| range.start);

        Self {
            functions: function_map,
            data: data_map,
            function_ranges,
            data_ranges,
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Determine flags for a data symbol based on section properties
fn data_flags_for_address(addr: u64, sections: &[fission_loader::loader::SectionInfo]) -> u32 {
    if let Some(section) = find_section_for_address(addr, sections) {
        let mut flags = SYMBOL_FLAG_DATA;
        if !section.is_writable {
            flags |= SYMBOL_FLAG_READONLY;
        }
        return flags;
    }

    SYMBOL_FLAG_DATA
}

/// Estimate size of a data symbol based on next symbol address
fn estimate_data_size(
    addr: u64,
    next_addr: Option<u64>,
    sections: &[fission_loader::loader::SectionInfo],
) -> Option<u32> {
    let section = find_section_for_address(addr, sections)?;
    let (_, end) = section_range(section)?;
    if let Some(next) = next_addr {
        if next > addr && next < end {
            let delta = next - addr;
            if let Ok(delta_u32) = u32::try_from(delta) {
                if delta_u32 > 0 {
                    return Some(delta_u32);
                }
            }
        }
    }
    None
}

/// Find the section containing a given address
fn find_section_for_address<'a>(
    addr: u64,
    sections: &'a [fission_loader::loader::SectionInfo],
) -> Option<&'a fission_loader::loader::SectionInfo> {
    for section in sections {
        if let Some((start, end)) = section_range(section) {
            if addr >= start && addr < end {
                return Some(section);
            }
        }
    }
    None
}

/// Find an executable section containing a given address
fn find_executable_section_for_address<'a>(
    addr: u64,
    sections: &'a [fission_loader::loader::SectionInfo],
) -> Option<&'a fission_loader::loader::SectionInfo> {
    for section in sections {
        if !section.is_executable {
            continue;
        }
        if let Some((start, end)) = section_range(section) {
            if addr >= start && addr < end {
                return Some(section);
            }
        }
    }
    None
}

/// Get the virtual address range of a section
fn section_range(section: &fission_loader::loader::SectionInfo) -> Option<(u64, u64)> {
    let size = if section.virtual_size > 0 {
        section.virtual_size
    } else {
        section.file_size
    };
    if size == 0 {
        return None;
    }
    let start = section.virtual_address;
    let end = start.saturating_add(size);
    if end <= start {
        return None;
    }
    Some((start, end))
}

/// Build a range structure for binary search
fn build_range(start: u64, size: u64) -> Option<SymbolProviderRange> {
    if size == 0 {
        return None;
    }

    let mut end = start.saturating_add(size);
    if end <= start {
        end = start.saturating_add(1);
    }

    Some(SymbolProviderRange {
        start,
        end,
        entry_addr: start,
    })
}

/// Find the entry address for a range containing the given address
fn find_range_entry(ranges: &[SymbolProviderRange], address: u64) -> Option<u64> {
    if ranges.is_empty() {
        return None;
    }

    let idx = ranges.partition_point(|range| range.start <= address);
    if idx == 0 {
        return None;
    }

    let range = &ranges[idx - 1];
    if address < range.end {
        Some(range.entry_addr)
    } else {
        None
    }
}

// ============================================================================
// FFI Callbacks
// ============================================================================

/// FFI callback for finding a symbol (data)
#[cfg(feature = "native_decomp")]
pub(super) extern "C" fn symbol_provider_find_symbol(
    userdata: *mut std::ffi::c_void,
    address: u64,
    _size: u32,
    require_start: c_int,
    out: *mut DecompSymbolInfo,
) -> c_int {
    if userdata.is_null() || out.is_null() {
        return 0;
    }

    let state = unsafe { &*(userdata as *const SymbolProviderState) };
    let entry = match state.data.get(&address) {
        Some(entry) => entry,
        None => {
            if require_start == 0 {
                if let Some(start) = find_range_entry(&state.data_ranges, address) {
                    match state.data.get(&start) {
                        Some(entry) => entry,
                        None => return 0,
                    }
                } else {
                    return 0;
                }
            } else {
                return 0;
            }
        }
    };

    unsafe {
        (*out).address = address;
        (*out).size = entry.size;
        (*out).flags = entry.flags;
        (*out).name = entry.name.as_ptr();
        (*out).name_len = entry.name.as_bytes().len().min(u32::MAX as usize) as u32;
    }

    1
}

/// FFI callback for finding a function
#[cfg(feature = "native_decomp")]
pub(super) extern "C" fn symbol_provider_find_function(
    userdata: *mut std::ffi::c_void,
    address: u64,
    out: *mut DecompSymbolInfo,
) -> c_int {
    if userdata.is_null() || out.is_null() {
        return 0;
    }

    let state = unsafe { &*(userdata as *const SymbolProviderState) };
    let entry = match state.functions.get(&address) {
        Some(entry) => entry,
        None => {
            if let Some(start) = find_range_entry(&state.function_ranges, address) {
                match state.functions.get(&start) {
                    Some(entry) => entry,
                    None => return 0,
                }
            } else {
                return 0;
            }
        }
    };

    unsafe {
        (*out).address = address;
        (*out).size = entry.size;
        (*out).flags = entry.flags;
        (*out).name = entry.name.as_ptr();
        (*out).name_len = entry.name.as_bytes().len().min(u32::MAX as usize) as u32;
    }

    1
}
