//! Go runtime API snapshot loader.
//!
//! Parses the `go1.X.Y.json` snapshot files produced by the Ghidra go-api-parser tool
//! (see `Ghidra/Features/Base/src/main/java/ghidra/app/util/bin/format/golang/rtti/GoApiSnapshot.java`).
//!
//! Each JSON file is a map of platform-key → `{Funcs, Types}`.  Platform keys follow the
//! `GOOS-GOARCH` naming convention (`"all"`, `"amd64"`, `"linux"`, `"linux-amd64"`, …).
//! We merge keys in the order Ghidra uses: `all → goarch → goos → unix (if unix-like) → goos-goarch`.
//!
//! The resulting [`GoTypeinfoDatabase`] exposes function-parameter hints under the canonical
//! Go symbol name (e.g. `"fmt.Printf"`, `"os.(*File).Write"`).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// JSON serde types (mirror GoApiSnapshot schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonParam {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "DataType")]
    data_type: String,
}

#[derive(Debug, Deserialize)]
struct JsonResult {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "DataType")]
    data_type: String,
}

#[derive(Debug, Deserialize)]
struct JsonFuncSig {
    #[serde(rename = "Params", default)]
    params: Vec<JsonParam>,
    #[serde(rename = "Results", default)]
    results: Vec<JsonResult>,
}

#[derive(Debug, Deserialize)]
struct JsonTypeEntry {
    #[serde(rename = "Kind")]
    kind: String,
    /// For alias/interface kinds
    #[serde(rename = "Target", default)]
    target: String,
    /// For struct kinds
    #[serde(rename = "Fields", default)]
    fields: Vec<JsonField>,
}

#[derive(Debug, Deserialize)]
struct JsonField {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "DataType")]
    data_type: String,
}

#[derive(Debug, Deserialize)]
struct JsonPlatformEntry {
    #[serde(rename = "Funcs", default)]
    funcs: HashMap<String, JsonFuncSig>,
    #[serde(rename = "Types", default)]
    types: HashMap<String, JsonTypeEntry>,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single Go function signature: parameter names+types and named return types.
#[derive(Debug, Clone)]
pub struct GoFuncSig {
    pub params: Vec<(String, String)>,
    /// `(name, type)` pairs; name may be empty for unnamed returns.
    pub results: Vec<(String, String)>,
}

/// A single Go type entry (struct, alias, or interface).
#[derive(Debug, Clone)]
pub struct GoTypeEntry {
    pub kind: String,
    /// Struct fields (name, type) or empty for aliases/interfaces.
    pub fields: Vec<(String, String)>,
    /// Alias target (for Kind == "alias").
    pub target: String,
}

/// Flat function+type database loaded from a Go API snapshot JSON file.
///
/// Keys are canonical Go symbol names (e.g. `"fmt.Println"`, `"os.(*File).Read"`).
#[derive(Debug, Default)]
pub struct GoTypeinfoDatabase {
    pub funcs: HashMap<String, GoFuncSig>,
    pub types: HashMap<String, GoTypeEntry>,
}

// ---------------------------------------------------------------------------
// GOOS constants
// ---------------------------------------------------------------------------

const UNIX_GOOS: &[&str] = &[
    "aix",
    "android",
    "darwin",
    "dragonfly",
    "freebsd",
    "hurd",
    "illumos",
    "ios",
    "linux",
    "netbsd",
    "openbsd",
    "solaris",
];

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl GoTypeinfoDatabase {
    /// Load a Go API snapshot JSON for the given `version`, `goos`, and `goarch`.
    ///
    /// `version` should be the raw buildinfo string like `"go1.22.3"`.
    /// Patch-level is stripped to find the base `go1.22.0.json` file (Ghidra's convention).
    ///
    /// Returns `None` if the file is not found; propagates JSON parse errors.
    pub fn load_for_binary(
        version: &str,
        goos: &str,
        goarch: &str,
        typeinfo_dir: &Path,
    ) -> Option<Self> {
        let json_path = resolve_json_path(version, typeinfo_dir)?;
        let file = std::fs::File::open(&json_path).ok()?;
        let reader = std::io::BufReader::new(file);
        let raw: HashMap<String, JsonPlatformEntry> = serde_json::from_reader(reader)
            .map_err(|e| {
                log::warn!("[GoTypeinfo] Failed to parse {:?}: {}", json_path, e);
            })
            .ok()?;

        Some(Self::from_raw(raw, goos, goarch))
    }

    fn from_raw(raw: HashMap<String, JsonPlatformEntry>, goos: &str, goarch: &str) -> Self {
        let is_unix = UNIX_GOOS.contains(&goos);
        let mut db = Self::default();

        // Merge order (lowest → highest priority): all, arch, os, unix, os-arch
        let merge_keys: Vec<String> = {
            let mut v = vec!["all".to_string(), goarch.to_string(), goos.to_string()];
            if is_unix {
                v.push("unix".to_string());
            }
            v.push(format!("{}-{}", goos, goarch));
            v
        };

        for key in &merge_keys {
            if let Some(entry) = raw.get(key.as_str()) {
                for (name, sig) in &entry.funcs {
                    db.funcs.entry(name.clone()).or_insert_with(|| GoFuncSig {
                        params: sig
                            .params
                            .iter()
                            .map(|p| (p.name.clone(), p.data_type.clone()))
                            .collect(),
                        results: sig
                            .results
                            .iter()
                            .map(|r| (r.name.clone(), r.data_type.clone()))
                            .collect(),
                    });
                }
                for (name, ty) in &entry.types {
                    db.types.entry(name.clone()).or_insert_with(|| GoTypeEntry {
                        kind: ty.kind.clone(),
                        fields: ty
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.data_type.clone()))
                            .collect(),
                        target: ty.target.clone(),
                    });
                }
            }
        }

