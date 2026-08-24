//! API type signatures loaded from the resolved signatures corpus (`ResourceProvider`).

use fission_core::resources::ResourceProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Mutex, OnceLock};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiTypeError {
    #[error("api signature file was not found")]
    NotFound,
    #[error("failed to read api signature file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid api signature at {path}:{line}: {reason}")]
    Parse {
        path: PathBuf,
        line: usize,
        reason: String,
    },
}

/// Parameter type information with optional enum group for context-aware constant resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamInfo {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_group: Option<String>,
}

/// Whether a signature type string names a type, or is the extractor's
/// placeholder for "nothing was recovered".
///
/// The GDT extractor resolves a type ID only when it lands in the built-in
/// table, so an unresolved typedef, composite or pointer is written `int`. That
/// makes `int` ambiguous in this data: it is either a recovered `int` or a lost
/// `FILE *`. Treating it as absent costs nothing for the former, because `int`
/// is where inference lands anyway.
pub fn type_name_is_informative(type_name: &str) -> bool {
    !matches!(type_name.trim(), "" | "int" | "long" | "void")
}

/// Whether a rendered pointer surface carries declaration information beyond
/// the structural integer-pointer lattice.
///
/// Character pointers retain source-relevant signedness and named/aggregate
/// pointers retain a declaration identity. Width-only primitive spellings do
/// neither; transporting e.g. `longlong **` from an isolated decompilation can
/// turn a caller's aggregate pointer into an invented extra indirection.
pub fn pointer_surface_type_name_is_specific(type_name: &str) -> bool {
    let compact = type_name
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if !compact.ends_with('*') {
        return false;
    }
    let mut base = compact.trim_end_matches('*');
    loop {
        let stripped = ["const", "volatile", "restrict"]
            .into_iter()
            .find_map(|qualifier| base.strip_prefix(qualifier));
        match stripped {
            Some(rest) => base = rest,
            None => break,
        }
    }
    !matches!(
        base,
        "" | "void"
            | "lpvoid"
            | "pvoid"
            | "bool"
            | "_bool"
            | "byte"
            | "word"
            | "dword"
            | "qword"
            | "short"
            | "ushort"
            | "signedshort"
            | "unsignedshort"
            | "int"
            | "uint"
            | "signedint"
            | "unsignedint"
            | "long"
            | "ulong"
            | "signedlong"
            | "unsignedlong"
            | "longlong"
            | "ulonglong"
            | "signedlonglong"
            | "unsignedlonglong"
            | "int8_t"
            | "uint8_t"
            | "int16_t"
            | "uint16_t"
            | "int32_t"
            | "uint32_t"
            | "int64_t"
            | "uint64_t"
            | "intptr_t"
            | "uintptr_t"
            | "size_t"
            | "ssize_t"
    )
}

/// Function signature with parameter and return types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiSignature {
    pub name: String,
    pub return_type: String,
    pub params: Vec<ParamInfo>,
}

impl ApiSignature {
    /// How many of this entry's type strings actually name a type.
    ///
    /// Used to order two entries for the same function: the one that says more
    /// wins, so a file full of placeholders cannot erase a recovered signature.
    pub fn informative_type_count(&self) -> usize {
        usize::from(type_name_is_informative(&self.return_type))
            + self
                .params
                .iter()
                .filter(|p| type_name_is_informative(&p.type_name))
                .count()
    }
}


/// One `name|return_type|param:type,...` record.
fn parse_record(line: &str) -> Result<ApiSignature, String> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() != 3 {
        return Err("expected name|return_type|params".to_string());
    }
    let name = parts[0].trim();
    let return_type = parts[1].trim();
    if name.is_empty() || return_type.is_empty() {
        return Err("name and return type must be non-empty".to_string());
    }
    let mut params = Vec::new();
    let params_text = parts[2].trim();
    if !params_text.is_empty() && params_text != "void" {
        for param in params_text.split(',') {
            let param = param.trim();
            if param.is_empty() || param == "..." {
                continue;
            }
            let Some((param_name, type_name)) = param.split_once(':') else {
                return Err(format!("invalid parameter '{param}'"));
            };
            params.push(ParamInfo {
                name: param_name.trim().to_string(),
                type_name: type_name.trim().to_string(),
                enum_group: None,
            });
        }
    }
    Ok(ApiSignature {
        name: name.to_string(),
        return_type: return_type.to_string(),
        params,
    })
}

