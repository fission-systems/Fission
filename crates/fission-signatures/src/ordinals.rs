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
    /// Packed table, consulted per DLL instead of parsing 21.5M of JSON.
    packed: Option<std::sync::Arc<crate::fpk::FpkReader>>,
    /// Ghidra's own export tables, consulted only where the primary has no
    /// answer.
    ///
    /// They carry 30,929 (dll, ordinal) pairs the RetDec tables lack, including
    /// six DLLs missing entirely -- mfc140, mfc140u, and the 16-bit kernel.exe,
    /// krnl386.exe, gdi.exe, user.exe. They also disagree with the primary on
    /// 91,555 pairs, almost all in MFC, where ordinals are reassigned between
    /// builds:
    ///
    /// ```text
    /// mfc80u.dll #6323   ghidra: ?wndTopMost@CWnd@@2V1@B
    ///                    retdec: ??1CDaoException@@UAE@XZ
    /// ```
    ///
    /// Nothing records which build either source described, so a disagreement
    /// is not something this can settle -- it is consulted second and never
    /// overrides.
    secondary: Option<std::sync::Arc<crate::fpk::FpkReader>>,
    /// DLLs already decoded from `packed`.
    ///
    /// Entries are `&'static` because `resolve` hands out references into them
    /// and the eagerly built map they replace was never freed either. Bounded
    /// by the DLLs a binary actually imports from, which is a handful of the
    /// 1,393 in the table.
    resolved: std::sync::Arc<std::sync::Mutex<HashMap<String, &'static HashMap<u32, String>>>>,
}

static ORDINAL_CACHE: std::sync::OnceLock<Result<OrdinalDatabase, OrdinalError>> =
    std::sync::OnceLock::new();


/// One DLL's `ordinal -> name` entries from a packed table.
///
/// The record is `<dll>|<ordinal>:<name>,<ordinal>:<name>`, so a lookup reads
/// the one block whose key range covers `dll` and scans it for that prefix.
fn read_dll_table(reader: &crate::fpk::FpkReader, dll: &str) -> Option<Vec<(u32, String)>> {
    let block = reader.block_for(dll).ok().flatten()?;
    let line = block
        .lines()
        .find(|l| l.strip_prefix(dll).is_some_and(|rest| rest.starts_with('|')))?;
    let body = line.split_once('|')?.1;
    Some(
        body.split(',')
            .filter_map(|pair| {
                let (ordinal, name) = pair.split_once(':')?;
                Some((ordinal.parse::<u32>().ok()?, name.to_string()))
            })
            .collect(),
    )
}

impl OrdinalDatabase {
    pub fn try_new() -> Result<Self, OrdinalError> {
        ORDINAL_CACHE.get_or_init(Self::load).clone()
    }