        db
    }

    /// Number of function signatures loaded.
    pub fn func_count(&self) -> usize {
        self.funcs.len()
    }

    /// Number of type entries loaded.
    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    /// Look up a function signature by its canonical Go name.
    pub fn get_func(&self, name: &str) -> Option<&GoFuncSig> {
        self.funcs.get(name)
    }

    /// Infer GOOS from a binary format string (e.g. `"ELF"` → `"linux"`, `"Mach-O"` → `"darwin"`).
    pub fn goos_from_format(format: &str) -> &'static str {
        let upper = format.to_ascii_uppercase();
        if upper.contains("MACH") {
            "darwin"
        } else if upper.starts_with("PE") {
            "windows"
        } else {
            "linux"
        }
    }

    /// Infer GOARCH from `is_64bit` and arch_spec (e.g. `"amd64"` / `"386"`).
    pub fn goarch_from_spec(is_64bit: bool, arch_spec: &str) -> &'static str {
        let spec = arch_spec.to_ascii_lowercase();
        if spec.contains("arm") && spec.contains("64") {
            "arm64"
        } else if spec.contains("arm") {
            "arm"
        } else if spec.contains("aarch64") {
            "arm64"
        } else if spec.contains("mips") && spec.contains("64") {
            "mips64"
        } else if spec.contains("mips") {
            "mips"
        } else if is_64bit {
            "amd64"
        } else {
            "386"
        }
    }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolve a `go1.X.Y.json` (or `go1.X.json` for patch-0) path under `typeinfo_dir/golang/`.
