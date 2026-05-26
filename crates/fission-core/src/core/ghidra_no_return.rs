//! Ghidra no-return function index.
//!
//! Loads `noReturnFunctionConstraints.xml` and the referenced plain-text
//! function-name lists, plus all `.hints` XML files under
//! `symbols/win32/` and `symbols/win64/`, and answers whether a given
//! (format, compiler, library, symbol) combination is known to never return.
//!
//! Matching is Ghidra-faithful:
//! - Format is matched against Ghidra's `executable_format name` attribute.
//! - Compiler key (`"golang"`, `"rustc"`) is applied only when the binary
//!   was identified as a Go or Rust binary; otherwise only the format-level
//!   list is consulted.
//! - Symbol names from the plain-text lists are normalised by stripping a
//!   single leading `_` before storing (matching Ghidra's own stripping rule).
//! - `.hints` entries require `ATTR="NO_RETURN"` and `VALUE="y"` and are
//!   looked up by (normalised DLL name, exact symbol name).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

/// Canonical Ghidra format name for PE binaries.
pub const GHIDRA_FORMAT_PE: &str = "Portable Executable (PE)";
/// Canonical Ghidra format name for ELF binaries.
pub const GHIDRA_FORMAT_ELF: &str = "Executable and Linking Format (ELF)";
/// Canonical Ghidra format name for Mach-O binaries.
pub const GHIDRA_FORMAT_MACHO: &str = "Mac OS X Mach-O";
/// Canonical Ghidra format name for DYLD cache images.
pub const GHIDRA_FORMAT_DYLD: &str = "DYLD Cache";

/// Global lazy-loaded index.
static INDEX: OnceLock<GhidraNoReturnIndex> = OnceLock::new();

/// Return the process-global `GhidraNoReturnIndex`, loaded once on first
/// call from the compiled-in `ghidra_data_root()` path.
pub fn ghidra_no_return_index() -> &'static GhidraNoReturnIndex {
    INDEX.get_or_init(|| GhidraNoReturnIndex::load(&ghidra_data_root()))
}

fn ghidra_data_root() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("FISSION_GHIDRA_DATA_DIR") {
        return std::path::PathBuf::from(path);
    }
    // Resolve from this crate's manifest directory: crates/fission-core → repo root
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
        .join("utils")
        .join("ghidra-data")
}

