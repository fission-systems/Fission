//! Lazy, cached `.cspec` document loader.
//!
//! Ghidra resolves a compiler spec via `(language_id, compiler_spec_id)` — e.g.,
//! `("x86:LE:64:default", "gcc")`. This module maps that pair to a `.cspec` file path
//! in the Fission `utils/sleigh-specs/languages/` tree and caches the result.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{CspecDocument, SlaRegisterMap};

/// Cache key: `(processor_dir, cspec_filename_stem)`.
/// E.g. `("x86", "x86-64-gcc")`.
type CacheKey = (String, String);
type CspecCache = HashMap<CacheKey, Option<CspecDocument>>;

static CSPEC_CACHE: OnceLock<std::sync::Mutex<CspecCache>> = OnceLock::new();

fn global_cache() -> &'static std::sync::Mutex<CspecCache> {
    CSPEC_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Load and cache a `.cspec` document.
///
/// - `language_dir`: absolute path to the language directory (e.g. `.../languages/x86/`).
/// - `cspec_stem`: the stem of the `.cspec` filename (e.g. `"x86-64-gcc"`).
pub fn load_cspec(language_dir: &Path, cspec_stem: &str) -> Option<CspecDocument> {
    let dir_key = language_dir.to_string_lossy().to_string();
    let key = (dir_key, cspec_stem.to_string());

    {
        let guard = global_cache().lock().ok()?;
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }

    let mut path = language_dir.to_path_buf();
    path.push(format!("{cspec_stem}.cspec"));
    let doc = CspecDocument::parse_file(&path);

    {
        if let Ok(mut guard) = global_cache().lock() {
            guard.entry(key).or_insert_with(|| doc.clone());
        }
    }

    doc
}

/// Convenience: load the default `.cspec` for a processor directory and resolve
/// register names using the provided SLA register map.
///
/// `preferred_stems` is checked in order; the first one that exists on disk wins.
pub fn load_default_cspec_resolved(
    language_dir: &Path,
    preferred_stems: &[&str],
    reg_map: &SlaRegisterMap,
) -> Option<super::ResolvedCspec> {
    for &stem in preferred_stems {
        if let Some(doc) = load_cspec(language_dir, stem) {
            return Some(doc.resolve(reg_map));
        }
    }
    None
}

/// Attempt to find the processor language directory from the sleigh-specs tree.
///
/// `sleigh_specs_root`: root of the `utils/sleigh-specs/languages/` tree.
/// `processor_subdir`: e.g. `"x86"`, `"ARM"`, `"MIPS"`.
pub fn find_language_dir(sleigh_specs_root: &Path, processor_subdir: &str) -> Option<PathBuf> {
    let dir = sleigh_specs_root.join(processor_subdir);
    dir.is_dir().then_some(dir)
}
