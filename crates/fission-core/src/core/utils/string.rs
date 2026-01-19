//! String and hex utilities

/// Format bytes as a hex string with spaces
pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Parse a hex string (with or without spaces/0x) into bytes
pub fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let clean = s.replace("0x", "").replace(' ', "");
    if clean.len() % 2 != 0 {
        return None;
    }

    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
        .collect()
}

/// Truncate a string with ellipsis if it exceeds max length
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
