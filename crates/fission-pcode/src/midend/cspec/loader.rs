//! Lazy, cached `.cspec` document loader — Ghidra-style resolution.
//!
//! ## Resolution pipeline
//!
//! 1. **Exact** — query the global `.ldefs` index for `(language_id, compiler_spec_id)`
//!    to get the precise `.cspec` filename.
//! parsed and cached at most once per process lifetime.

use crate::midend::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{CspecDocument, ResolvedCspec, SlaRegisterMap, ldefs::global_ldefs_index};

// ── Per-file document cache ───────────────────────────────────────────────────

/// Cache key: absolute path of the `.cspec` file.
type CspecCache = HashMap<PathBuf, Option<CspecDocument>>;

static CSPEC_CACHE: OnceLock<std::sync::Mutex<CspecCache>> = OnceLock::new();

fn global_cache() -> &'static std::sync::Mutex<CspecCache> {
    CSPEC_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::default()))
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
