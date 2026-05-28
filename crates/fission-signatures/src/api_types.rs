//! API type signatures loaded from the resolved signatures corpus (`ResourceProvider`).

use fission_core::resources::ResourceProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Function signature with parameter and return types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiSignature {
    pub name: String,
    pub return_type: String,
    pub params: Vec<ParamInfo>,
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

    pub fn merge_path(&mut self, path: &Path) -> Result<(), ApiTypeError> {
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
            self.signatures.insert(
                name.to_string(),
                ApiSignature {
                    name: name.to_string(),
                    return_type: return_type.to_string(),
                    params,
                },
            );
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
        let zw_term = db
            .get("ZwTerminateProcess")
            .expect("ZwTerminateProcess");
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
        let mm_copy = db
            .get("MmCopyVirtualMemory")
            .expect("MmCopyVirtualMemory");
        assert_eq!(mm_copy.params.len(), 7);
        let ob_reg = db
            .get("ObRegisterCallbacks")
            .expect("ObRegisterCallbacks");
        assert_eq!(ob_reg.params.len(), 2);
    }
}
