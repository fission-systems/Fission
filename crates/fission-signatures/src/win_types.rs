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

/// Windows structures database
pub struct WindowsStructures {
    pub structures: HashMap<String, StructDef>,
}

impl WindowsStructures {
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|e| panic!("{e}"))
    }

    /// Load the Windows structure corpus without panicking.
    pub fn try_new() -> Result<Self, WinTypesError> {
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

        Ok(Self { structures })
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
