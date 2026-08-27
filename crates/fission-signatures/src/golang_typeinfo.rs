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
#[derive(Default)]
pub struct GoTypeinfoDatabase {
    pub funcs: HashMap<String, GoFuncSig>,
    pub types: HashMap<String, GoTypeEntry>,
    /// Packed tables plus the build tags to consult, in merge order.
    packed: Option<PackedTables>,
}

/// The `.fpk` form: symbols keyed by `<tag>\x1f<name>`.
///
/// `from_raw` merged the tags once at load, which meant reading the whole
/// snapshot -- 42ms for go1.20, 80ms for go1.25 -- before answering anything.
/// The same merge happens per lookup instead: the tags are tried in order and
/// the first definition wins, which is what `or_insert` did.
struct PackedTables {
    funcs: crate::fpk::FpkReader,
    types: crate::fpk::FpkReader,
    merge_tags: Vec<String>,
    resolved_funcs: std::sync::Mutex<HashMap<String, Option<&'static GoFuncSig>>>,
    resolved_types: std::sync::Mutex<HashMap<String, Option<&'static GoTypeEntry>>>,
}

impl std::fmt::Debug for GoTypeinfoDatabase {
    /// Reports shape without materialising: formatting must not decode tables.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoTypeinfoDatabase")
            .field("eager_funcs", &self.funcs.len())
            .field("eager_types", &self.types.len())
            .field("packed", &self.packed.is_some())
            .finish()
    }
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

/// Build tags to consult, lowest priority first -- the order `from_raw` merged
/// them in, so trying them in sequence and taking the first hit reproduces
/// `or_insert`'s "first definition wins".
fn merge_tags_for(goos: &str, goarch: &str) -> Vec<String> {
    let mut tags = vec!["all".to_string(), goarch.to_string(), goos.to_string()];
    if UNIX_GOOS.contains(&goos) {
        tags.push("unix".to_string());
    }
    tags.push(format!("{goos}-{goarch}"));
    tags
}

/// The record for `<tag>\x1f<name>`, if the table has one.
fn lookup_record(reader: &crate::fpk::FpkReader, tag: &str, name: &str) -> Option<String> {
    let key = format!("{tag}\u{1f}{}", escape_field(name));
    let block = reader.block_for(&key).ok().flatten()?;
    block
        .lines()
        .find(|line| {
            line.strip_prefix(key.as_str())
                .is_some_and(|rest| rest.starts_with('|'))
        })
        .map(str::to_owned)
}

/// Mirror of the packer's escaping.
fn escape_field(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace(';', "\\s")
        .replace('\u{1f}', "\\u")
        .replace('\n', "\\n")
}

fn unescape_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p') => out.push('|'),
            Some('s') => out.push(';'),
            Some('u') => out.push('\u{1f}'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `name:type;name:type` -> pairs. An empty field is no pairs, not one empty
/// pair, which is what `split` would give.
fn parse_pairs(text: &str) -> Vec<(String, String)> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(';')
        .filter_map(|pair| {
            let (name, ty) = pair.split_once(':')?;
            Some((unescape_field(name), unescape_field(ty)))
        })
        .collect()
}

fn parse_func_record(line: &str) -> Option<GoFuncSig> {
    let mut fields = line.splitn(3, '|');
    let _key = fields.next()?;
    let params = fields.next()?;
    let results = fields.next()?;
    Some(GoFuncSig {
        params: parse_pairs(params),
        results: parse_pairs(results),
    })
}

