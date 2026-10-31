//! Normalize PE-style import spellings for WinAPI signature database lookup (`CloseHandle`).

/// Returns the flat symbol name used in Win API tables from the signatures corpus.
///
/// Handles `KERNEL32.dll!CloseHandle`, `__imp_CloseHandle`, `__imp__CloseHandle`,
/// and the 32-bit x86 decorations of `__stdcall` (`_CloseHandle@4`) and
/// `__fastcall` (`@CloseHandle@4`).
/// Ordinal imports (`*:Ordinal_N`) return `None` (no stable API row key).
#[must_use]
pub fn symbol_for_win_api_database_lookup(name: &str) -> Option<&str> {
    let name = name.trim();
    let after_bang = name.rsplit_once('!').map(|(_, s)| s.trim()).unwrap_or(name);

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
    Some(strip_x86_call_decoration(sym))
}

/// Remove the `_Name@N` / `@Name@N` / `Name@N` decoration a 32-bit x86 linker
/// puts on `__stdcall` and `__fastcall` symbols (`N` is the argument bytes).
///
/// The `@N` suffix is what marks a symbol as decorated, so it is required
/// before a leading `_` or `@` is removed: `_lopen` and `_hread` are real
/// Windows API names that begin with an underscore, and an MSVC C++ name
/// (`?Name@@...`) is not this scheme at all.
fn strip_x86_call_decoration(sym: &str) -> &str {
    if sym.starts_with('?') {
        return sym;
    }
    let Some((base, bytes)) = sym.rsplit_once('@') else {
        return sym;
    };
    if bytes.is_empty() || !bytes.bytes().all(|byte| byte.is_ascii_digit()) {
        return sym;
    }
    let name = base
        .strip_prefix('@')
        .or_else(|| base.strip_prefix('_'))
        .unwrap_or(base);
    if name.is_empty() { sym } else { name }
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
        assert_eq!(
            symbol_for_win_api_database_lookup("__imp_CloseHandle"),
            Some("CloseHandle")
        );
    }

    #[test]
    fn imp_double_underscore() {
        assert_eq!(
            symbol_for_win_api_database_lookup("__imp__what"),
            Some("what")
        );
    }

    #[test]
    fn x86_calling_convention_decorations_are_removed() {
        for (decorated, plain) in [
            ("_CloseHandle@4", "CloseHandle"),
            ("__imp__CloseHandle@4", "CloseHandle"),
            ("@CloseHandle@4", "CloseHandle"),
            ("__imp_@CloseHandle@4", "CloseHandle"),
            ("CloseHandle@4", "CloseHandle"),
            ("KERNEL32.dll!_ExitProcess@4", "ExitProcess"),
            ("_VirtualAlloc@16", "VirtualAlloc"),
        ] {
            assert_eq!(
                symbol_for_win_api_database_lookup(decorated),
                Some(plain),
                "{decorated}"
            );
        }
    }

    /// Leading underscores that belong to the name, and names that are not this
    /// scheme, are left alone.
    #[test]
    fn real_underscore_names_and_cpp_names_are_not_mangled() {
        assert_eq!(symbol_for_win_api_database_lookup("_lopen"), Some("_lopen"));
        assert_eq!(symbol_for_win_api_database_lookup("_hread"), Some("_hread"));
        assert_eq!(
            symbol_for_win_api_database_lookup("__imp__lclose"),
            Some("lclose")
        );
        assert_eq!(
            symbol_for_win_api_database_lookup("?Method@Class@@QAEXXZ"),
            Some("?Method@Class@@QAEXXZ")
        );
        assert_eq!(
            symbol_for_win_api_database_lookup("@NoDigits@x"),
            Some("@NoDigits@x")
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
