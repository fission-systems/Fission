//! String and hex utilities

/// Parse a hex or decimal address string.
///
/// Accepts:
/// - `0x`/`0X`-prefixed hex: `0x401000`, `0X401000`
/// - Bare hex string ≥ 4 all-hex-digit chars: `401000`
/// - Decimal: `4198400`
pub fn parse_address(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else if trimmed.len() >= 4 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        u64::from_str_radix(trimmed, 16).ok()
    } else {
        trimmed.parse::<u64>().ok()
    }
}

/// Format a 64-bit address as a lowercase hex string with `0x` prefix.
///
/// ```
/// use fission_core::format_addr;
/// assert_eq!(format_addr(0x401000), "0x401000");
/// ```
pub fn format_addr(addr: u64) -> String {
    format!("0x{:x}", addr)
}

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
    if !clean.len().is_multiple_of(2) {
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
