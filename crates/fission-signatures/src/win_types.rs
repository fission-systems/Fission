//! Windows Data Types and Structures
//!
//! Common Windows API structures for type annotation in decompiled code.
//! Based on Windows SDK headers and ghidra-data community definitions.
//! Canonical JSON lives under the resolved signatures corpus (`ResourceProvider` / workspace layout).
//! (`base_types.json`, `structures.json`), loaded via [`fission_core::resources::ResourceProvider`].

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use fission_core::resources::ResourceProvider;
use serde::Deserialize;

fn resources() -> ResourceProvider {
    ResourceProvider::global()
}

#[derive(Debug, Clone)]
pub struct WinTypesError {
    message: String,
}

impl WinTypesError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for WinTypesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for WinTypesError {}

fn try_win32_typeinfo_json_path(filename: &str) -> Result<PathBuf, WinTypesError> {
    resources()
        .win32_typeinfo_json_path(filename)
        .ok_or_else(|| {
            WinTypesError::new(format!(
                "fission-signatures win_types: missing typeinfo JSON `{filename}` \
                 (configure bundle/workspace signatures or PATHS.gdt_dir)"
            ))
        })
}

fn try_read_typeinfo_json(filename: &str) -> Result<String, WinTypesError> {
    let path = try_win32_typeinfo_json_path(filename)?;
    if !path.exists() {
        return Err(WinTypesError::new(format!(
            "fission-signatures win_types: missing canonical data file {} (under resolved Win32 typeinfo dir)",
            path.display()
        )));
    }
    fs::read_to_string(&path).map_err(|e| {
        WinTypesError::new(format!(
            "fission-signatures win_types: failed to read {}: {e}",
            path.display(),
        ))
    })
}

/// Rust struct layouts (`typeinfo/rust/rust_structures.json`) are an optional
/// supplement to the Windows corpus, not every environment ships them, so
/// unlike [`try_read_typeinfo_json`] a missing file is not an error here.
fn try_read_rust_typeinfo_json(filename: &str) -> Option<String> {
    let path = resources().rust_typeinfo_json_path(filename)?;
    fs::read_to_string(&path).ok()
}

/// Same optional-supplement contract as [`try_read_rust_typeinfo_json`], for
/// the generic (cross-platform C library) typeinfo corpus.
fn try_read_generic_typeinfo_json(filename: &str) -> Option<String> {
    let path = resources().generic_typeinfo_json_path(filename)?;
    fs::read_to_string(&path).ok()
}

/// Same optional-supplement contract as [`try_read_rust_typeinfo_json`], for
/// the mac_10.9 typeinfo corpus.
fn try_read_mac_typeinfo_json(filename: &str) -> Option<String> {
    let path = resources().mac_typeinfo_json_path(filename)?;
    fs::read_to_string(&path).ok()
}

// ============================================================================
// Windows Base Types (for annotation purposes)
// ============================================================================

/// Windows base type sizes
pub mod base_types {
    use serde::Deserialize;

    use super::{WinTypesError, try_read_typeinfo_json};

    /// Type size information for annotation
    #[derive(Debug, Clone)]
    pub struct TypeInfo {
        pub name: String,
        pub size_32: usize,
        pub size_64: usize,
        pub is_pointer: bool,
        pub is_signed: bool,
    }

    #[derive(Deserialize)]
    struct JsonTypeInfo {
        name: String,
        size_32: usize,
        size_64: usize,
        is_pointer: bool,
        is_signed: bool,
    }

    /// Load all base types from `base_types.json` in the Win32 typeinfo corpus.
    pub fn all() -> Vec<TypeInfo> {
        try_all().unwrap_or_else(|e| panic!("{e}"))
    }

    /// Fallible variant of [`all`] for UI and batch workers that must degrade gracefully.
    pub fn try_all() -> Result<Vec<TypeInfo>, WinTypesError> {
        let json_str = try_read_typeinfo_json("base_types.json")?;
        let items: Vec<JsonTypeInfo> = serde_json::from_str(&json_str).map_err(|e| {
            let path = super::try_win32_typeinfo_json_path("base_types.json")
                .map(|p| p.display().to_string())
                .unwrap_or_else(|path_err| path_err.to_string());
            WinTypesError::new(format!("Failed to parse base_types.json at {path}: {e}"))
        })?;
        Ok(items
            .into_iter()
            .map(|j| TypeInfo {
                name: j.name,
                size_32: j.size_32,
                size_64: j.size_64,
                is_pointer: j.is_pointer,
                is_signed: j.is_signed,
            })
            .collect())
    }
}

// ============================================================================
// Windows Structure Definitions
// ============================================================================

/// Structure field definition
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_name: String,
    pub offset_32: usize,
    pub offset_64: usize,
    pub size_32: usize,
    pub size_64: usize,
}

