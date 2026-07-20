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
            }
            continue;
        }
        let tag_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let tag = &rest[..tag_end];
        if tag == "language" {
            let close = rest.find('>').unwrap_or(rest.len());
            let segment = &rest[..close];
            if let (Some(language_id), Some(slafile)) = (
                extract_attr(segment, "id"),
                extract_attr(segment, "slafile"),
            ) {
                let slaspec = slafile_to_slaspec_path(dir, slafile);
                index.insert(language_id.to_string(), slaspec);
            }
            rest = &rest[close.min(rest.len())..];
        } else if let Some(close) = rest.find('>') {
            rest = &rest[close + 1..];
        } else {
            break;
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

    let mut rest = contents;
    loop {
        // Find the next '<'
        let Some(lt) = rest.find('<') else { break };
        rest = &rest[lt + 1..];

        // Skip comments
        if rest.starts_with("!--") {
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
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
                // Extract `id="..."` and `processorspec="..."` attributes.
                let close = rest.find('>').unwrap_or(rest.len());
                let segment = &rest[..close];
                current_language_id = extract_attr(segment, "id").map(str::to_string);
                current_pspec_filename = extract_attr(segment, "processorspec").map(str::to_string);
                current_dwarf_filename = None;
                pending_compilers.clear();
                rest = &rest[close.min(rest.len())..];
            }
            "/language" => {
                if let Some(lang_id) = current_language_id.take() {
                    for (compiler_id, cspec_filename) in pending_compilers.drain(..) {
                        index.insert(
                            (lang_id.clone(), compiler_id),
                            LdefsEntry {
                                language_dir: dir.to_path_buf(),
                                cspec_filename,
                                pspec_filename: current_pspec_filename.clone(),
                                dwarf_mapping_filename: current_dwarf_filename.clone(),
                            },
                        );
                    }
                }
                current_pspec_filename = None;
                current_dwarf_filename = None;
                pending_compilers.clear();
                rest = &rest[tag_end.min(rest.len())..];
            }
            "compiler" => {
                if current_language_id.is_some() {
                    let close = rest.find('>').unwrap_or(rest.len());
                    let segment = &rest[..close];
                    if let (Some(compiler_id), Some(spec)) =
                        (extract_attr(segment, "id"), extract_attr(segment, "spec"))
                    {
                        pending_compilers.push((compiler_id.to_string(), spec.to_string()));
                    }
                    rest = &rest[close.min(rest.len())..];
                }
            }
            "external_name" => {
                if current_language_id.is_some() {
                    let close = rest.find('>').unwrap_or(rest.len());
                    let segment = &rest[..close];
                    if extract_attr(segment, "tool") == Some("DWARF.register.mapping.file") {
                        current_dwarf_filename = extract_attr(segment, "name").map(str::to_string);
                    }
                    rest = &rest[close.min(rest.len())..];
                }
            }
            _ => {
                // Skip to '>'
                if let Some(close) = rest.find('>') {
                    rest = &rest[close + 1..];
                } else {
                    break;
                }
            }
        }
    }
}

/// Extract `key="value"` from an XML attribute string.
fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=\"");
    let start = segment.find(needle.as_str())? + needle.len();
    let end = segment[start..].find('"')? + start;
    Some(&segment[start..end])
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
}
