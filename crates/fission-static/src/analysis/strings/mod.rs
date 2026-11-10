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

/// Extract UTF-16 LE Unicode strings from binary data.
///
/// A UTF-16 string is 2-byte aligned within its own object, not within the
/// section holding it -- after an odd-length string or an odd-sized header it
/// starts at an odd byte offset -- so both byte phases are scanned. A string
/// is a run of code units in one phase, so the two phases cannot report the
/// same one, and plain ASCII is rejected in the odd phase because a pair of
/// ASCII bytes read as one code unit lands outside the printable range.
fn extract_unicode_strings(data: &[u8], base_addr: u64, min_length: usize) -> Vec<ExtractedString> {
    let mut strings = extract_unicode_strings_from(data, base_addr, min_length, false);
    if data.len() > 1 {
        strings.extend(extract_unicode_strings_from(
            &data[1..],
            base_addr + 1,
            min_length,
            true,
        ));
    }
    strings.sort_by_key(|s| s.address);
    strings
}

/// `ascii_only` restricts a run to code units whose high byte is zero and
/// whose low byte is printable ASCII. The odd phase reads every adjacent byte
/// pair of arbitrary data as a code unit, and `is_printable_unicode` also
/// accepts Latin-1/Latin Extended-A, where bytes such as `01 01` become `U+0101`;
/// unrestricted, that reported machine code and debug info as wide text.
fn extract_unicode_strings_from(
    data: &[u8],
    base_addr: u64,
    min_length: usize,
    ascii_only: bool,
) -> Vec<ExtractedString> {
    let mut strings = Vec::new();
    let mut current_string = Vec::new();
    let mut start_offset = 0;
    let mut i = 0;

    while i + 1 < data.len() {
        let low = data[i];
        let high = data[i + 1];

        // UTF-16 LE: low byte first
        let char_val = u16::from_le_bytes([low, high]);

        if is_printable_unicode(char_val) && (!ascii_only || (high == 0 && is_printable_ascii(low)))
        {
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

#[cfg(test)]
mod odd_offset_tests {
    use super::*;

    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
    }

    fn contents(strings: &[ExtractedString]) -> Vec<&str> {
        strings.iter().map(|s| s.content.as_str()).collect()
    }

    /// A UTF-16LE string only has to be 2-byte aligned *within its own
    /// object*; the section it sits in can put it at any byte offset (after an
    /// odd-length string, say). Scanning only even offsets missed all of them.
    #[test]
    fn a_wide_string_at_an_odd_offset_is_found_at_its_real_address() {
        let mut data = vec![0xFFu8];
        data.extend(wide("Hello World"));
        data.extend([0, 0]);
        let found = extract_unicode_strings(&data, 0x1000, 4);
        assert_eq!(contents(&found), ["Hello World"], "{found:?}");
        assert_eq!(found[0].address, 0x1001);
        assert_eq!(found[0].length, 22);
    }

    /// The control: the same string at an even offset keeps being found.
    #[test]
    fn a_wide_string_at_an_even_offset_is_still_found() {
        let mut data = wide("Hello World");
        data.extend([0, 0]);
        let found = extract_unicode_strings(&data, 0x1000, 4);
        assert_eq!(contents(&found), ["Hello World"], "{found:?}");
        assert_eq!(found[0].address, 0x1000);
    }

    /// Plain ASCII read two bytes at a time is not UTF-16 text; scanning the
    /// odd phase must not turn it into strings.
    #[test]
    fn plain_ascii_is_not_reported_as_utf16_in_either_phase() {
        let data = b"This is only an ASCII sentence with no wide text in it.\0";
        assert!(
            extract_unicode_strings(data, 0x1000, 4).is_empty(),
            "{:?}",
            extract_unicode_strings(data, 0x1000, 4)
        );
    }

    /// One string must not be reported twice just because both phases see it.
    #[test]
    fn a_string_is_reported_once() {
        let mut data = wide("Only Once");
        data.extend([0, 0]);
        let found = extract_unicode_strings(&data, 0x1000, 4);
        assert_eq!(found.len(), 1, "{found:?}");
    }

    /// The odd phase reads arbitrary adjacent byte pairs as code units, and
    /// `is_printable_unicode` accepts Latin Extended-A, so repeated `01 01`
    /// bytes (padding, small integers) look like "āāāā". They are not text.
    #[test]
    fn repeated_low_bytes_are_not_reported_as_wide_text_in_the_odd_phase() {
        let mut data = vec![0xFFu8];
        data.extend([0x01u8; 16]);
        data.extend([0xFFu8]);
        let found = extract_unicode_strings(&data, 0x1000, 4);
        assert!(
            found.iter().all(|s| s.address % 2 == 0),
            "odd-phase junk reported: {found:?}"
        );
    }
}
