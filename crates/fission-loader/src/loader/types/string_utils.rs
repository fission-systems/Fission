/// Extract a null-terminated string from a byte slice starting at the given index.
///
/// This function finds the null terminator and returns the string up to that point.
/// Invalid UTF-8 sequences are replaced with the Unicode replacement character.
///
/// # Arguments
/// * `data` - The byte slice to extract from
/// * `start` - The starting index within the slice
///
/// # Returns
/// The extracted string, or an empty string if start is out of bounds.
///
/// # Example
/// ```ignore
/// let data = b"hello\0world";
/// assert_eq!(extract_cstring(data, 0), "hello");
/// assert_eq!(extract_cstring(data, 6), "world");
/// ```
pub fn extract_cstring(data: &[u8], start: usize) -> String {
    if start >= data.len() {
        return String::new();
    }
    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|pos| start + pos)
        .unwrap_or(data.len());
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

/// Extract a null-terminated string from a fixed-size byte array.
///
/// This is useful for parsing fixed-size name fields in binary formats
/// (e.g., PE section names which are 8 bytes, Mach-O segment names which are 16 bytes).
///
/// # Arguments
/// * `bytes` - The byte slice (typically a fixed-size field)
///
/// # Returns
/// The extracted string up to the first null byte or the end of the slice.
///
/// # Example
/// ```ignore
/// let name = b".text\0\0\0";
/// assert_eq!(extract_fixed_string(name), ".text");
/// ```
pub fn extract_fixed_string(bytes: &[u8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cstring_basic() {
        let data = b"hello\0world";
        assert_eq!(extract_cstring(data, 0), "hello");
        assert_eq!(extract_cstring(data, 6), "world");
    }

    #[test]
    fn test_extract_cstring_no_null() {
        let data = b"hello";
        assert_eq!(extract_cstring(data, 0), "hello");
    }

    #[test]
    fn test_extract_cstring_empty() {
        let data = b"\0hello";
        assert_eq!(extract_cstring(data, 0), "");
    }

    #[test]
    fn test_extract_cstring_out_of_bounds() {
        let data = b"hello";
        assert_eq!(extract_cstring(data, 100), "");
    }

    #[test]
    fn test_extract_fixed_string_basic() {
        let data = b".text\0\0\0";
        assert_eq!(extract_fixed_string(data), ".text");
    }

    #[test]
    fn test_extract_fixed_string_full() {
        let data = b"fullname";
        assert_eq!(extract_fixed_string(data), "fullname");
    }

    #[test]
    fn test_extract_fixed_string_empty() {
        let data = b"\0\0\0\0";
        assert_eq!(extract_fixed_string(data), "");
    }
}
