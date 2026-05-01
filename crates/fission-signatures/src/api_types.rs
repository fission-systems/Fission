//! API type signatures loaded from the repository `utils/signatures` tree.

use fission_core::PATHS;
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
        let path = api_signature_path().ok_or(ApiTypeError::NotFound)?;
        Self::from_path(&path)
    }

    pub fn from_path(path: &Path) -> Result<Self, ApiTypeError> {
        let content = fs::read_to_string(path).map_err(|source| ApiTypeError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_pipe_text(path, &content)
    }

    fn from_pipe_text(path: &Path, content: &str) -> Result<Self, ApiTypeError> {
        let mut db = Self::default();
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
            db.signatures.insert(
                name.to_string(),
                ApiSignature {
                    name: name.to_string(),
                    return_type: return_type.to_string(),
                    params,
                },
            );
        }
        Ok(db)
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

fn api_signature_path() -> Option<PathBuf> {
    let filename = "win_api_signatures.txt";
    if let Some(gdt_dir) = &PATHS.gdt_dir {
        let path = gdt_dir.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    let root = PATHS.workspace_root.as_ref()?;
    let path = root
        .join("utils")
        .join("signatures")
        .join("typeinfo")
        .join("win32")
        .join(filename);
    path.exists().then_some(path)
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
}