/// Maps `LoadedBinary.format` (as returned by the loader) to the matching
/// Ghidra `executable_format name` attribute.
///
/// Returns `None` for formats that are not represented in
/// `noReturnFunctionConstraints.xml`.
pub fn binary_format_to_ghidra_format(loader_format: &str) -> Option<&'static str> {
    let upper = loader_format.to_ascii_uppercase();
    if upper.starts_with("PE") {
        Some(GHIDRA_FORMAT_PE)
    } else if upper == "ELF" {
        Some(GHIDRA_FORMAT_ELF)
    } else if upper.starts_with("MACH-O") || upper.starts_with("MACHO") {
        Some(GHIDRA_FORMAT_MACHO)
    } else if upper.starts_with("DYLD") {
        Some(GHIDRA_FORMAT_DYLD)
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Loaded and indexed no-return facts from `utils/ghidra-data`.
pub struct GhidraNoReturnIndex {
    /// `(ghidra_format_name, compiler_key)` → set of canonical symbol names.
    ///
    /// `compiler_key` is `None` for the format-level (unconditional) list and
    /// `Some("golang")` / `Some("rustc")` for compiler-specific lists.
    ///
    /// Canonical symbol name = original name with at most one leading `_`
    /// stripped (Ghidra's documented normalisation rule for ELF names).
    format_lists: HashMap<(String, Option<String>), HashSet<String>>,
    /// Normalised DLL name (ASCII lower-case, no path) → set of exact symbol names.
    hint_lists: HashMap<String, HashSet<String>>,
}

impl GhidraNoReturnIndex {
    /// Load the index from `ghidra_data_root`.  On any I/O or parse error the
    /// affected list is silently skipped so that a missing file never prevents
    /// decompilation.
    pub fn load(ghidra_data_root: &Path) -> Self {
        let base = ghidra_data_root.join("Ghidra").join("Features").join("Base").join("data");

        // ── 1. Parse constraint manifest ────────────────────────────────────
        let constraints_path = base.join("noReturnFunctionConstraints.xml");
        let constraint_entries = std::fs::read_to_string(&constraints_path)
            .map(|text| parse_constraints_xml(&text))
            .unwrap_or_default();

        // ── 2. Load each referenced name-list file ───────────────────────────
        let mut format_lists: HashMap<(String, Option<String>), HashSet<String>> = HashMap::new();

        for (format_name, compiler_key, file_name) in &constraint_entries {
            let list_path = base.join(file_name);
            let Ok(text) = std::fs::read_to_string(&list_path) else {
                continue;
            };
            let names = parse_no_return_names(&text);
            format_lists
                .entry((format_name.clone(), compiler_key.clone()))
                .or_default()
                .extend(names);
        }

        // ── 3. Load .hints files ─────────────────────────────────────────────
        let mut hint_lists: HashMap<String, HashSet<String>> = HashMap::new();

        for subdir in &["symbols/win32", "symbols/win64"] {
            let hints_dir = base.join(subdir);
            let Ok(entries) = std::fs::read_dir(&hints_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("hints") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let (dll_name, names) = parse_hints_xml(&text);
                if dll_name.is_empty() {
                    continue;
                }
                let key = normalize_dll_name(&dll_name);
                hint_lists.entry(key).or_default().extend(names);
            }
        }

        Self { format_lists, hint_lists }
    }

    /// Return `true` if the symbol is known to never return according to the
    /// loaded Ghidra fact data.
    ///
    /// - `ghidra_format`: one of the `GHIDRA_FORMAT_*` constants, or the raw
    ///   `executable_format name` attribute value from the XML.
    /// - `compiler_key`: `Some("golang")` or `Some("rustc")` when the binary
    ///   was identified as a Go or Rust binary, otherwise `None`.
    /// - `library_name`: the `FunctionInfo.external_library` value (DLL name),
    ///   used to look up DLL-specific `.hints`.  May be `None`.
    /// - `symbol_name`: the raw symbol name from the binary.
    pub fn is_no_return(
        &self,
        ghidra_format: &str,
        compiler_key: Option<&str>,
        library_name: Option<&str>,
        symbol_name: &str,
    ) -> bool {
        let canonical = canonical_name(symbol_name);

        // 1. Format-level unconditional list (compiler_key = None).
        if let Some(names) = self.format_lists.get(&(ghidra_format.to_string(), None)) {
            if names.contains(canonical.as_str()) || names.contains(symbol_name) {
                return true;
            }
        }

        // 2. Compiler-specific list (golang / rustc).
        if let Some(ck) = compiler_key {
            if let Some(names) =
                self.format_lists.get(&(ghidra_format.to_string(), Some(ck.to_string())))
            {
                if names.contains(canonical.as_str()) || names.contains(symbol_name) {
                    return true;
                }
            }
        }

        // 3. DLL-specific .hints (requires external_library on the FunctionInfo).
        if let Some(lib) = library_name {
            let key = normalize_dll_name(lib);
            if let Some(names) = self.hint_lists.get(&key) {
                if names.contains(symbol_name) {
                    return true;
                }
            }
        }

        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal parsers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse `noReturnFunctionConstraints.xml` into a list of
/// `(format_name, compiler_key, list_filename)` triples.
///
/// Uses a simple line-by-line scanner; no XML crate dependency.
fn parse_constraints_xml(text: &str) -> Vec<(String, Option<String>, String)> {
    let mut results: Vec<(String, Option<String>, String)> = Vec::new();
    let mut current_format: Option<String> = None;
    let mut current_compiler: Option<String> = None;
    let mut in_compiler = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        // <executable_format name="...">
        if line.starts_with("<executable_format") {
            current_format = extract_attr(line, "name");
            current_compiler = None;
            in_compiler = false;
            continue;
        }

        // </executable_format>
        if line.starts_with("</executable_format") {
            current_format = None;
            current_compiler = None;
            in_compiler = false;
            continue;
        }

        // <compiler id="..."> or <compiler name="...">
        if line.starts_with("<compiler") {
            let key = extract_attr(line, "id").or_else(|| extract_attr(line, "name"));
            current_compiler = key;
            in_compiler = true;
            continue;
        }

        // </compiler>
        if line.starts_with("</compiler") {
            current_compiler = None;
            in_compiler = false;
            continue;
        }

        // <functionNamesFile>SomeFile</functionNamesFile>
        if line.starts_with("<functionNamesFile>") {
            let file_name = line
                .trim_start_matches("<functionNamesFile>")
                .trim_end_matches("</functionNamesFile>")
                .trim()
                .to_string();
            if file_name.is_empty() {
                continue;
            }
            let compiler_key = if in_compiler { current_compiler.clone() } else { None };
            if let Some(fmt) = &current_format {
                results.push((fmt.clone(), compiler_key, file_name));
            }
        }
    }

    results
}

/// Parse a plain-text no-return names file.
///
/// Lines starting with `#` are comments; blank lines are skipped.
/// Each non-blank, non-comment line is stored as-is **and** with a leading
/// `_` stripped (Ghidra's documented normalisation rule).
fn parse_no_return_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let canonical = canonical_name(trimmed);
        names.insert(canonical);
        // Also keep original in case the canonical differs and both are needed.
        names.insert(trimmed.to_string());
    }
    names
}

