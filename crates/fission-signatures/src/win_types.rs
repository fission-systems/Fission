//! Windows Data Types and Structures
//!
//! Common Windows API structures for type annotation in decompiled code.
//! Based on Windows SDK headers and ghidra-data community definitions.
//! Data is loaded from JSON files at compile time via `include_str!`.

use std::collections::HashMap;

use serde::Deserialize;

// ============================================================================
// Windows Base Types (for annotation purposes)
// ============================================================================

/// Windows base type sizes
pub mod base_types {
    use serde::Deserialize;

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

    /// Load all base types from compiled-in JSON data.
    pub fn all() -> Vec<TypeInfo> {
        let json_str = include_str!("../data/win_types/base_types.json");
        let items: Vec<JsonTypeInfo> = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!(
                "Failed to parse base_types.json - this is compile-time embedded data, check syntax in data/win_types/base_types.json: {}",
                e
            ));
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
        let json_str = include_str!("../data/win_types/structures.json");
        let items: Vec<JsonStructDef> = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!(
                "Failed to parse structures.json - this is compile-time embedded data, check syntax in data/win_types/structures.json: {}",
                e
            ));

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
