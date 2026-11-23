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

use crate::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

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

// ── Global per-root cache ────────────────────────────────────────────────────

type LdefsIndexCache = HashMap<PathBuf, Arc<LdefsIndex>>;
type LanguageSlaspecCache = HashMap<PathBuf, Arc<LanguageSlaspecIndex>>;

static LDEFS_INDEX_CACHE: OnceLock<RwLock<LdefsIndexCache>> = OnceLock::new();
static LANGUAGE_SLASPEC_CACHE: OnceLock<RwLock<LanguageSlaspecCache>> = OnceLock::new();

/// `language_id` → absolute path to the checked-in `.slaspec` entry file.
pub type LanguageSlaspecIndex = HashMap<String, PathBuf>;

/// Return (or build and cache) the global `language_id` → `.slaspec` index for `languages_root`.
///
/// Keyed by canonical `languages_root` path; non-existent directories and empty scans
/// are not cached so transient path mistakes never poison later calls.
pub fn global_language_slaspec_index(languages_root: &Path) -> Arc<LanguageSlaspecIndex> {
    let Ok(canonical) = std::fs::canonicalize(languages_root) else {
        return Arc::new(HashMap::default());
    };
    if !canonical.is_dir() {
        return Arc::new(HashMap::default());
    }
    let cache = LANGUAGE_SLASPEC_CACHE.get_or_init(|| RwLock::new(HashMap::default()));
    if let Ok(guard) = cache.read()
        && let Some(hit) = guard.get(&canonical)
    {
        return Arc::clone(hit);
    }
    let built = build_language_slaspec_index(&canonical);
    if built.is_empty() {
        return Arc::new(built);
    }
    let shared = Arc::new(built);
    if let Ok(mut guard) = cache.write() {
        let entry = guard
            .entry(canonical)
            .or_insert_with(|| Arc::clone(&shared));
        return Arc::clone(entry);
    }
    shared
}

/// Return (or build and cache) the global `.ldefs` index for `languages_root`.
///
/// Keyed by canonical `languages_root` path; non-existent directories and empty scans
/// are not cached so subsequent calls with a valid `languages_root` succeed cleanly.
pub fn global_ldefs_index(languages_root: &Path) -> Arc<LdefsIndex> {
    let Ok(canonical) = std::fs::canonicalize(languages_root) else {
        return Arc::new(HashMap::default());
    };
    if !canonical.is_dir() {
        return Arc::new(HashMap::default());
    }
    let cache = LDEFS_INDEX_CACHE.get_or_init(|| RwLock::new(HashMap::default()));
    if let Ok(guard) = cache.read()
        && let Some(hit) = guard.get(&canonical)
    {
        return Arc::clone(hit);
    }
    let built = build_ldefs_index(&canonical);
    if built.is_empty() {
        return Arc::new(built);
    }
    let shared = Arc::new(built);
    if let Ok(mut guard) = cache.write() {
        let entry = guard
            .entry(canonical)
            .or_insert_with(|| Arc::clone(&shared));
        return Arc::clone(entry);
    }
    shared
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

fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    crate::xml_scan::extract_attr(segment, key)
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

        assert!(
            languages_root.exists(),
            "{} is committed under utils/; a missing copy is a broken checkout",
            languages_root.display()
        );

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
        assert!(
            languages_root.exists(),
            "{} is committed under utils/; a missing copy is a broken checkout",
            languages_root.display()
        );
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

    #[test]
    fn failed_or_nonexistent_root_does_not_poison_global_ldefs_and_slaspec_caches() {
        let missing = Path::new("/nonexistent/fission-cspec-ldefs-missing-root");
        assert!(global_ldefs_index(missing).is_empty());
        assert!(global_language_slaspec_index(missing).is_empty());

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let valid = repo_root.join("utils/sleigh-specs/languages");
        if valid.is_dir() {
            assert!(
                !global_ldefs_index(&valid).is_empty(),
                "earlier missing root call must not cache an empty ldefs index forever"
            );
            assert!(
                !global_language_slaspec_index(&valid).is_empty(),
                "earlier missing root call must not cache an empty slaspec index forever"
            );
        }

        // Distinct custom roots stay isolated.
        let tmp = std::env::temp_dir().join(format!(
            "fission-cspec-ldefs-cache-test-{}",
            std::process::id()
        ));
        let lang_dir = tmp.join("Toy");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&lang_dir).unwrap();
        std::fs::write(
            lang_dir.join("toy.ldefs"),
            r#"<language_definitions><language id="Toy:LE:32:test" processorspec="toy.pspec" slafile="toy.sla" spec="toy.cspec"/></language_definitions>"#,
        )
        .unwrap();
        std::fs::write(lang_dir.join("toy.slaspec"), "define endian=little;\n").unwrap();

        let custom_ldefs = global_ldefs_index(&tmp);
        let custom_sla = global_language_slaspec_index(&tmp);
        assert!(
            custom_ldefs
                .get(&("Toy:LE:32:test".to_string(), "default".to_string()))
                .is_some()
        );
        assert!(custom_sla.contains_key("Toy:LE:32:test"));
        if valid.is_dir() {
            assert!(
                global_ldefs_index(&valid)
                    .get(&("Toy:LE:32:test".to_string(), "default".to_string()))
                    .is_none()
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
