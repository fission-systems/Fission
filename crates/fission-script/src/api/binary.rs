//! Read-only views over [`LoadedBinary`] for Rhai exposure.

use fission_loader::loader::function_view::{
    canonical_exports_sorted, canonical_functions_sorted, canonical_imports_sorted,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use rhai::{Array, Dynamic, Map};

fn hex_addr(a: u64) -> String {
    format!("0x{a:x}")
}

fn fn_import_library(f: &FunctionInfo) -> Dynamic {
    match &f.external_library {
        Some(s) => Dynamic::from(s.clone()),
        None => Dynamic::UNIT,
    }
}

pub fn function_row(f: &FunctionInfo) -> Map {
    let mut m = Map::new();
    m.insert("address".into(), Dynamic::from(hex_addr(f.address)));
    m.insert("name".into(), Dynamic::from(f.name.clone()));
    m.insert("size".into(), Dynamic::from(f.size as i64));
    m.insert("is_import".into(), Dynamic::from(f.is_import));
    m.insert("is_export".into(), Dynamic::from(f.is_export));
    m.insert("library".into(), fn_import_library(f));
    m
}

pub fn functions_array(binary: &LoadedBinary) -> Array {
    canonical_functions_sorted(binary)
        .into_iter()
        .map(|f| Dynamic::from_map(function_row(f)))
        .collect()
}

pub fn imports_array(binary: &LoadedBinary) -> Array {
    canonical_imports_sorted(binary)
        .into_iter()
        .map(|f| Dynamic::from_map(function_row(f)))
        .collect()
}

pub fn exports_array(binary: &LoadedBinary) -> Array {
    canonical_exports_sorted(binary)
        .into_iter()
        .map(|f| Dynamic::from_map(function_row(f)))
        .collect()
}

pub fn sections_array(binary: &LoadedBinary) -> Array {
    binary
        .sections
        .iter()
        .map(|s| {
            let mut m = Map::new();
            m.insert("name".into(), Dynamic::from(s.name.clone()));
            m.insert(
                "virtual_address".into(),
                Dynamic::from(hex_addr(s.virtual_address)),
            );
            m.insert("virtual_size".into(), Dynamic::from(s.virtual_size as i64));
            m.insert("file_offset".into(), Dynamic::from(hex_addr(s.file_offset)));
            m.insert("file_size".into(), Dynamic::from(s.file_size as i64));
            m.insert("executable".into(), Dynamic::from(s.is_executable));
            m.insert("readable".into(), Dynamic::from(s.is_readable));
            m.insert("writable".into(), Dynamic::from(s.is_writable));
            Dynamic::from_map(m)
        })
        .collect()
}

/// Printable ASCII strings using the same extraction rule as CLI strings: `(offset_in_file, value)`.
pub fn extract_ascii_strings(data: &[u8], min_len: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut current_bytes: Vec<u8> = Vec::with_capacity(256);
    let mut start_offset = 0usize;

    for (i, &byte) in data.iter().enumerate() {
        if (0x20..0x7f).contains(&byte) {
            if current_bytes.is_empty() {
                start_offset = i;
            }
            current_bytes.push(byte);
        } else {
            if current_bytes.len() >= min_len {
                let taken = std::mem::take(&mut current_bytes);
                let value = String::from_utf8_lossy(&taken).into_owned();
                out.push((start_offset, value));
            }
            current_bytes.clear();
        }
    }
    if current_bytes.len() >= min_len {
        let value = String::from_utf8_lossy(&current_bytes).into_owned();
        out.push((start_offset, value));
    }
    out
}

pub fn strings_array(binary: &LoadedBinary, min_len: usize) -> Array {
    let mut rows: Vec<(u64, String)> = binary
        .string_map
        .iter()
        .map(|(&addr, s)| (addr, s.clone()))
        .collect();
    rows.sort_by_key(|(a, _)| *a);

    let mut arr: Array = rows
        .into_iter()
        .map(|(addr, value)| {
            let mut m = Map::new();
            m.insert("address".into(), Dynamic::from(hex_addr(addr)));
            m.insert("value".into(), Dynamic::from(value));
            Dynamic::from_map(m)
        })
        .collect();

    if arr.is_empty() {
        let slice = binary.data.as_slice();
        for (off, value) in extract_ascii_strings(slice, min_len) {
            let mut m = Map::new();
            m.insert(
                "address".into(),
                Dynamic::from(format!("file_offset:0x{off:x}")),
            );
            m.insert("value".into(), Dynamic::from(value));
            arr.push(Dynamic::from_map(m));
        }
    }

    arr
}
