//! Ghidra-style `.ldefs` index builder.
//!
//! Ghidra resolves a `.cspec` file via `(language_id, compiler_spec_id)`:
//!
//! ```xml
//! <language id="x86:LE:64:default">
//!   <compiler id="windows" spec="x86-64-win.cspec"/>
//!   <compiler id="gcc"     spec="x86-64-gcc.cspec"/>
//! </language>
//! ```
//!
//! This module scans all `.ldefs` files under `languages_root` and builds a flat
//! `HashMap<(language_id, compiler_spec_id), cspec_filename>` index.  The index is
//! computed once and cached globally.
//!
//! Unlike the previous stem-guessing approach, this gives us **exact** file names for
//! every `(language_id, compiler_spec_id)` pair supported by Ghidra — including all
//! AArch64 variants, ARM, MIPS, PowerPC, RISC-V, Sparc, LoongArch, etc.

use crate::midend::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `(language_id, compiler_spec_id)` → relative `.cspec` filename stem (no extension).
///
/// Key:   `("x86:LE:64:default", "gcc")` → `"x86-64-gcc"`
/// Value: the stem used to load `<language_dir>/<stem>.cspec`.
pub type LdefsIndex = HashMap<(String, String), LdefsEntry>;

/// One resolved entry from the .ldefs index.
#[derive(Debug, Clone)]
pub struct LdefsEntry {
    /// Absolute path to the directory that contains the `.cspec` file.
    pub language_dir: PathBuf,
    /// `.cspec` filename including extension, e.g. `"x86-64-gcc.cspec"`.
    pub cspec_filename: String,
    /// `.pspec` filename from `processorspec="..."` on `<language>`, if present.
    /// e.g. `"x86-64.pspec"`, `"AARCH64.pspec"`, `"ARMCortex.pspec"`.
    pub pspec_filename: Option<String>,
    /// `.dwarf` filename from
    /// `<external_name tool="DWARF.register.mapping.file" name="..."/>` inside
    /// `<language>`, if present. e.g. `"x86-64.dwarf"`, `"AARCH64.dwarf"`.
    /// Unlike `pspec_filename` this is a *child element*, not a `<language>`
    /// attribute, and Ghidra's real `.ldefs` files declare it after the
    /// `<compiler>` children — so the parser buffers compiler entries and
    /// backfills this at `</language>` rather than capturing it inline.
    pub dwarf_mapping_filename: Option<String>,
}

impl LdefsEntry {
    /// Absolute path to the `.cspec` file.
    pub fn cspec_path(&self) -> PathBuf {
        self.language_dir.join(&self.cspec_filename)
    }

    /// Absolute path to the `.pspec` file, or `None` if no `processorspec` was declared.
    pub fn pspec_path(&self) -> Option<PathBuf> {
        self.pspec_filename
            .as_deref()
            .map(|name| self.language_dir.join(name))
    }

    /// Absolute path to the `.dwarf` register mapping file, or `None` if the
    /// language declares no `DWARF.register.mapping.file` external name.
    pub fn dwarf_mapping_path(&self) -> Option<PathBuf> {
        self.dwarf_mapping_filename
            .as_deref()
            .map(|name| self.language_dir.join(name))
    }
}

// ── Global cache ─────────────────────────────────────────────────────────────

static LDEFS_INDEX: OnceLock<LdefsIndex> = OnceLock::new();
static LANGUAGE_SLASPEC_INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();

/// `language_id` → absolute path to the checked-in `.slaspec` entry file.
pub type LanguageSlaspecIndex = HashMap<String, PathBuf>;

/// Return (or build and cache) the global `language_id` → `.slaspec` index.
pub fn global_language_slaspec_index(languages_root: &Path) -> &'static LanguageSlaspecIndex {
    LANGUAGE_SLASPEC_INDEX.get_or_init(|| build_language_slaspec_index(languages_root))
}