    fn load() -> Result<Self, OrdinalError> {
        let resources = ResourceProvider::global();
        // One record per DLL, keyed by its name: `dll|ordinal:name,ordinal:name`.
        // Loading the JSON instead costs 78ms per process to build a map that a
        // binary consults for a handful of DLLs.
        let secondary = resources
            .ordinals_json_path("ghidra_exports.fpk")
            .and_then(|path| crate::fpk::FpkReader::open(&path).ok())
            .map(std::sync::Arc::new);
        if let Some(path) = resources.ordinals_json_path("ordinals.fpk")
            && let Ok(reader) = crate::fpk::FpkReader::open(&path)
        {
            return Ok(Self {
                tables: std::sync::Arc::new(HashMap::new()),
                packed: Some(std::sync::Arc::new(reader)),
                secondary,
                resolved: Default::default(),
            });
        }
        let mut tables: HashMap<String, HashMap<u32, String>> = HashMap::new();
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
            packed: None,
            secondary,
            resolved: Default::default(),
        })
    }

    /// The ordinal table for one DLL, decoded on first use.
    fn table_for(&self, dll: &str) -> Option<&'static HashMap<u32, String>> {
        if let Ok(cache) = self.resolved.lock()
            && let Some(hit) = cache.get(dll)
        {
            return Some(*hit);
        }
        let mut table = HashMap::new();
        // Primary first, then the secondary fills gaps it left. `entry().or_insert`
        // is what keeps the second source additive.
        for (reader, is_primary) in [
            (self.packed.as_ref(), true),
            (self.secondary.as_ref(), false),
        ] {
            let Some(reader) = reader else { continue };
            let _ = is_primary;
            if let Some(entries) = read_dll_table(reader, dll) {
                for (ordinal, name) in entries {
                    table.entry(ordinal).or_insert(name);
                }
            }
        }
        if table.is_empty() {
            return None;
        }
        let table: &'static HashMap<u32, String> = Box::leak(Box::new(table));
        if let Ok(mut cache) = self.resolved.lock() {
            cache.insert(dll.to_string(), table);
        }
        return Some(table);
    }


    /// Resolve `dll_name` (any case, `.dll` suffix optional) + ordinal to an
    /// export name, if known.
    pub fn resolve(&self, dll_name: &str, ordinal: u32) -> Option<&str> {
        let normalized = normalize_dll_name(dll_name);
        if let Some(name) = self
            .tables
            .get(&normalized)
            .and_then(|by_ordinal| by_ordinal.get(&ordinal))
        {
            return Some(name.as_str());
        }
        Some(self.table_for(&normalized)?.get(&ordinal)?.as_str())
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

#[cfg(test)]
mod packed_tests {
    use super::*;

    /// The packed and JSON paths must answer identically, including the
    /// normalisation `resolve` does and the misses.
    /// Ghidra's export tables add DLLs the primary lacks and never override it.
    #[test]
    fn ghidra_exports_fill_gaps_without_overriding() {
        let Some(path) = ResourceProvider::global().ordinals_json_path("ghidra_exports.fpk") else {
            return; // bundle not present in this checkout
        };
        if !path.exists() {
            return;
        }
        let db = OrdinalDatabase::try_new().expect("load");

        // mfc140 is in Ghidra's tables and in no RetDec one.
        assert!(
            db.resolve("mfc140u.dll", 1000).is_some(),
            "a DLL only Ghidra carries should resolve"
        );

        // mfc80u #6323 is in both and they disagree; the primary has to win,
        // because nothing records which MFC build either source described.
        assert_eq!(
            db.resolve("mfc80u.dll", 6323),
            Some("??1CDaoException@@UAE@XZ"),
            "the secondary must not override the primary"
        );

        assert_eq!(db.resolve("no-such-dll.dll", 1), None);
    }

    #[test]
    fn packed_and_json_agree() {
        let Some(fpk) = ResourceProvider::global().ordinals_json_path("ordinals.fpk") else {
            return; // bundle not present in this checkout
        };
        if !fpk.exists() {
            return;
        }
        let packed = OrdinalDatabase::try_new().expect("packed load");
        assert!(packed.packed.is_some(), "test needs the packed path");

        // Build the JSON form directly for comparison.
        let mut json = OrdinalDatabase {
            tables: Default::default(),
            packed: None,
            secondary: None,
            resolved: Default::default(),
        };
        let mut tables: HashMap<String, HashMap<u32, String>> = HashMap::new();
        // The JSON moved to `utils/source/ordinals` and is not shipped, so this
        // comparison only runs in a tree that still has the packer inputs.
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../utils/source/ordinals");
        for filename in ["x86_ordinals.json", "arm_ordinals.json"] {
            let path = source.join(filename);
            let Ok(content) = fs::read_to_string(&path) else { continue };
            let raw: HashMap<String, HashMap<String, String>> =
                serde_json::from_str(&content).expect("parse json");
            for (dll, entries) in raw {
                let by_ordinal = tables.entry(dll).or_default();
                for (ord, name) in entries {
                    if let Ok(ord) = ord.parse::<u32>() {
                        by_ordinal.entry(ord).or_insert(name);
                    }
                }
            }
        }
        json.tables = std::sync::Arc::new(tables);

        let mut checked = 0usize;
        for (dll, table) in json.tables.iter() {
            for ordinal in table.keys() {
                assert_eq!(
                    packed.resolve(dll, *ordinal),
                    json.resolve(dll, *ordinal),
                    "{dll} #{ordinal}"
                );
                checked += 1;
            }
        }
        assert!(checked > 100_000, "expected the full corpus, got {checked}");

        // Misses and normalisation.
        assert_eq!(packed.resolve("no-such-dll.dll", 1), None);
        assert_eq!(packed.resolve("KERNEL32.DLL", 1), packed.resolve("kernel32", 1));
    }
}
