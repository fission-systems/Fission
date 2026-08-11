//! DLL ordinal -> export-name resolution.
//!
//! PE imports made by ordinal (no name, just a numeric index into the
//! exporting DLL's export table) carry no readable symbol on their own.
//! This loads a corpus of known DLL export tables (ordinal -> name), keyed
//! by lowercased DLL filename with extension, sourced from RetDec's vendored
//! ordinal tables via `scripts/retdec_ordinals_extract.py`.

use std::collections::HashMap;
use std::fmt;
use std::fs;

use fission_core::resources::ResourceProvider;

#[derive(Debug, Clone)]
pub struct OrdinalError {
    message: String,
}

impl fmt::Display for OrdinalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OrdinalError {}

/// `dll_name.dll` (lowercased) -> ordinal -> exported name.
///
/// `Arc`-wrapped and cached process-wide (like [`crate::win_types::WindowsStructures`])
/// since PE import parsing can look this up thousands of times per binary.
#[derive(Clone, Default)]
pub struct OrdinalDatabase {
    tables: std::sync::Arc<HashMap<String, HashMap<u32, String>>>,
}

static ORDINAL_CACHE: std::sync::OnceLock<Result<OrdinalDatabase, OrdinalError>> =
    std::sync::OnceLock::new();

impl OrdinalDatabase {
    pub fn try_new() -> Result<Self, OrdinalError> {
        ORDINAL_CACHE.get_or_init(Self::load).clone()
    }

    fn load() -> Result<Self, OrdinalError> {
        let mut tables: HashMap<String, HashMap<u32, String>> = HashMap::new();
        let resources = ResourceProvider::global();
        for filename in ["x86_ordinals.json", "arm_ordinals.json"] {
            let Some(path) = resources.ordinals_json_path(filename) else {
                continue;
            };
            let content = fs::read_to_string(&path).map_err(|e| OrdinalError {
                message: format!(
                    "fission-signatures ordinals: failed to read {}: {e}",
                    path.display()
                ),
            })?;
            let raw: HashMap<String, HashMap<String, String>> = serde_json::from_str(&content)
                .map_err(|e| OrdinalError {
                    message: format!(
                        "fission-signatures ordinals: failed to parse {}: {e}",
                        path.display()
                    ),
                })?;
            for (dll, entries) in raw {
                let by_ordinal = tables.entry(dll).or_default();
                for (ord_str, name) in entries {
                    if let Ok(ord) = ord_str.parse::<u32>() {
                        by_ordinal.entry(ord).or_insert(name);
                    }
                }
            }
        }
        Ok(Self {
            tables: std::sync::Arc::new(tables),
        })
    }

    /// Resolve `dll_name` (any case, `.dll` suffix optional) + ordinal to an
    /// export name, if known.
    pub fn resolve(&self, dll_name: &str, ordinal: u32) -> Option<&str> {
        let normalized = normalize_dll_name(dll_name);
        self.tables
            .get(&normalized)
            .and_then(|by_ordinal| by_ordinal.get(&ordinal))
            .map(String::as_str)
    }
}

fn normalize_dll_name(dll_name: &str) -> String {
    let lower = dll_name.trim().to_ascii_lowercase();
    if lower.ends_with(".dll") {
        lower
    } else {
        format!("{lower}.dll")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_ordinals() {
        let db = OrdinalDatabase::try_new().expect("load ordinal corpus");
        assert_eq!(db.resolve("ws2_32.dll", 1), Some("accept"));
        assert_eq!(db.resolve("WS2_32", 1), Some("accept"), "case/suffix-insensitive lookup");
        assert_eq!(db.resolve("mssign32.dll", 1), Some("DllUnregisterServer"));
        assert_eq!(db.resolve("not_a_real_dll_xyz.dll", 1), None);
    }
}