/// Return (or build and cache) the global `.ldefs` index for `languages_root`.
///
/// The first call scans the entire `languages_root` tree.  Subsequent calls return
/// the cached result immediately.  If `languages_root` doesn't exist the index is
/// empty and the caller falls back to the legacy heuristic.
pub fn global_ldefs_index(languages_root: &Path) -> &'static LdefsIndex {
    LDEFS_INDEX.get_or_init(|| build_ldefs_index(languages_root))
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Scan `languages_root` for all `.ldefs` files and return the full index.
pub fn build_ldefs_index(languages_root: &Path) -> LdefsIndex {
    let mut index = HashMap::default();
    if let Ok(entries) = std::fs::read_dir(languages_root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                scan_processor_dir(&dir, &mut index);
            }
        }
    }
    index
}

pub fn build_language_slaspec_index(languages_root: &Path) -> LanguageSlaspecIndex {
    let mut index = HashMap::default();
    if let Ok(entries) = std::fs::read_dir(languages_root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                scan_processor_dir_for_slaspec(&dir, &mut index);
            }
        }
    }
    index
}

fn scan_processor_dir_for_slaspec(dir: &Path, index: &mut LanguageSlaspecIndex) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_processor_dir_for_slaspec(&path, index);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ldefs") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                parse_ldefs_slaspec_into_index(&contents, dir, index);
            }
        }
    }
}

fn parse_ldefs_slaspec_into_index(contents: &str, dir: &Path, index: &mut LanguageSlaspecIndex) {
    let mut rest = contents;
    loop {
        let Some(lt) = rest.find('<') else { break };
        rest = &rest[lt + 1..];
        if rest.starts_with("!--") {
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
            } else {
                break;
            }
            continue;
        }
        let tag_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let tag = &rest[..tag_end];
        if tag == "language" {
            let close = find_tag_close(rest);
            let segment = &rest[..close];
            if let (Some(language_id), Some(slafile)) = (
                extract_attr(segment, "id"),
                extract_attr(segment, "slafile"),
            ) {
                let slaspec = slafile_to_slaspec_path(dir, slafile);
                index.insert(language_id.to_string(), slaspec);
            }
            if close < rest.len() {
                rest = &rest[close + 1..];
            } else {
                break;
            }
        } else {
            let close = find_tag_close(rest);
            if close < rest.len() {
                rest = &rest[close + 1..];
            } else {
                break;
            }
        }
    }
}

fn slafile_to_slaspec_path(dir: &Path, slafile: &str) -> PathBuf {
    let stem = slafile.strip_suffix(".sla").unwrap_or(slafile);
    dir.join(format!("{stem}.slaspec"))
}

fn scan_processor_dir(dir: &Path, index: &mut LdefsIndex) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories (e.g. Toy/old/v01stuff/)
            scan_processor_dir(&path, index);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ldefs") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                parse_ldefs_into_index(&contents, dir, index);
            }
        }
    }
}

