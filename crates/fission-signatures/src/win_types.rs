//! Windows Data Types and Structures
//!
//! Common Windows API structures for type annotation in decompiled code.
//! Based on Windows SDK headers and ghidra-data community definitions.
//! Canonical JSON lives under the resolved signatures corpus (`ResourceProvider` / workspace layout).
//! (`base_types.json`, `structures.json`), loaded via [`fission_core::resources::ResourceProvider`].

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use fission_core::resources::ResourceProvider;
use serde::Deserialize;

fn resources() -> ResourceProvider {
    ResourceProvider::global()
}

fn win32_typeinfo_json_path(filename: &str) -> PathBuf {
    resources()
        .win32_typeinfo_json_path(filename)
        .unwrap_or_else(|| {
            panic!(
                "fission-signatures win_types: missing typeinfo JSON `{filename}` (configure bundle/workspace signatures or PATHS.gdt_dir)"
            )
        })
}

fn read_typeinfo_json(filename: &str) -> String {
    let path = win32_typeinfo_json_path(filename);
    if !path.exists() {
        panic!(
            "fission-signatures win_types: missing canonical data file {} (under resolved Win32 typeinfo dir)",
            path.display()
        );
    }
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "fission-signatures win_types: failed to read {}: {e}",
            path.display()
        )
    })
}

// ============================================================================
// Windows Base Types (for annotation purposes)
// ============================================================================

/// Windows base type sizes
pub mod base_types {
    use serde::Deserialize;

    use super::read_typeinfo_json;

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
        let json_str = read_typeinfo_json("base_types.json");
        let items: Vec<JsonTypeInfo> = serde_json::from_str(&json_str).unwrap_or_else(|e| {
            let path = super::win32_typeinfo_json_path("base_types.json");
            panic!(
                "Failed to parse base_types.json at {}: {e}",
                path.display()
            )
        });
        items
            .into_iter()
            .map(|j| TypeInfo {
                name: j.name,
                size_32: j.size_32,
                size_64: j.size_64,
                is_pointer: j.is_pointer,
                is_signed: j.is_signed,
            })
            .collect()
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
        let json_str = read_typeinfo_json("structures.json");
        let items: Vec<JsonStructDef> = serde_json::from_str(&json_str).unwrap_or_else(|e| {
            let path = win32_typeinfo_json_path("structures.json");
            panic!(
                "Failed to parse structures.json at {}: {e}",
                path.display()
            )
        });

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

        Self { structures }
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
