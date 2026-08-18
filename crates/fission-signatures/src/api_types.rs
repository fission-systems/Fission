//! API type signatures loaded from the resolved signatures corpus (`ResourceProvider`).

use fission_core::resources::ResourceProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
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

#[derive(Debug, Clone, Default)]
pub struct ApiTypeDatabase {
    signatures: HashMap<String, ApiSignature>,
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
    pub fn merge_path(&mut self, path: &Path) -> Result<(), ApiTypeError> {
        let packed = path.with_extension("fpk");
        if packed.exists()
            && let Ok(reader) = crate::fpk::FpkReader::open(&packed)
            && let Ok(records) = reader.read_all()
        {
            return self.merge_pipe_text(&packed, &records.join("\n"));
        }
        let content = fs::read_to_string(path).map_err(|source| ApiTypeError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        self.merge_pipe_text(path, &content)
    }

    fn merge_pipe_text(&mut self, path: &Path, content: &str) -> Result<(), ApiTypeError> {
        for (line_idx, raw_line) in content.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() != 3 {
                return Err(ApiTypeError::Parse {
                    path: path.to_path_buf(),
                    line: line_no,
                    reason: "expected name|return_type|params".to_string(),
                });
            }
            let name = parts[0].trim();
            let return_type = parts[1].trim();
            if name.is_empty() || return_type.is_empty() {
                return Err(ApiTypeError::Parse {
                    path: path.to_path_buf(),
                    line: line_no,
                    reason: "name and return type must be non-empty".to_string(),
                });
            }
            let mut params = Vec::new();
            let params_text = parts[2].trim();
            if !params_text.is_empty() && params_text != "void" {
                for param in params_text.split(',') {
                    let param = param.trim();
                    if param.is_empty() {
                        continue;
                    }
                    if param == "..." {
                        continue;
                    }
                    let Some((param_name, type_name)) = param.split_once(':') else {
                        return Err(ApiTypeError::Parse {
                            path: path.to_path_buf(),
                            line: line_no,
                            reason: format!("invalid parameter '{param}'"),
                        });
                    };
                    params.push(ParamInfo {
                        name: param_name.trim().to_string(),
                        type_name: type_name.trim().to_string(),
                        enum_group: None,
                    });
                }
            }
            let candidate = ApiSignature {
                name: name.to_string(),
                return_type: return_type.to_string(),
                params,
            };
            // Files merge in a fixed order and the map used to take whichever
            // arrived last, so `mac_osx_signatures.txt` -- 3,801 entries, not
            // one of them carrying a type -- overwrote 302 signatures that did.
            //
            // An entry is now replaced only by one that says at least as much.
            // Ties still go to the later file, which is what keeps the 64-bit
            // C library winning over the 32-bit one on the 30,998 names they
            // share; only strictly-less-informative overwrites are refused.
            match self.signatures.entry(name.to_string()) {
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
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ApiSignature> {
        self.signatures.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ApiSignature> {
        self.signatures.values()
    }

    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
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
}