/// State machine parser for a single `.ldefs` file.
///
/// Ghidra's `.ldefs` XML looks like:
/// ```xml
/// <language_definitions>
///   <language ... id="x86:LE:64:default">
///     <compiler id="windows" spec="x86-64-win.cspec"/>
///     <compiler id="gcc"     spec="x86-64-gcc.cspec"/>
///   </language>
/// </language_definitions>
/// ```
///
/// We use a minimal hand-written state machine — no XML library dependency.
///
/// `<compiler>` entries are buffered per-`<language>` block and only inserted
/// into `index` at `</language>`, because `<external_name
/// tool="DWARF.register.mapping.file">` is a child element that Ghidra's real
/// `.ldefs` files declare *after* the `<compiler>` children — capturing it
/// inline (like the `processorspec` attribute) would miss it.
fn parse_ldefs_into_index(contents: &str, dir: &Path, index: &mut LdefsIndex) {
    let mut current_language_id: Option<String> = None;
    // `processorspec` attribute from `<language>` — shared by all `<compiler>` children.
    let mut current_pspec_filename: Option<String> = None;
    let mut current_dwarf_filename: Option<String> = None;
    let mut pending_compilers: Vec<(String, String)> = Vec::new();

    let commit_current = |lang_id: Option<String>,
                          pspec: Option<String>,
                          dwarf: Option<String>,
                          compilers: &mut Vec<(String, String)>,
                          index: &mut LdefsIndex| {
        if let Some(id) = lang_id {
            if compilers.is_empty() {
                let default_cspec = pspec
                    .as_deref()
                    .and_then(|p| p.strip_suffix(".pspec"))
                    .map(|stem| format!("{stem}.cspec"))
                    .unwrap_or_default();
                index.insert(
                    (id, "default".to_string()),
                    LdefsEntry {
                        language_dir: dir.to_path_buf(),
                        cspec_filename: default_cspec,
                        pspec_filename: pspec,
                        dwarf_mapping_filename: dwarf,
                    },
                );
            } else {
                for (compiler_id, cspec_filename) in compilers.drain(..) {
                    index.insert(
                        (id.clone(), compiler_id),
                        LdefsEntry {
                            language_dir: dir.to_path_buf(),
                            cspec_filename,
                            pspec_filename: pspec.clone(),
                            dwarf_mapping_filename: dwarf.clone(),
                        },
                    );
                }
            }
        }
    };

    let mut rest = contents;
    loop {
        // Find the next '<'
        let Some(lt) = rest.find('<') else { break };
        rest = &rest[lt + 1..];

        // Skip comments
        if rest.starts_with("!--") {
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
            } else {
                break;
            }
            continue;
        }

        // Read tag name. A leading '/' (closing tag, e.g. `</language>`) must
        // stay part of the name -- scanning for the name-terminator has to
        // start *after* it, or `</language>` reads as an empty tag and the
        // "/language" arm below never fires.
        let tag_start = usize::from(rest.starts_with('/'));
        let tag_end = rest[tag_start..]
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .map(|pos| pos + tag_start)
            .unwrap_or(rest.len());
        let tag = &rest[..tag_end];

        match tag {
            "language" => {
                // If a previous language was not explicitly closed by </language>, commit it now.
                commit_current(
                    current_language_id.take(),
                    current_pspec_filename.take(),
                    current_dwarf_filename.take(),
                    &mut pending_compilers,
                    index,
                );

                let close = find_tag_close(rest);
                let segment = &rest[..close];
                let is_self_closing = segment.trim_end().ends_with('/');

                let lang_id = extract_attr(segment, "id").map(str::to_string);
                let pspec = extract_attr(segment, "processorspec").map(str::to_string);
                let dwarf = None;

                if let Some(spec) =
                    extract_attr(segment, "spec").or_else(|| extract_attr(segment, "cspec"))
                {
                    let compiler_id = extract_attr(segment, "compiler")
                        .unwrap_or("default")
                        .to_string();
                    pending_compilers.push((compiler_id, spec.to_string()));
                }

                if is_self_closing {
                    commit_current(lang_id, pspec, dwarf, &mut pending_compilers, index);
                    current_language_id = None;
                    current_pspec_filename = None;
                    current_dwarf_filename = None;
                } else {
                    current_language_id = lang_id;
                    current_pspec_filename = pspec;
                    current_dwarf_filename = dwarf;
                }

                if close < rest.len() {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
            "/language" => {
                commit_current(
                    current_language_id.take(),
                    current_pspec_filename.take(),
                    current_dwarf_filename.take(),
                    &mut pending_compilers,
                    index,
                );
                current_pspec_filename = None;
                current_dwarf_filename = None;
                let close = find_tag_close(rest);
                if close < rest.len() {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
            "compiler" => {
                let close = find_tag_close(rest);
                let segment = &rest[..close];
                if current_language_id.is_some() {
                    if let (Some(compiler_id), Some(spec)) =
                        (extract_attr(segment, "id"), extract_attr(segment, "spec"))
                    {
                        pending_compilers.push((compiler_id.to_string(), spec.to_string()));
                    }
                }
                if close < rest.len() {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
            "external_name" => {
                let close = find_tag_close(rest);
                let segment = &rest[..close];
                if current_language_id.is_some() {
                    if extract_attr(segment, "tool") == Some("DWARF.register.mapping.file") {
                        current_dwarf_filename = extract_attr(segment, "name").map(str::to_string);
                    }
                }
                if close < rest.len() {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
            _ => {
                let close = find_tag_close(rest);
                if close < rest.len() {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
        }
    }

    commit_current(
        current_language_id.take(),
        current_pspec_filename.take(),
        current_dwarf_filename.take(),
        &mut pending_compilers,
        index,
    );
}

/// Find the byte offset of the closing `>` for an XML tag, skipping any `>`
/// characters inside quoted attribute values (`"..."` or `'...'`).
fn find_tag_close(s: &str) -> usize {
    let mut in_quote = None;
    for (i, c) in s.char_indices() {
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_quote = Some(c);
                } else if c == '>' {
                    return i;
                }
            }
        }
    }
    s.len()
}

/// Extract an attribute value by name from an XML tag segment (e.g., `key="value"` or `key='value'`).
/// Matches attributes as distinct tokens (avoiding prefix/substring collisions) and tolerates
/// single quotes as well as optional whitespace around `=`.
fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    let mut chars = segment.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            chars.next();
            while let Some((_, qc)) = chars.next() {
                if qc == quote {
                    break;
                }
            }
            continue;
        }
        if c == '/' || c == '>' {
            chars.next();
            continue;
        }

        // We are at the start of a token (tag name or attribute name)
        let name_start = i;
        while let Some(&(next_i, next_c)) = chars.peek() {
            if next_c.is_ascii_whitespace()
                || next_c == '='
                || next_c == '/'
                || next_c == '>'
                || next_c == '"'
                || next_c == '\''
            {
                break;
            }
            chars.next();
        }
        let name_end = chars.peek().map_or(segment.len(), |&(ni, _)| ni);
        let name = &segment[name_start..name_end];

        // Skip whitespace between name and '='
        while let Some(&(_, next_c)) = chars.peek() {
            if next_c.is_ascii_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        // Check if followed by '='
        if let Some(&(_, '=')) = chars.peek() {
            chars.next(); // consume '='

            // Skip whitespace between '=' and quote
            while let Some(&(_, next_c)) = chars.peek() {
                if next_c.is_ascii_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }

            // Check quote
            if let Some(&(_, quote)) = chars.peek() {
                if quote == '"' || quote == '\'' {
                    chars.next(); // consume opening quote
                    let val_start = chars.peek().map_or(segment.len(), |&(vi, _)| vi);
                    let mut val_end = val_start;
                    while let Some((vi, vc)) = chars.next() {
                        if vc == quote {
                            val_end = vi;
                            break;
                        }
                    }
                    if name == key {
                        return Some(&segment[val_start..val_end]);
                    }
                }
            }
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aarch64_ldefs() {
        let xml = r#"<?xml version="1.1" encoding="UTF-8"?>
<language_definitions>
   <language processor="AARCH64"
             endian="little"
             size="64"
             id="AARCH64:LE:64:v8A">
     <compiler name="default" spec="AARCH64.cspec" id="default"/>
     <compiler name="Visual Studio" spec="AARCH64_win.cspec" id="windows"/>
     <compiler name="golang" spec="AARCH64_golang.cspec" id="golang"/>
   </language>
</language_definitions>"#;

        use std::path::PathBuf;
        let dir = PathBuf::from("/tmp/AARCH64");
        let mut index = HashMap::default();
        parse_ldefs_into_index(xml, &dir, &mut index);

        let key_default = ("AARCH64:LE:64:v8A".to_string(), "default".to_string());
        let key_win = ("AARCH64:LE:64:v8A".to_string(), "windows".to_string());
        let key_go = ("AARCH64:LE:64:v8A".to_string(), "golang".to_string());

        assert_eq!(
            index.get(&key_default).map(|e| e.cspec_filename.as_str()),
            Some("AARCH64.cspec")
        );
        assert_eq!(
            index.get(&key_win).map(|e| e.cspec_filename.as_str()),
            Some("AARCH64_win.cspec")
        );
        assert_eq!(
            index.get(&key_go).map(|e| e.cspec_filename.as_str()),
            Some("AARCH64_golang.cspec")
        );
    }

    #[test]
    fn parse_x86_ldefs_multiblock() {
        let xml = r#"<language_definitions>
  <language id="x86:LE:32:default">
    <compiler id="windows" spec="x86win.cspec"/>
    <compiler id="gcc"     spec="x86gcc.cspec"/>
  </language>
  <language id="x86:LE:64:default">
    <compiler id="windows" spec="x86-64-win.cspec"/>
    <compiler id="gcc"     spec="x86-64-gcc.cspec"/>
    <compiler id="golang"  spec="x86-64-golang.cspec"/>
    <compiler id="swift"   spec="x86-64-swift.cspec"/>
  </language>
</language_definitions>"#;

        use std::path::PathBuf;
        let dir = PathBuf::from("/tmp/x86");
        let mut index = HashMap::default();
        parse_ldefs_into_index(xml, &dir, &mut index);

        assert_eq!(
            index
                .get(&("x86:LE:64:default".to_string(), "windows".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86-64-win.cspec")
        );
        assert_eq!(
            index
                .get(&("x86:LE:64:default".to_string(), "gcc".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86-64-gcc.cspec")
        );
        assert_eq!(
            index
                .get(&("x86:LE:32:default".to_string(), "gcc".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86gcc.cspec")
        );
    }
    /// Integration test: build the real index from `utils/sleigh-specs/languages`.
    ///
    /// Validates canonical Ghidra pairs that every Fission target depends on.
    #[test]
    fn real_ldefs_index_covers_canonical_pairs() {
        // Navigate to the repo root relative to this crate's CARGO_MANIFEST_DIR.
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2) // crates/fission-pcode → repo root
            .unwrap()
            .to_path_buf();
        let languages_root = repo_root.join("utils/sleigh-specs/languages");

        if !languages_root.exists() {
            // Skip if not in a full repo checkout.
            return;
        }

        let index = build_ldefs_index(&languages_root);

        // x86-64
        let got = index
            .get(&("x86:LE:64:default".to_string(), "gcc".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("x86-64-gcc.cspec"), "x86-64 gcc cspec");

        let got = index
            .get(&("x86:LE:64:default".to_string(), "windows".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("x86-64-win.cspec"), "x86-64 windows cspec");

        let got = index
            .get(&("x86:LE:64:default".to_string(), "swift".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("x86-64-swift.cspec"), "x86-64 swift cspec");

        // x86-32
        let got = index
            .get(&("x86:LE:32:default".to_string(), "windows".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("x86win.cspec"), "x86-32 windows cspec");

        // AARCH64
        let got = index
            .get(&("AARCH64:LE:64:v8A".to_string(), "default".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("AARCH64.cspec"), "AARCH64 default cspec");

        let got = index
            .get(&("AARCH64:LE:64:v8A".to_string(), "windows".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert_eq!(got, Some("AARCH64_win.cspec"), "AARCH64 windows cspec");

        // ARM 32-bit
        let got = index
            .get(&("ARM:LE:32:v7".to_string(), "default".to_string()))
            .map(|e| e.cspec_filename.as_str());
        assert!(
            got.map_or(false, |f| f.ends_with(".cspec")),
            "ARM:LE:32:v7 default should map to some cspec, got: {got:?}"
        );
    }

    /// Index must not be empty when the real sleigh-specs tree is present.
    #[test]
    fn real_ldefs_index_is_non_empty() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let languages_root = repo_root.join("utils/sleigh-specs/languages");
        if !languages_root.exists() {
            return;
        }
        let index = build_ldefs_index(&languages_root);
        assert!(
            index.len() > 50,
            "expected > 50 (language_id, compiler_spec_id) pairs, got {}",
            index.len()
        );
    }

    #[test]
    fn extract_attr_rejects_substring_keys() {
        let segment = r#"language processorspec="x86.pspec" external_id="ext123""#;
        // "spec" must not match inside "processorspec"
        assert_eq!(extract_attr(segment, "spec"), None);
        // "id" must not match inside "external_id"
        assert_eq!(extract_attr(segment, "id"), None);
        // Full keys must match
        assert_eq!(extract_attr(segment, "processorspec"), Some("x86.pspec"));
        assert_eq!(extract_attr(segment, "external_id"), Some("ext123"));
    }

    #[test]
    fn extract_attr_supports_single_quotes_and_whitespace() {
        let segment =
            r#"compiler id = 'windows' spec= "x86win.cspec" name ='Default Compiler' opt = 'val'"#;
        assert_eq!(extract_attr(segment, "id"), Some("windows"));
        assert_eq!(extract_attr(segment, "spec"), Some("x86win.cspec"));
        assert_eq!(extract_attr(segment, "name"), Some("Default Compiler"));
        assert_eq!(extract_attr(segment, "opt"), Some("val"));
    }

    #[test]
    fn find_tag_close_ignores_greater_than_inside_quotes() {
        let tag_inner = r#"compiler name="condition > 5" spec="test.cspec" id="test"/>rest"#;
        let close = find_tag_close(tag_inner);
        assert_eq!(
            &tag_inner[..close],
            r#"compiler name="condition > 5" spec="test.cspec" id="test"/"#
        );
    }

    #[test]
    fn parse_ldefs_handles_self_closing_language_tags() {
        let xml = r#"<language_definitions>
  <language id="ARM:LE:32:v8" processorspec="ARM.pspec" spec="ARM.cspec"/>
  <language id="ARM:LE:64:v8A" processorspec="AARCH64.pspec"/>
</language_definitions>"#;

        use std::path::PathBuf;
        let dir = PathBuf::from("/tmp/arm");
        let mut index = HashMap::default();
        parse_ldefs_into_index(xml, &dir, &mut index);

        let entry32 = index.get(&("ARM:LE:32:v8".to_string(), "default".to_string()));
        assert!(entry32.is_some());
        assert_eq!(entry32.unwrap().cspec_filename, "ARM.cspec");
        assert_eq!(
            entry32.unwrap().pspec_filename.as_deref(),
            Some("ARM.pspec")
        );

        let entry64 = index.get(&("ARM:LE:64:v8A".to_string(), "default".to_string()));
        assert!(entry64.is_some());
        assert_eq!(entry64.unwrap().cspec_filename, "AARCH64.cspec");
        assert_eq!(
            entry64.unwrap().pspec_filename.as_deref(),
            Some("AARCH64.pspec")
        );
    }

    #[test]
    fn parse_ldefs_handles_unclosed_language_tags() {
        let xml = r#"<language_definitions>
  <language id="x86:LE:32:default">
    <compiler id="windows" spec="x86win.cspec"/>
    <compiler id="gcc"     spec="x86gcc.cspec"/>
  <language id="x86:LE:64:default">
    <compiler id="windows" spec="x86-64-win.cspec"/>
</language_definitions>"#;

        use std::path::PathBuf;
        let dir = PathBuf::from("/tmp/x86");
        let mut index = HashMap::default();
        parse_ldefs_into_index(xml, &dir, &mut index);

        assert_eq!(
            index
                .get(&("x86:LE:32:default".to_string(), "windows".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86win.cspec")
        );
        assert_eq!(
            index
                .get(&("x86:LE:32:default".to_string(), "gcc".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86gcc.cspec")
        );
        assert_eq!(
            index
                .get(&("x86:LE:64:default".to_string(), "windows".to_string()))
                .map(|e| e.cspec_filename.as_str()),
            Some("x86-64-win.cspec")
        );
    }
}