/// Structure definition
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub size_32: usize,
    pub size_64: usize,
    pub fields: Vec<FieldDef>,
}

// JSON deserialization types
#[derive(Deserialize)]
struct JsonFieldDef {
    name: String,
    type_name: String,
    offset_32: usize,
    offset_64: usize,
    size_32: usize,
    size_64: usize,
}

#[derive(Deserialize)]
struct JsonStructDef {
    name: String,
    size_32: usize,
    size_64: usize,
    fields: Vec<JsonFieldDef>,
}

/// Parse a `structures.json`-shaped JSON document and merge its entries into
/// `structures`. When `overwrite` is false, an entry whose name already
/// exists is left untouched (additive-only merge) rather than replaced.
fn merge_json_struct_defs(
    structures: &mut HashMap<String, StructDef>,
    source_name: &str,
    json_str: &str,
    overwrite: bool,
) {
    let items: Vec<JsonStructDef> = match serde_json::from_str(json_str) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("fission-signatures win_types: failed to parse {source_name}, skipping: {e}");
            return;
        }
    };
    for item in items {
        if !overwrite && structures.contains_key(&item.name) {
            continue;
        }
        let fields = item
            .fields
            .into_iter()
            .map(|f| FieldDef {
                name: f.name,
                type_name: f.type_name,
                offset_32: f.offset_32,
                offset_64: f.offset_64,
                size_32: f.size_32,
                size_64: f.size_64,
            })
            .collect();
        structures.insert(
            item.name.clone(),
            StructDef {
                name: item.name,
                size_32: item.size_32,
                size_64: item.size_64,
                fields,
            },
        );
    }
}

/// Windows structures database
///
/// `structures` is `Arc`-wrapped so [`WindowsStructures::try_new`] can hand
/// out cheap clones of a process-wide cache instead of re-reading and
/// re-parsing the multi-megabyte `structures.json` corpus on every call —
/// this constructor runs once per decompiled function on some call paths
/// (normalize pointer-arithmetic recovery, NIR type-context assembly), so an
/// uncached load dominates per-function wall time on binaries with many
/// functions.
#[derive(Clone)]
pub struct WindowsStructures {
    pub structures: std::sync::Arc<HashMap<String, StructDef>>,
}

static STRUCTURES_CACHE: std::sync::OnceLock<Result<WindowsStructures, WinTypesError>> =
    std::sync::OnceLock::new();

impl WindowsStructures {
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|e| panic!("{e}"))
    }

    /// Load the Windows structure corpus without panicking. Cached process-wide
    /// after the first successful (or failed) load — cloning the cached result
    /// only bumps an `Arc` refcount, it does not re-read or re-parse the JSON.
    pub fn try_new() -> Result<Self, WinTypesError> {
        STRUCTURES_CACHE.get_or_init(Self::load).clone()
    }

    fn load() -> Result<Self, WinTypesError> {
        let json_str = try_read_typeinfo_json("structures.json")?;
        let items: Vec<JsonStructDef> = serde_json::from_str(&json_str).map_err(|e| {
            let path = try_win32_typeinfo_json_path("structures.json")
                .map(|p| p.display().to_string())
                .unwrap_or_else(|path_err| path_err.to_string());
            WinTypesError::new(format!("Failed to parse structures.json at {path}: {e}"))
        })?;

        let mut structures = HashMap::with_capacity(items.len());
        for item in items {
            let fields = item
                .fields
                .into_iter()
                .map(|f| FieldDef {
                    name: f.name,
                    type_name: f.type_name,
                    offset_32: f.offset_32,
                    offset_64: f.offset_64,
                    size_32: f.size_32,
                    size_64: f.size_64,
                })
                .collect();

            let def = StructDef {
                name: item.name.clone(),
                size_32: item.size_32,
                size_64: item.size_64,
                fields,
            };
            structures.insert(item.name, def);
        }

        if let Some(rust_json) = try_read_rust_typeinfo_json("rust_structures.json") {
            merge_json_struct_defs(&mut structures, "rust_structures.json", &rust_json, true);
        }

        // phnt_structures.json is computed (not looked up) from System
        // Informer's phnt C headers by scripts/phnt_extract_structs.py,
        // which independently applies MSVC's struct-layout algorithm to
        // undocumented NT structs the win32 corpus above doesn't have.
        // Merged additive-only: a handful of names that DO already exist in
        // structures.json disagree with the phnt-computed size on cross-
        // validation (some confirmed to be stale/placeholder entries in the
        // older corpus, e.g. PROCESS_MITIGATION_SEHOP_POLICY, but not all
        // audited yet) -- so on any name collision the existing entry wins
        // rather than risking silently replacing a correct value with a
        // wrong one from an unaudited case.
        if let Ok(phnt_json) = try_read_typeinfo_json("phnt_structures.json") {
            merge_json_struct_defs(&mut structures, "phnt_structures.json", &phnt_json, false);
        }

        // windows_vs12_structures.json / generic_clib_structures.json /
        // mac_osx_structures.json are computed from Ghidra's OWN bundled
        // .gdt archives (Ghidra/Features/Base/data/typeinfo/{win32,generic,
        // mac_10.9}/) via scripts/gdt_extract_structs.py +
        // scripts/merge_gdt_struct_widths.py -- struct/composite data this
        // corpus never mined despite shipping right next to it. Confirmed
        // >94% net-new against the existing win32 corpus (e.g. 5,187 of
        // windows_vs12's 5,500 structs have no prior entry at all) with the
        // same cross-validation approach used for phnt (merged additive-
        // only for the same reason: ~2-3% of overlapping names disagree on
        // size and not all are individually audited).
        if let Ok(vs12_json) = try_read_typeinfo_json("windows_vs12_structures.json") {
            merge_json_struct_defs(&mut structures, "windows_vs12_structures.json", &vs12_json, false);
        }
        if let Some(generic_json) = try_read_generic_typeinfo_json("generic_clib_structures.json") {
            merge_json_struct_defs(&mut structures, "generic_clib_structures.json", &generic_json, false);
        }
        if let Some(mac_json) = try_read_mac_typeinfo_json("mac_osx_structures.json") {
            // mac_osx.gdt has no 64-bit sibling archive (a legacy Mac OS X
            // 10.9-era 32-bit-only source) -- these entries carry size_64=0
            // (see scripts/gdt_extract_structs.py's caller for this file),
            // which candidate_struct_name/infer_struct_name_from_offsets
            // already treat as "never matches" rather than a wrong guess.
            merge_json_struct_defs(&mut structures, "mac_osx_structures.json", &mac_json, false);
        }

        Ok(Self {
            structures: std::sync::Arc::new(structures),
        })
    }

    /// Get structure by name
    pub fn get(&self, name: &str) -> Option<&StructDef> {
        self.structures.get(name)
    }

    /// Get all structure names
    pub fn names(&self) -> Vec<&String> {
        self.structures.keys().collect()
    }
}

