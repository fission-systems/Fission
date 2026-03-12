//! String extraction and analysis module.
//!
//! Extracts ASCII and Unicode strings from binary data with configurable minimum length.

use std::collections::HashMap;

/// A string found in the binary
#[derive(Debug, Clone)]
pub struct ExtractedString {
    /// Virtual address of the string
    pub address: u64,
    /// The string content
    pub content: String,
    /// String type (ASCII or Unicode)
    pub string_type: StringType,
    /// Length in bytes
    pub length: usize,
}

/// Type of string
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringType {
    /// ASCII string (single-byte characters)
    Ascii,
    /// UTF-16 LE Unicode string
    Unicode,
}

/// Extract all strings from binary data
pub fn extract_strings(data: &[u8], base_addr: u64, min_length: usize) -> Vec<ExtractedString> {
    let mut strings = Vec::new();

    // Extract ASCII strings
    strings.extend(extract_ascii_strings(data, base_addr, min_length));

    // Extract Unicode strings
    strings.extend(extract_unicode_strings(data, base_addr, min_length));

    strings
}

/// Extract ASCII strings from binary data
fn extract_ascii_strings(data: &[u8], base_addr: u64, min_length: usize) -> Vec<ExtractedString> {
    let mut strings = Vec::new();
    let mut current_string = Vec::new();
    let mut start_offset = 0;

    for (i, &byte) in data.iter().enumerate() {
        if is_printable_ascii(byte) {
            if current_string.is_empty() {
                start_offset = i;
            }
            current_string.push(byte);
        } else {
            if current_string.len() >= min_length {
                if let Ok(s) = String::from_utf8(current_string.clone()) {
                    strings.push(ExtractedString {
                        address: base_addr + start_offset as u64,
                        content: s,
                        string_type: StringType::Ascii,
                        length: current_string.len(),
                    });
                }
            }
            current_string.clear();
        }
    }

    // Check last string
    if current_string.len() >= min_length {
        if let Ok(s) = String::from_utf8(current_string) {
            let len = s.len();
            strings.push(ExtractedString {
                address: base_addr + start_offset as u64,
                content: s,
                string_type: StringType::Ascii,
                length: len,
            });
        }
    }

    strings
}

/// Extract UTF-16 LE Unicode strings from binary data
fn extract_unicode_strings(data: &[u8], base_addr: u64, min_length: usize) -> Vec<ExtractedString> {
    let mut strings = Vec::new();
    let mut current_string = Vec::new();
    let mut start_offset = 0;
    let mut i = 0;

    while i + 1 < data.len() {
        let low = data[i];
        let high = data[i + 1];

        // UTF-16 LE: low byte first
        let char_val = u16::from_le_bytes([low, high]);

        if is_printable_unicode(char_val) {
            if current_string.is_empty() {
                start_offset = i;
            }
            current_string.push(char_val);
        } else {
            if current_string.len() >= min_length {
                if let Ok(s) = String::from_utf16(&current_string) {
                    strings.push(ExtractedString {
                        address: base_addr + start_offset as u64,
                        content: s,
                        string_type: StringType::Unicode,
                        length: current_string.len() * 2,
                    });
                }
            }
            current_string.clear();
        }

        i += 2;
    }

    // Check last string
    if current_string.len() >= min_length {
        if let Ok(s) = String::from_utf16(&current_string) {
            let len = current_string.len() * 2;
            strings.push(ExtractedString {
                address: base_addr + start_offset as u64,
                content: s,
                string_type: StringType::Unicode,
                length: len,
            });
        }
    }

    strings
}

/// Check if a byte is printable ASCII
fn is_printable_ascii(byte: u8) -> bool {
    // Printable ASCII: space (0x20) to tilde (0x7E), plus tab (0x09), newline (0x0A), carriage return (0x0D)
    matches!(byte, 0x09 | 0x0A | 0x0D | 0x20..=0x7E)
}

/// Check if a Unicode character is printable
fn is_printable_unicode(char_val: u16) -> bool {
    // Basic Latin and common printable ranges
    matches!(char_val, 0x09 | 0x0A | 0x0D | 0x20..=0x7E | 0xA0..=0xFF | 0x100..=0x17F)
}

/// Build a quick lookup table: string content -> addresses
pub fn build_string_lookup(strings: &[ExtractedString]) -> HashMap<String, Vec<u64>> {
    let mut lookup = HashMap::new();
    for s in strings {
        lookup
            .entry(s.content.clone())
            .or_insert_with(Vec::new)
            .push(s.address);
    }
    lookup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_extraction() {
        let data = b"Hello, World!\x00\x00\x00Test";
        let strings = extract_ascii_strings(data, 0x1000, 4);

        assert_eq!(strings.len(), 2);
        assert_eq!(strings[0].content, "Hello, World!");
        assert_eq!(strings[1].content, "Test");
        assert_eq!(strings[0].address, 0x1000);
    }

    #[test]
    fn test_min_length_filter() {
        let data = b"Hi\x00Test\x00LongerString";
        let strings = extract_ascii_strings(data, 0x1000, 4);

        // "Hi" should be filtered out (< 4 chars)
        assert_eq!(strings.len(), 2);
        assert!(strings.iter().all(|s| s.content.len() >= 4));
    }

    #[test]
    fn test_unicode_extraction() {
        // "Test" in UTF-16 LE
        let data = b"T\x00e\x00s\x00t\x00\x00\x00";
        let strings = extract_unicode_strings(data, 0x1000, 4);

        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].content, "Test");
        assert_eq!(strings[0].string_type, StringType::Unicode);
    }
}
