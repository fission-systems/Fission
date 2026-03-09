//! ASCII/UTF-8 string scanning for data sections.
//!
//! Scans `.rdata`, `.rodata` and similar read-only sections for null-terminated
//! strings to support string inlining in decompiled output (e.g. `printf(0x408020)`
//! → `printf("Hello")`).

use std::collections::HashMap;

use crate::loader::types::SectionInfo;

/// Section names that typically contain string literals (read-only data).
/// Matches DataSectionScanner / DecompilationPipeline conventions.
const DATA_SECTION_NAMES: &[&str] = &[
    ".rdata",        // PE read-only data
    ".rodata",       // ELF read-only data
    ".data",         // PE/ELF writable data (may contain string literals)
    "__const",       // Mach-O read-only constants
    "__DATA_CONST",  // Mach-O data const
    ".data.rel.ro",  // ELF RELRO (read-only after relocation)
];

/// Minimum string length to consider (avoids noise from shorts like "AA", "x").
pub const MIN_STRING_LEN: usize = 4;

/// Maximum string length to consider (avoids huge bogus strings).
pub const MAX_STRING_LEN: usize = 1024;

/// Minimum ratio of printable ASCII bytes (0-127) to consider valid.
/// 80% matches DataSectionScanner / STRING_INLINING.md criteria.
const MIN_PRINTABLE_RATIO: f64 = 0.80;

/// Bytes considered printable: space through tilde (32-126), plus newline, cr, tab.
fn is_printable(b: u8) -> bool {
    matches!(b, 0x09 | 0x0a | 0x0d) || (b >= 0x20 && b <= 0x7e)
}

/// Check if the slice starting at `offset` looks like a null-terminated ASCII string.
/// Returns the length (excluding null) if valid, otherwise None.
fn looks_like_ascii_string(data: &[u8], offset: usize, section_size: usize) -> Option<usize> {
    if offset >= section_size {
        return None;
    }
    let mut len = 0usize;
    let mut printable_count = 0usize;
    while offset + len < section_size {
        let b = data[offset + len];
        if b == 0 {
            break;
        }
        if is_printable(b) {
            printable_count += 1;
        }
        len += 1;
        if len >= MAX_STRING_LEN {
            return None;
        }
    }
    if len < MIN_STRING_LEN {
        return None;
    }
    let ratio = printable_count as f64 / len as f64;
    if ratio < MIN_PRINTABLE_RATIO {
        return None;
    }
    Some(len)
}

/// Scan a byte slice for null-terminated ASCII strings.
///
/// # Arguments
/// * `data` - Raw section bytes
/// * `base_addr` - Virtual address of the start of the section
///
/// # Returns
/// Map of virtual address → string content (without null terminator).
pub fn scan_ascii_strings(data: &[u8], base_addr: u64) -> HashMap<u64, String> {
    let mut result = HashMap::new();
    let section_size = data.len();
    let mut offset = 0usize;

    while offset < section_size {
        if let Some(len) = looks_like_ascii_string(data, offset, section_size) {
            let va = base_addr + offset as u64;
            let bytes = &data[offset..offset + len];
            match std::str::from_utf8(bytes) {
                Ok(s) => {
                    result.insert(va, s.to_string());
                }
                Err(_) => {
                    result.insert(va, String::from_utf8_lossy(bytes).into_owned());
                }
            }
            offset += len + 1;
        } else {
            offset += 1;
        }
    }

    result
}

/// Scan all applicable data sections and merge results into a single map.
///
/// Sections whose names match `DATA_SECTION_NAMES` are scanned. Overlapping
/// addresses are resolved by first-come (lower address) wins.
pub fn scan_ascii_strings_from_sections(
    data: &[u8],
    sections: &[SectionInfo],
) -> HashMap<u64, String> {
    let mut merged = HashMap::new();
    for section in sections {
        if !is_data_section(&section.name) {
            continue;
        }
        let file_start = section.file_offset as usize;
        let file_end = file_start + section.file_size as usize;
        if file_end > data.len() || file_start >= file_end {
            continue;
        }
        let slice = &data[file_start..file_end];
        let base_addr = section.virtual_address;
        let found = scan_ascii_strings(slice, base_addr);
        for (va, s) in found {
            merged.entry(va).or_insert(s);
        }
    }
    merged
}

fn is_data_section(name: &str) -> bool {
    let name = name.trim_matches('\0');
    DATA_SECTION_NAMES.iter().any(|&n| name == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_basic() {
        let data = b"Hello\0World\0\x00\x00\x00\x00";
        let map = scan_ascii_strings(data, 0x1000);
        assert_eq!(map.get(&0x1000), Some(&"Hello".to_string()));
        assert_eq!(map.get(&0x1006), Some(&"World".to_string()));
    }

    #[test]
    fn test_scan_min_length() {
        let data = b"Hi\0"; // too short
        let map = scan_ascii_strings(data, 0);
        assert!(map.is_empty());
    }

    #[test]
    fn test_scan_with_newline() {
        let data = b"Line1\nLine2\0";
        let map = scan_ascii_strings(data, 0);
        assert_eq!(map.get(&0), Some(&"Line1\nLine2".to_string()));
    }
}