/// Merge one record under the rule the whole database uses.
///
/// Sources are consulted in a fixed order and an entry is replaced only by one
/// with at least as many informative type strings. Ties go to the later source,
/// which is what keeps the 64-bit C library winning over the 32-bit one on the
/// 30,998 names they share; only strictly-less-informative overwrites are
/// refused. Before this, `mac_osx_signatures` -- 3,801 entries, not one of them
/// carrying a type -- merged last and erased 302 signatures that did.
fn merge_into(into: &mut HashMap<String, ApiSignature>, candidate: ApiSignature) {
    match into.entry(candidate.name.clone()) {
        Entry::Occupied(mut slot) => {
            if candidate.informative_type_count() >= slot.get().informative_type_count() {
                slot.insert(candidate);
            }
        }
        Entry::Vacant(slot) => {
            slot.insert(candidate);
        }
    }
}

/// Pick the winner among candidates offered in source order.
fn merge_candidates(candidates: Vec<ApiSignature>) -> Option<ApiSignature> {
    let mut best: Option<ApiSignature> = None;
    for candidate in candidates {
        match &best {
            Some(current)
                if candidate.informative_type_count() < current.informative_type_count() => {}
            _ => best = Some(candidate),
        }
    }
    best
}

#[derive(Default)]
pub struct ApiTypeDatabase {
    /// Records from sources that had no `.fpk`, merged eagerly.
    signatures: HashMap<String, ApiSignature>,
    /// Packed sources, in merge order, read one block at a time.
    packed: Vec<crate::fpk::FpkReader>,
    /// Names already resolved against `packed`.
    ///
    /// Entries are `&'static` because the database itself is a process-lifetime
    /// `LazyLock` -- the eager map it replaces was never freed either -- and
    /// because `get` hands out references. Bounded by the number of distinct
    /// names a run looks up, which is hundreds, not the 151,408 in the tables.
    resolved: Mutex<HashMap<String, Option<&'static ApiSignature>>>,
    /// Everything, materialised only if someone iterates.
    materialised: OnceLock<HashMap<String, ApiSignature>>,
}

impl std::fmt::Debug for ApiTypeDatabase {
    /// Deliberately does not materialise: formatting a database for a log
    /// should not decompress every table.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiTypeDatabase")
            .field("eager_entries", &self.signatures.len())
            .field("packed_tables", &self.packed.len())
            .field("materialised", &self.materialised.get().is_some())
            .finish()
    }
}

impl ApiTypeDatabase {
    pub fn from_utils_signatures() -> Result<Self, ApiTypeError> {
        let mut db = Self::default();
        if let Some(path) = ResourceProvider::global().win_api_signatures_txt() {
            db.merge_path(&path)?;
        }
        if let Some(path) = ResourceProvider::global().ntoskrnl_signatures_txt() {
            db.merge_path(&path)?;
        }
        if let Some(path) = ResourceProvider::global().wdk_signatures_txt() {
            db.merge_path(&path)?;
        }
        if let Some(path) = ResourceProvider::global().generic_clib_signatures_txt() {
            db.merge_path(&path)?;
        }
        if let Some(path) = ResourceProvider::global().generic_clib_64_signatures_txt() {
            db.merge_path(&path)?;
        }
        if let Some(path) = ResourceProvider::global().mac_osx_signatures_txt() {
            db.merge_path(&path)?;
        }
        Ok(db)
    }

    pub fn from_path(path: &Path) -> Result<Self, ApiTypeError> {
        let mut db = Self::default();
        db.merge_path(path)?;
        Ok(db)
    }