/// Parse a `.hints` XML file and return `(library_name, [no_return_fn_names])`.
///
/// Only entries with `ATTR="NO_RETURN"` and `VALUE="y"` are collected.
fn parse_hints_xml(text: &str) -> (String, Vec<String>) {
    let mut library_name = String::new();
    let mut names: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();

        // <LIBRARY_HINTS NAME="...">
        if line.starts_with("<LIBRARY_HINTS") {
            if let Some(n) = extract_attr(line, "NAME") {
                library_name = n;
            }
            continue;
        }

        // <HINT ATTR="NO_RETURN" VALUE="y" NAME="..." />
        if line.starts_with("<HINT") {
            let is_no_return = extract_attr(line, "ATTR")
                .map(|v| v.eq_ignore_ascii_case("NO_RETURN"))
                .unwrap_or(false);
            let is_yes = extract_attr(line, "VALUE")
                .map(|v| v.eq_ignore_ascii_case("y"))
                .unwrap_or(false);
            if is_no_return && is_yes {
                if let Some(fn_name) = extract_attr(line, "NAME") {
                    names.push(fn_name);
                }
            }
        }
    }

    (library_name, names)
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Strip at most one leading `_` from a symbol name (Ghidra ELF rule).
fn canonical_name(name: &str) -> String {
    name.strip_prefix('_').unwrap_or(name).to_string()
}

/// Normalise a DLL name for case-insensitive matching:
/// strip path, convert to ASCII lower-case.
fn normalize_dll_name(dll: &str) -> String {
    let stem = dll
        .rfind(|c| c == '/' || c == '\\')
        .map(|i| &dll[i + 1..])
        .unwrap_or(dll);
    stem.to_ascii_lowercase()
}

