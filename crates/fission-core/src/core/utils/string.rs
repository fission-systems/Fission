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

/// Normalize a symbol-like name into a stable decompiler-facing identifier.
///
/// This strips common import/export decorations, module prefixes, and C-style
/// signature suffixes while preserving structured names like `{lambda#1}`.
pub fn sanitize_symbol_name(name: &str) -> String {
    let mut sanitized = name.trim().to_string();
    if let Some((_, tail)) = sanitized.rsplit_once('!') {
        sanitized = tail.trim().to_string();
    }
    if let Some(stripped) = sanitized.strip_prefix("__imp_") {
        sanitized = stripped.trim().to_string();
    }
    for suffix in [" [import]", " [export]"] {
        if let Some(stripped) = sanitized.strip_suffix(suffix) {
            sanitized = stripped.trim_end().to_string();
        }
    }
    if sanitized.starts_with('{') {
        return sanitized;
    }
    if let Some(paren) = sanitized.find('(') {
        let prefix = sanitized[..paren].trim_end();
        if !prefix.is_empty() {
            sanitized = prefix
                .split_whitespace()
                .last()
                .unwrap_or(prefix)
                .to_string();
        }
    }
    sanitized
}

/// Normalize a spelled type name into a stable nominal identity.
///
/// This strips trailing pointer/reference qualifiers and C/C++ storage keywords
/// so different provenance paths can agree on the same type spelling.
pub fn normalize_named_type_identity(type_name: &str) -> Option<String> {
    let mut cleaned = type_name.trim();
    if cleaned.is_empty() {
        return None;
    }

    while let Some(stripped) = cleaned.strip_suffix('*') {
        cleaned = stripped.trim_end();
    }
    while let Some(stripped) = cleaned.strip_suffix('&') {
        cleaned = stripped.trim_end();
    }

    let tokens: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|token| !matches!(*token, "const" | "volatile" | "struct" | "class" | "enum"))
        .collect();
    if tokens.is_empty() {
        return None;
    }

    Some(tokens.join(" "))
}

#[cfg(test)]
mod tests {
    use super::{normalize_named_type_identity, sanitize_symbol_name};

    #[test]
    fn sanitize_symbol_name_strips_common_import_and_signature_wrappers() {
        assert_eq!(
            sanitize_symbol_name("USER32!__imp_MessageBoxW [import]"),
            "MessageBoxW"
        );
        assert_eq!(sanitize_symbol_name("int __cdecl foo(int)"), "foo");
        assert_eq!(sanitize_symbol_name("{lambda#1}"), "{lambda#1}");
    }

    #[test]
    fn normalize_named_type_identity_strips_cvref_noise() {
        assert_eq!(
            normalize_named_type_identity("const struct MY_TYPE *"),
            Some("MY_TYPE".to_string())
        );
        assert_eq!(
            normalize_named_type_identity("volatile class FooBar &"),
            Some("FooBar".to_string())
        );
    }
}