    /// Merge a signature table, preferring the `.fpk` beside it.
    ///
    /// The packed form holds the same records -- block payloads are the
    /// original text -- so this is a change of container, not of content. It is
    /// preferred rather than required: a checkout with only the `.txt` present
    /// still loads, which is what keeps the two forms interchangeable while the
    /// bundle carries both.
    /// Merge a signature table, preferring the `.fpk` for it.
    ///
    /// `path` names the `.txt` the table was built from. That text moved to
    /// `utils/source/` and is normally absent, so a missing `.txt` with a
    /// present `.fpk` is the expected shape rather than an error.
    pub fn merge_path(&mut self, path: &Path) -> Result<(), ApiTypeError> {
        let packed_path = path.with_extension("fpk");
        if packed_path.exists()
            && let Ok(reader) = crate::fpk::FpkReader::open(&packed_path)
        {
            // Kept unread. A lookup decompresses the one block that could hold
            // the name; building the map here would spend 64.6ms to answer
            // questions that may never be asked.
            self.packed.push(reader);
            return Ok(());
        }
        let content = fs::read_to_string(path).map_err(|source| ApiTypeError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        self.merge_pipe_text(path, &content)
    }

    fn merge_pipe_text(&mut self, path: &Path, content: &str) -> Result<(), ApiTypeError> {
        for (line_idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let candidate = parse_record(line).map_err(|reason| ApiTypeError::Parse {
                path: path.to_path_buf(),
                line: line_idx + 1,
                reason,
            })?;
            merge_into(&mut self.signatures, candidate);
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ApiSignature> {
        if self.packed.is_empty() {
            return self.signatures.get(name);
        }
        if let Ok(cache) = self.resolved.lock()
            && let Some(hit) = cache.get(name)
        {
            return *hit;
        }
        let mut candidates: Vec<ApiSignature> = Vec::new();
        if let Some(eager) = self.signatures.get(name) {
            candidates.push(eager.clone());
        }
        for reader in &self.packed {
            let Ok(Some(block)) = reader.block_for(name) else {
                continue;
            };
            for line in block.lines() {
                let Some(rest) = line.strip_prefix(name) else {
                    continue;
                };
                if !rest.starts_with('|') {
                    continue;
                }
                if let Ok(record) = parse_record(line) {
                    candidates.push(record);
                }
                break;
            }
        }
        // Leaked so `get` can hand out a reference; see `resolved`.
        let winner: Option<&'static ApiSignature> =
            merge_candidates(candidates).map(|s| &*Box::leak(Box::new(s)));
        if let Ok(mut cache) = self.resolved.lock() {
            cache.insert(name.to_string(), winner);
        }
        winner
    }

    /// Everything the sources hold, merged. Only for callers that genuinely
    /// need the whole table -- a lookup should use [`ApiTypeDatabase::get`].
    fn all(&self) -> &HashMap<String, ApiSignature> {
        if self.packed.is_empty() {
            return &self.signatures;
        }
        self.materialised.get_or_init(|| {
            let mut merged = self.signatures.clone();
            for reader in &self.packed {
                let Ok(records) = reader.read_all() else {
                    continue;
                };
                for line in records {
                    if let Ok(record) = parse_record(&line) {
                        merge_into(&mut merged, record);
                    }
                }
            }
            merged
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ApiSignature> {
        self.all().values()
    }

    pub fn len(&self) -> usize {
        self.all().len()
    }

    pub fn is_empty(&self) -> bool {
        self.all().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(files: &[&str]) -> ApiTypeDatabase {
        let mut db = ApiTypeDatabase::default();
        for (i, text) in files.iter().enumerate() {
            db.merge_pipe_text(Path::new(&format!("f{i}")), text)
                .expect("parse");
        }
        db
    }

    #[test]
    fn a_typeless_entry_does_not_erase_a_typed_one() {
        // The shape that cost 302 signatures: mac_osx_signatures.txt carries no
        // types at all and merges last.
        let db = merged(&[
            "fopen|FILE*|__filename:char*,__modes:char*\n",
            "fopen|int|__filename:int,__modes:int\n",
        ]);
        let sig = db.get("fopen").expect("fopen");
        assert_eq!(sig.return_type, "FILE*");
        assert_eq!(sig.params[0].type_name, "char*");
    }

    #[test]
    fn a_typed_entry_replaces_a_typeless_one_whichever_order() {
        let db = merged(&[
            "fopen|int|__filename:int,__modes:int\n",
            "fopen|FILE*|__filename:char*,__modes:char*\n",
        ]);
        assert_eq!(db.get("fopen").expect("fopen").return_type, "FILE*");
    }

    #[test]
    fn equally_informative_entries_still_take_the_later_file() {
        // 30,998 names are shared by the 32- and 64-bit C library files; the
        // 64-bit one merges second and must keep winning.
        let db = merged(&["size_t|int|void\n", "size_t|long long|void\n"]);
        assert_eq!(db.get("size_t").expect("size_t").return_type, "long long");
    }

    #[test]
    fn informative_type_count_ignores_placeholders() {
        let db = merged(&["f|int|a:char*,b:int,c:FILE*\n"]);
        assert_eq!(db.get("f").expect("f").informative_type_count(), 2);
    }

    #[test]
    fn specific_pointer_surfaces_keep_declarations_not_width_spellings() {
        assert!(pointer_surface_type_name_is_specific("char **"));
        assert!(pointer_surface_type_name_is_specific("FILE*"));
        assert!(pointer_surface_type_name_is_specific("struct record *"));
        assert!(!pointer_surface_type_name_is_specific("void *"));
        assert!(!pointer_surface_type_name_is_specific("longlong **"));
        assert!(!pointer_surface_type_name_is_specific(
            "const unsigned long *"
        ));
    }

    #[test]
    fn loads_utils_win_api_signatures() {
        let db = ApiTypeDatabase::from_utils_signatures().expect("load utils api signatures");
        assert!(db.get("CloseHandle").is_some());
        assert!(db.get("VirtualAlloc").is_some());
        assert!(db.get("BCryptOpenAlgorithmProvider").is_some());
        assert!(db.get("GetClientRect").is_some());
        assert!(db.get("GetWindowRect").is_some());
        assert!(db.get("GetMessageW").is_some());
        assert!(db.len() > 100);
    }

    #[test]
    fn loads_ntoskrnl_signatures_with_correct_arity() {
        let db = ApiTypeDatabase::from_utils_signatures().expect("load utils api signatures");
        let ps_lookup = db
            .get("PsLookupProcessByProcessId")
            .expect("PsLookupProcessByProcessId");
        assert_eq!(ps_lookup.params.len(), 2);
        let zw_term = db.get("ZwTerminateProcess").expect("ZwTerminateProcess");
        assert_eq!(zw_term.params.len(), 2);
        let ke_attach = db
            .get("KeStackAttachProcess")
            .expect("KeStackAttachProcess");
        assert_eq!(ke_attach.params.len(), 2);
        let ke_detach = db
            .get("KeUnstackDetachProcess")
            .expect("KeUnstackDetachProcess");
        assert_eq!(ke_detach.params.len(), 1);
        let obf_deref = db
            .get("ObfDereferenceObject")
            .expect("ObfDereferenceObject");
        assert_eq!(obf_deref.params.len(), 1);
        let mm_copy = db.get("MmCopyVirtualMemory").expect("MmCopyVirtualMemory");
        assert_eq!(mm_copy.params.len(), 7);
        let ob_reg = db.get("ObRegisterCallbacks").expect("ObRegisterCallbacks");
        assert_eq!(ob_reg.params.len(), 2);
    }

    #[test]
    fn posix_multi_argument_contracts_are_not_void() {
        let db = ApiTypeDatabase::from_utils_signatures().expect("load utils api signatures");

        let stat = db.get("stat").expect("stat");
        assert_eq!(stat.params.len(), 2);
        assert_eq!(stat.params[0].type_name, "char*");
        assert_eq!(stat.params[1].type_name, "stat*");

        let sigaction = db.get("sigaction").expect("sigaction");
        assert_eq!(sigaction.params.len(), 3);
        assert_eq!(sigaction.params[1].type_name, "sigaction*");
        assert_eq!(sigaction.params[2].type_name, "sigaction*");

        let wait = db.get("wait").expect("wait");
        assert_eq!(wait.params.len(), 1);
        assert_eq!(wait.params[0].type_name, "int*");

        assert!(db.get("getpid").expect("getpid").params.is_empty());
    }
}