/// Extract the value of `attr="..."` from a single XML element line.
/// Handles both single and double quotes.
fn extract_attr(line: &str, attr: &str) -> Option<String> {
    // Search for `attr="` or `attr='`
    let search_dq = format!("{attr}=\"");
    let search_sq = format!("{attr}='");

    let (start_idx, quote_char) = if let Some(pos) = line.find(&search_dq) {
        (pos + search_dq.len(), '"')
    } else if let Some(pos) = line.find(&search_sq) {
        (pos + search_sq.len(), '\'')
    } else {
        return None;
    };

    let rest = &line[start_idx..];
    let end_idx = rest.find(quote_char)?;
    Some(rest[..end_idx].to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_attr_double_quote() {
        let line = r#"<executable_format name="Portable Executable (PE)">"#;
        assert_eq!(
            extract_attr(line, "name").as_deref(),
            Some("Portable Executable (PE)")
        );
    }

    #[test]
    fn test_extract_attr_missing() {
        assert!(extract_attr("<foo bar=\"x\">", "baz").is_none());
    }

    #[test]
    fn test_parse_constraints_xml_pe() {
        let xml = r#"
<noReturnFunctionConstraints>
    <executable_format name="Portable Executable (PE)">
        <compiler id="golang">
            <functionNamesFile>GolangFunctionsThatDoNotReturn</functionNamesFile>
        </compiler>
        <compiler name="rustc">
            <functionNamesFile>RustFunctionsThatDoNotReturn</functionNamesFile>
        </compiler>
        <functionNamesFile>PEFunctionsThatDoNotReturn</functionNamesFile>
    </executable_format>
</noReturnFunctionConstraints>
"#;
        let entries = parse_constraints_xml(xml);
        assert_eq!(entries.len(), 3);

        let (fmt, key, file) = &entries[0];
        assert_eq!(fmt, "Portable Executable (PE)");
        assert_eq!(key.as_deref(), Some("golang"));
        assert_eq!(file, "GolangFunctionsThatDoNotReturn");

        let (_, key2, file2) = &entries[1];
        assert_eq!(key2.as_deref(), Some("rustc"));
        assert_eq!(file2, "RustFunctionsThatDoNotReturn");

        let (_, key3, file3) = &entries[2];
        assert!(key3.is_none(), "format-level entry must have no compiler key");
        assert_eq!(file3, "PEFunctionsThatDoNotReturn");
    }

    #[test]
    fn test_parse_no_return_names_strips_underscore() {
        let text = "# comment\n\nabort\n_exit\nExitProcess\n";
        let names = parse_no_return_names(text);
        assert!(names.contains("abort"));
        // "_exit" → canonical "exit"
        assert!(names.contains("exit"));
        // original "_exit" also stored
        assert!(names.contains("_exit"));
        assert!(names.contains("ExitProcess"));
        assert!(!names.contains("malloc"));
    }

    #[test]
    fn test_parse_hints_xml_kernel32() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LIBRARY_HINTS NAME="KERNEL32.DLL">
    <HINT ATTR="NO_RETURN" VALUE="y" NAME="ExitProcess" />
    <HINT ATTR="NO_RETURN" VALUE="y" NAME="ExitThread" />
    <HINT ATTR="OTHER" VALUE="y" NAME="SomeOtherHint" />
</LIBRARY_HINTS>"#;
        let (lib, names) = parse_hints_xml(xml);
        assert_eq!(lib, "KERNEL32.DLL");
        assert!(names.contains(&"ExitProcess".to_string()));
        assert!(names.contains(&"ExitThread".to_string()));
        assert!(!names.contains(&"SomeOtherHint".to_string()));
    }

    #[test]
    fn test_normalize_dll_name() {
        assert_eq!(normalize_dll_name("KERNEL32.DLL"), "kernel32.dll");
        assert_eq!(normalize_dll_name("C:\\Windows\\System32\\KERNEL32.DLL"), "kernel32.dll");
        assert_eq!(normalize_dll_name("kernel32.dll"), "kernel32.dll");
    }

    #[test]
    fn test_canonical_name() {
        assert_eq!(canonical_name("_exit"), "exit");
        assert_eq!(canonical_name("ExitProcess"), "ExitProcess");
        assert_eq!(canonical_name("__underscores"), "_underscores");
    }

    #[test]
    fn test_binary_format_to_ghidra_format() {
        assert_eq!(binary_format_to_ghidra_format("PE64"), Some(GHIDRA_FORMAT_PE));
        assert_eq!(binary_format_to_ghidra_format("pe"), Some(GHIDRA_FORMAT_PE));
        assert_eq!(binary_format_to_ghidra_format("ELF"), Some(GHIDRA_FORMAT_ELF));
        assert_eq!(binary_format_to_ghidra_format("Mach-O"), Some(GHIDRA_FORMAT_MACHO));
        assert_eq!(binary_format_to_ghidra_format("RAW"), None);
    }

    /// Integration test against the real `utils/ghidra-data` tree (only runs
    /// if the workspace root is accessible at compile time).
    #[test]
    fn test_is_no_return_pe_exit_process_real_data() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf()
            .join("utils")
            .join("ghidra-data");
        if !root.exists() {
            return;
        }
        let idx = GhidraNoReturnIndex::load(&root);
        assert!(
            idx.is_no_return(GHIDRA_FORMAT_PE, None, None, "ExitProcess"),
            "ExitProcess must be no-return for PE"
        );
        assert!(
            idx.is_no_return(GHIDRA_FORMAT_ELF, None, None, "abort"),
            "abort must be no-return for ELF"
        );
        assert!(
            idx.is_no_return(
                GHIDRA_FORMAT_PE,
                None,
                Some("KERNEL32.DLL"),
                "FreeLibraryAndExitThread"
            ),
            "FreeLibraryAndExitThread must be no-return via kernel32.hints"
        );
        assert!(
            !idx.is_no_return(GHIDRA_FORMAT_PE, None, None, "malloc"),
            "malloc must NOT be no-return"
        );
        assert!(
            !idx.is_no_return(GHIDRA_FORMAT_ELF, None, None, "printf"),
            "printf must NOT be no-return"
        );
    }
}
