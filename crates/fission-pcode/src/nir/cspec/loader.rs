//! Lazy, cached `.cspec` document loader — Ghidra-style resolution.
//!
//! ## Resolution pipeline
//!
//! 1. **Exact** — query the global `.ldefs` index for `(language_id, compiler_spec_id)`
//!    to get the precise `.cspec` filename.  This covers every architecture and compiler
//!    variant that Ghidra ships, including AARCH64, ARM, MIPS, PowerPC, RISC-V, Sparc, …
//!
//! 2. **Fallback** — if the `.ldefs` index is unavailable (e.g., `utils/sleigh-specs`
//!    tree not populated), fall back to the legacy stem-heuristic so that the caller
//!    never hard-crashes.
//!
//! Both paths feed into the same `CspecDocument` cache, so a given `.cspec` file is
//! parsed and cached at most once per process lifetime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{ldefs::global_ldefs_index, CspecDocument, ResolvedCspec, SlaRegisterMap};

// ── Per-file document cache ───────────────────────────────────────────────────

/// Cache key: absolute path of the `.cspec` file.
type CspecCache = HashMap<PathBuf, Option<CspecDocument>>;

static CSPEC_CACHE: OnceLock<std::sync::Mutex<CspecCache>> = OnceLock::new();

fn global_cache() -> &'static std::sync::Mutex<CspecCache> {
    CSPEC_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Load and cache a `.cspec` document by absolute path.
pub fn load_cspec_path(path: &Path) -> Option<CspecDocument> {
    let key = path.to_path_buf();

    {
        let guard = global_cache().lock().ok()?;
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }

    let doc = CspecDocument::parse_file(path);

    if let Ok(mut guard) = global_cache().lock() {
        guard.entry(key).or_insert_with(|| doc.clone());
    }

    doc
}

// ── Ghidra-style .ldefs-based exact resolution ────────────────────────────────

/// Resolve the `.cspec` path for a `(language_id, compiler_spec_id)` pair using
/// the Ghidra-style `.ldefs` index.
///
/// Returns `None` if the pair is not found in any `.ldefs` file under `languages_root`.
///
/// # Example
/// ```text
/// language_id      = "x86:LE:64:default"
/// compiler_spec_id = "gcc"
/// → <languages_root>/x86/x86-64-gcc.cspec
/// ```
pub fn cspec_path_for_pair(
    languages_root: &Path,
    language_id: &str,
    compiler_spec_id: &str,
) -> Option<PathBuf> {
    let index = global_ldefs_index(languages_root);
    let key = (language_id.to_string(), compiler_spec_id.to_string());
    index.get(&key).map(|entry| entry.cspec_path())
}

/// Resolve and load a `.cspec` document for the given `(language_id, compiler_spec_id)`
/// pair, returning the fully resolved prototype.
///
/// This is the **primary** entry point for all cspec loading.  It uses the `.ldefs`
/// index for exact file-name resolution, then passes the result through the SLA
/// register map to produce `(offset, size)` tuples.
pub fn load_cspec_for_pair(
    languages_root: &Path,
    language_id: &str,
    compiler_spec_id: &str,
    reg_map: &SlaRegisterMap,
) -> Option<ResolvedCspec> {
    let path = cspec_path_for_pair(languages_root, language_id, compiler_spec_id)?;
    let doc = load_cspec_path(&path)?;
    Some(doc.resolve(reg_map))
}

// ── Legacy stem-based helpers (kept for backwards-compat / unit tests) ────────

/// Load and cache a `.cspec` document by directory + stem.
///
/// Kept for test helpers and the legacy fallback path.  New callers should prefer
/// `load_cspec_for_pair`.
pub fn load_cspec(language_dir: &Path, cspec_stem: &str) -> Option<CspecDocument> {
    let mut path = language_dir.to_path_buf();
    path.push(format!("{cspec_stem}.cspec"));
    load_cspec_path(&path)
}

/// Load and resolve a `.cspec` using a preferred-stem list.
///
/// Kept for unit tests.  New callers should prefer `load_cspec_for_pair`.
pub fn load_default_cspec_resolved(
    language_dir: &Path,
    preferred_stems: &[&str],
    reg_map: &SlaRegisterMap,
) -> Option<ResolvedCspec> {
    for &stem in preferred_stems {
        if let Some(doc) = load_cspec(language_dir, stem) {
            return Some(doc.resolve(reg_map));
        }
    }
    None
}

/// Attempt to find the processor language directory from the sleigh-specs tree.
///
/// Kept for tests and legacy callers.  New code resolves via `.ldefs`.
pub fn find_language_dir(sleigh_specs_root: &Path, processor_subdir: &str) -> Option<PathBuf> {
    // Case-sensitive first
    let dir = sleigh_specs_root.join(processor_subdir);
    if dir.is_dir() {
        return Some(dir);
    }
    // Case-insensitive fallback (e.g. "aarch64" vs "AARCH64")
    if let Ok(entries) = std::fs::read_dir(sleigh_specs_root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(processor_subdir))
                {
                    return Some(p);
                }
            }
        }
    }
    None
}