impl Default for WindowsStructures {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn loads_phnt_structures_additively() {
        let ws = super::WindowsStructures::new();
        // net-new undocumented NT structs, only available via phnt_structures.json
        let fbi = ws
            .get("FILE_BASIC_INFORMATION")
            .expect("FILE_BASIC_INFORMATION from phnt corpus");
        assert_eq!(fbi.size_64, 40);
        let oa = ws
            .get("OBJECT_ATTRIBUTES")
            .expect("OBJECT_ATTRIBUTES from phnt corpus");
        assert_eq!(oa.size_64, 48);
    }

    #[test]
    fn loads_gdt_mined_structures_additively() {
        let ws = super::WindowsStructures::new();
        // generic_clib_structures.json: dual-width merge of Ghidra's own
        // generic_clib.gdt + generic_clib_64.gdt, cross-checked against
        // real glibc struct layouts before shipping. `stat`/`tm` also exist
        // (correctly, additively) but collide with windows_vs12's own CRT
        // `stat`/`tm` definitions, which load first and win -- timespec/
        // div_t don't collide with anything in the win32-family sources.
        let timespec = ws.get("timespec").expect("timespec from generic_clib corpus");
        assert_eq!(timespec.size_32, 8);
        assert_eq!(timespec.size_64, 16);
        let div_t = ws.get("div_t").expect("div_t from generic_clib corpus");
        assert_eq!(div_t.size_32, 8);
        assert_eq!(div_t.size_64, 8);
        // windows_vs12_structures.json: net-new (not in the existing win32
        // corpus) struct pulled from Ghidra's own windows_vs12_{32,64}.gdt.
        assert!(ws.get("PPM_WMI_PERF_STATE").is_some());
        // scripts/gdt_extract_structs.py also emits typedef aliases (not
        // just the Composite tag name) -- ACTCTX/tagACTCTXA/tagACTCTXW are
        // all net-new (not in the pre-existing win32 corpus) and must
        // resolve to the same layout regardless of which name is used.
        let by_alias = ws.get("ACTCTX").expect("ACTCTX typedef alias");
        let by_tag = ws.get("tagACTCTXA").expect("tagACTCTXA composite tag name");
        assert_eq!(by_alias.size_64, by_tag.size_64);
        assert_eq!(by_alias.fields.len(), by_tag.fields.len());
    }

    #[test]
    fn loads_utils_win_types_json() {
        let ws = super::WindowsStructures::new();
        assert!(
            ws.get("UNICODE_STRING").is_some(),
            "expected UNICODE_STRING from structures.json (Win32 typeinfo corpus)"
        );
        let base = super::base_types::all();
        assert!(
            !base.is_empty(),
            "expected base_types from base_types.json (Win32 typeinfo corpus)"
        );
    }
}
