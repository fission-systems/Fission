//! Normalize PE-style import spellings for WinAPI signature database lookup (`CloseHandle`).

/// Returns the flat symbol name used in Win API tables from the signatures corpus.
///
/// Handles `KERNEL32.dll!CloseHandle`, `__imp_CloseHandle`, `__imp__CloseHandle`.
/// Ordinal imports (`*:Ordinal_N`) return `None` (no stable API row key).
#[must_use]
pub fn symbol_for_win_api_database_lookup(name: &str) -> Option<&str> {
    let name = name.trim();
    let after_bang = name
        .rsplit_once('!')
        .map(|(_, s)| s.trim())
        .unwrap_or(name);

    let sym = after_bang
        .strip_prefix("__imp__")
        .or_else(|| after_bang.strip_prefix("__imp_"))
        .unwrap_or(after_bang)
        .trim();

    if sym.is_empty() {
        return None;
    }
    if sym.starts_with("Ordinal_") || sym.contains(":Ordinal_") {
        return None;
    }
    Some(sym)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_bang_symbol() {
        assert_eq!(
            symbol_for_win_api_database_lookup("KERNEL32.dll!CloseHandle"),
            Some("CloseHandle")
        );
    }

    #[test]
    fn imp_prefix() {
        assert_eq!(symbol_for_win_api_database_lookup("__imp_CloseHandle"), Some("CloseHandle"));
    }

    #[test]
    fn imp_double_underscore() {
        assert_eq!(
            symbol_for_win_api_database_lookup("__imp__what"),
            Some("what")
        );
    }

    #[test]
    fn ordinal_skipped() {
        assert_eq!(
            symbol_for_win_api_database_lookup("KERNEL32.dll!KERNEL32.dll:Ordinal_12"),
            None
        );
        assert_eq!(symbol_for_win_api_database_lookup("foo!Ordinal_3"), None);
    }
}