///
/// Strategy: strip patch to 0 for the base file (`go1.22.0.json`); also accepts `go1.22.json`.
fn resolve_json_path(version: &str, typeinfo_dir: &Path) -> Option<std::path::PathBuf> {
    let golang_dir = typeinfo_dir.join("golang");
    if !golang_dir.exists() {
        return None;
    }

    // Normalise: strip leading "go" prefix if present
    let ver = version.strip_prefix("go").unwrap_or(version);
    // ver is now "1.22.3" or "1.22"
    let parts: Vec<&str> = ver.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let major = parts[0];
    let minor = parts[1];

    // Try go1.<major>.<minor>.json with patch stripped to 0
    let base0 = golang_dir.join(format!("go{}.{}.0.json", major, minor));
    if base0.exists() {
        return Some(base0);
    }
    // Try go1.<major>.<minor>.json (no patch component)
    let base_short = golang_dir.join(format!("go{}.{}.json", major, minor));
    if base_short.exists() {
        return Some(base_short);
    }
    // Fallback: iterate for any go1.<major>.<minor>.*.json
    if let Ok(entries) = std::fs::read_dir(&golang_dir) {
        let prefix = format!("go{}.{}.", major, minor);
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                return Some(golang_dir.join(name));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Global per-version cache (avoids re-parsing 80 MB+ JSON per function call)
// ---------------------------------------------------------------------------

use once_cell::sync::Lazy;
use std::sync::Mutex;

static DB_CACHE: Lazy<
    Mutex<HashMap<(String, String, String), Option<std::sync::Arc<GoTypeinfoDatabase>>>>,
> = Lazy::new(|| Mutex::new(HashMap::new()));

impl GoTypeinfoDatabase {
    /// Cached variant of [`load_for_binary`]: the JSON is parsed at most once per
    /// (version, goos, goarch) triple for the lifetime of the process.
    pub fn get_cached(
        version: &str,
        goos: &str,
        goarch: &str,
        typeinfo_dir: &Path,
    ) -> Option<std::sync::Arc<Self>> {
        let key = (version.to_string(), goos.to_string(), goarch.to_string());
        {
            let guard = DB_CACHE.lock().unwrap();
            if let Some(entry) = guard.get(&key) {
                return entry.clone();
            }
        }
        // Load outside the lock to avoid blocking
        let loaded =
            Self::load_for_binary(version, goos, goarch, typeinfo_dir).map(std::sync::Arc::new);
        let mut guard = DB_CACHE.lock().unwrap();
        // Another thread may have raced; prefer theirs
        guard.entry(key).or_insert(loaded).clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goos_from_format() {
        assert_eq!(GoTypeinfoDatabase::goos_from_format("ELF"), "linux");
        assert_eq!(GoTypeinfoDatabase::goos_from_format("Mach-O"), "darwin");
        assert_eq!(GoTypeinfoDatabase::goos_from_format("PE64"), "windows");
    }

    #[test]
    fn test_goarch_from_spec() {
        assert_eq!(
            GoTypeinfoDatabase::goarch_from_spec(true, "x86:LE:64:default"),
            "amd64"
        );
        assert_eq!(
            GoTypeinfoDatabase::goarch_from_spec(false, "x86:LE:32:default"),
            "386"
        );
        assert_eq!(
            GoTypeinfoDatabase::goarch_from_spec(true, "AARCH64:LE:64:v8A"),
            "arm64"
        );
    }

    #[test]
    fn test_resolve_json_path_nonexistent() {
        let tmp = std::path::PathBuf::from("/nonexistent");
        assert!(resolve_json_path("go1.22.3", &tmp).is_none());
    }

    fn workspace_typeinfo_dir() -> Option<std::path::PathBuf> {
        // Walk up from CARGO_MANIFEST_DIR to workspace root
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for _ in 0..5 {
            let candidate = dir.join("utils").join("signatures").join("typeinfo");
            if candidate.join("golang").exists() {
                return Some(candidate);
            }
            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
        None
    }

    #[test]
    fn test_resolve_json_path_real() {
        let Some(typeinfo_dir) = workspace_typeinfo_dir() else {
            eprintln!("skipped: typeinfo dir not found");
            return;
        };
        let path = resolve_json_path("go1.22.3", &typeinfo_dir);
        assert!(path.is_some(), "go1.22.0.json should resolve for go1.22.3");
        let path = path.unwrap();
        assert!(path.exists(), "resolved path must exist: {:?}", path);
        eprintln!("resolved: {:?}", path);
    }

    #[test]
    fn test_load_go1_22_linux_amd64() {
        let Some(typeinfo_dir) = workspace_typeinfo_dir() else {
            eprintln!("skipped: typeinfo dir not found");
            return;
        };
        let db = GoTypeinfoDatabase::load_for_binary("go1.22.3", "linux", "amd64", &typeinfo_dir);
        assert!(db.is_some(), "should load go1.22.0.json");
        let db = db.unwrap();
        eprintln!("funcs={} types={}", db.func_count(), db.type_count());
        assert!(
            db.func_count() > 1000,
            "expected many functions, got {}",
            db.func_count()
        );

        // fmt.Println must be present in 'all'
        let println = db.get_func("fmt.Println");
        assert!(println.is_some(), "fmt.Println should be in snapshot");
        let sig = println.unwrap();
        assert!(!sig.params.is_empty(), "fmt.Println must have params");
        eprintln!("fmt.Println params: {:?}", sig.params);
        eprintln!("fmt.Println results: {:?}", sig.results);

        // os.ReadFile should be present
        let readfile = db.get_func("os.ReadFile");
        assert!(readfile.is_some(), "os.ReadFile should be in snapshot");
        eprintln!("os.ReadFile params: {:?}", readfile.unwrap().params);
    }

    #[test]
    fn test_load_go1_22_darwin_arm64() {
        let Some(typeinfo_dir) = workspace_typeinfo_dir() else {
            eprintln!("skipped: typeinfo dir not found");
            return;
        };
        let db = GoTypeinfoDatabase::load_for_binary("go1.22.3", "darwin", "arm64", &typeinfo_dir);
        assert!(db.is_some(), "should load go1.22.0.json for darwin/arm64");
        let db = db.unwrap();
        eprintln!(
            "darwin/arm64: funcs={} types={}",
            db.func_count(),
            db.type_count()
        );
        // darwin should have more funcs than linux due to extra darwin-arm64 key
        assert!(db.func_count() > 1000);
    }
}