fn parse_type_record(line: &str) -> Option<GoTypeEntry> {
    let mut fields = line.splitn(4, '|');
    let _key = fields.next()?;
    let kind = fields.next()?;
    let target = fields.next()?;
    let fields_text = fields.next()?;
    Some(GoTypeEntry {
        kind: unescape_field(kind),
        target: unescape_field(target),
        fields: parse_pairs(fields_text),
    })
}

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
        // The packed tables are what ships; the JSON they were built from moved
        // to `utils/source/` and is normally absent, so they are resolved
        // directly rather than as siblings of a snapshot that may not be there.
        let stem = packed_stem_for(version, typeinfo_dir);
        let json_path = resolve_json_path(version, typeinfo_dir);
        let (fn_path, ty_path) = (
            typeinfo_dir.join("golang").join(format!("{stem}.fn.fpk")),
            typeinfo_dir.join("golang").join(format!("{stem}.ty.fpk")),
        );
        if fn_path.exists()
            && ty_path.exists()
            && let Ok(funcs) = crate::fpk::FpkReader::open(&fn_path)
            && let Ok(types) = crate::fpk::FpkReader::open(&ty_path)
        {
            return Some(Self {
                funcs: HashMap::new(),
                types: HashMap::new(),
                packed: Some(PackedTables {
                    funcs,
                    types,
                    merge_tags: merge_tags_for(goos, goarch),
                    resolved_funcs: Default::default(),
                    resolved_types: Default::default(),
                }),
            });
        }
        let json_path = json_path?;
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

    /// Number of function signatures available.
    ///
    /// Counts records in the packed table rather than decoding them: the point
    /// of the packed form is that nothing is materialised until it is asked
    /// for, and a count is not a reason to break that.
    ///
    /// The packed table holds every build tag where the eager map held only the
    /// merged view, so this is an upper bound there rather than the exact number
    /// a lookup can reach.
    pub fn func_count(&self) -> usize {
        match &self.packed {
            Some(packed) => packed.funcs.record_count() as usize,
            None => self.funcs.len(),
        }
    }

    /// Number of type entries available. See [`Self::func_count`].
    pub fn type_count(&self) -> usize {
        match &self.packed {
            Some(packed) => packed.types.record_count() as usize,
            None => self.types.len(),
        }
    }

    /// Look up a function signature by its canonical Go name.
    pub fn get_func(&self, name: &str) -> Option<&GoFuncSig> {
        if let Some(sig) = self.funcs.get(name) {
            return Some(sig);
        }
        let packed = self.packed.as_ref()?;
        if let Ok(cache) = packed.resolved_funcs.lock()
            && let Some(hit) = cache.get(name)
        {
            return *hit;
        }
        let found = packed
            .merge_tags
            .iter()
            .find_map(|tag| lookup_record(&packed.funcs, tag, name))
            .and_then(|line| parse_func_record(&line))
            // Leaked so `get_func` can return a reference, as elsewhere in this
            // crate: the database is cached for the process and the eager map
            // this replaces was never freed either.
            .map(|sig| &*Box::leak(Box::new(sig)));
        if let Ok(mut cache) = packed.resolved_funcs.lock() {
            cache.insert(name.to_string(), found);
        }
        found
    }

    /// Look up a type entry by its canonical Go name.
    pub fn get_type(&self, name: &str) -> Option<&GoTypeEntry> {
        if let Some(entry) = self.types.get(name) {
            return Some(entry);
        }
        let packed = self.packed.as_ref()?;
        if let Ok(cache) = packed.resolved_types.lock()
            && let Some(hit) = cache.get(name)
        {
            return *hit;
        }
        let found = packed
            .merge_tags
            .iter()
            .find_map(|tag| lookup_record(&packed.types, tag, name))
            .and_then(|line| parse_type_record(&line))
            .map(|entry| &*Box::leak(Box::new(entry)));
        if let Ok(mut cache) = packed.resolved_types.lock() {
            cache.insert(name.to_string(), found);
        }
        found
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
/// `go1.<major>.<minor>.0`, the name both the snapshot and its packed tables
/// carry. Derived from the version string rather than from a file on disk,
/// because the snapshot is not shipped.
fn packed_stem_for(version: &str, _typeinfo_dir: &Path) -> String {
    let ver = version.strip_prefix("go").unwrap_or(version);
    let mut parts = ver.split('.');
    let major = parts.next().unwrap_or("1");
    let minor = parts.next().unwrap_or("0");
    format!("go{major}.{minor}.0")
}

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

type DbCacheMap = HashMap<(String, String, String), Option<std::sync::Arc<GoTypeinfoDatabase>>>;

static DB_CACHE: Lazy<Mutex<DbCacheMap>> = Lazy::new(|| Mutex::new(HashMap::new()));

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
        // The snapshot JSON moved to `utils/source/` and is not shipped, so
        // what has to resolve for a version string is the packed pair. The
        // stem is derived from the version rather than found on disk, which is
        // what lets a tree without the sources still load.
        let Some(typeinfo_dir) = workspace_typeinfo_dir() else {
            eprintln!("skipped: typeinfo dir not found");
            return;
        };
        assert_eq!(packed_stem_for("go1.22.3", &typeinfo_dir), "go1.22.0");
        assert_eq!(packed_stem_for("1.22", &typeinfo_dir), "go1.22.0");

        let golang = typeinfo_dir.join("golang");
        let packed = golang.join("go1.22.0.fn.fpk");
        if !packed.exists() {
            eprintln!("skipped: packed tables not built here");
            return;
        }
        assert!(
            golang.join("go1.22.0.ty.fpk").exists(),
            "both halves ship together"
        );
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

#[cfg(test)]
mod packed_tests {
    use super::*;

    /// The packed path must answer exactly as the JSON one did, including the
    /// tag merge order -- a symbol defined in both `all` and `windows-amd64`
    /// has to resolve the same way it did when the tags were merged at load.
    #[test]
    fn packed_and_json_agree_across_platforms() {
        let dir = std::path::Path::new("/Users/sjkim1127/Fission/utils/signatures/typeinfo");
        if !dir.join("golang").exists() {
            return; // bundle not present in this checkout
        }
        for (version, goos, goarch) in [
            ("go1.25.0", "windows", "amd64"),
            ("go1.20.0", "linux", "amd64"),
        ] {
            let golang = dir.join("golang");
            let packed = match GoTypeinfoDatabase::load_for_binary(version, goos, goarch, dir) {
                Some(db) if db.packed.is_some() => db,
                _ => continue, // packed tables not built here
            };

            // Hide the packed tables so the same call takes the JSON path.
            let hidden: Vec<_> = ["fn", "ty"]
                .iter()
                .map(|kind| {
                    let from = golang.join(format!("{version}.{kind}.fpk"));
                    let to = std::env::temp_dir()
                        .join(format!("{version}.{kind}.{}.hidden", std::process::id()));
                    std::fs::rename(&from, &to).ok();
                    (from, to)
                })
                .collect();
            let json = GoTypeinfoDatabase::load_for_binary(version, goos, goarch, dir);
            for (from, to) in &hidden {
                std::fs::rename(to, from).ok();
            }
            let Some(json) = json else { continue };

            for (name, sig) in &json.funcs {
                let got = packed.get_func(name).unwrap_or_else(|| {
                    panic!("{version} {goos}/{goarch}: {name} missing from packed")
                });
                assert_eq!(got.params, sig.params, "{name} params");
                assert_eq!(got.results, sig.results, "{name} results");
            }
            for (name, entry) in &json.types {
                let got = packed.get_type(name).unwrap_or_else(|| {
                    panic!("{version} {goos}/{goarch}: type {name} missing from packed")
                });
                assert_eq!(got.kind, entry.kind, "{name} kind");
                assert_eq!(got.target, entry.target, "{name} target");
                assert_eq!(got.fields, entry.fields, "{name} fields");
            }
            assert!(packed.get_func("no.such.Symbol").is_none());
            assert!(json.funcs.len() > 10_000, "expected a full snapshot");
        }
    }

    #[test]
    fn merge_order_matches_what_from_raw_used() {
        // all, arch, os, unix when the OS is one, then os-arch.
        assert_eq!(
            merge_tags_for("linux", "amd64"),
            vec!["all", "amd64", "linux", "unix", "linux-amd64"]
        );
        assert_eq!(
            merge_tags_for("windows", "386"),
            vec!["all", "386", "windows", "windows-386"]
        );
    }
}
