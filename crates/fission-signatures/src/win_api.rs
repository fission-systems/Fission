//! Windows API Function Signatures
//!
//! Contains type information for common Windows API functions
//! to improve decompiler output quality.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Global lazily-initialized Windows API database for efficient reuse.
/// This avoids recreating the database with 100+ signatures on each use.
pub static WIN_API_DB: LazyLock<WinApiDatabase> = LazyLock::new(WinApiDatabase::new);

/// Parameter type information with optional enum group for context-aware constant resolution
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub type_name: String,
    /// Optional enum group for context-aware constant substitution
    /// e.g., "PAGE_PROTECT" for VirtualAlloc's flProtect parameter
    pub enum_group: Option<String>,
}

/// Function signature with parameter and return types
#[derive(Debug, Clone)]
pub struct ApiSignature {
    pub name: String,
    pub return_type: String,
    pub params: Vec<ParamInfo>,
}

/// Windows API Signature Database
pub struct WinApiDatabase {
    signatures: HashMap<String, ApiSignature>,
}

#[derive(Debug, Deserialize)]
struct JsonParamInfo {
    name: String,
    type_name: String,
    #[serde(default)]
    enum_group: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonApiSignature {
    name: String,
    return_type: String,
    params: Vec<JsonParamInfo>,
}

impl WinApiDatabase {
    /// Create a new WinApiDatabase with all built-in signatures
    ///
    /// Performance: Pre-allocates HashMap capacity based on known API count
    /// to avoid rehashing during loading (~130 APIs across all DLLs)
    pub fn new() -> Self {
        let mut db = Self {
            // Pre-allocate for ~130 known APIs to minimize HashMap rehashing
            signatures: HashMap::with_capacity(140),
        };
        db.load_kernel32();
        db.load_user32();
        db.load_ntdll();
        db.load_advapi32();
        db.load_ws2_32();
        db.load_winhttp();
        db.load_wininet();
        db.load_shell32();
        db.load_bcrypt();
        db
    }

    fn add(&mut self, sig: ApiSignature) {
        self.signatures.insert(sig.name.clone(), sig);
    }

    fn load_from_json_str(&mut self, json_str: &str, source: &str) {
        let signatures: Vec<JsonApiSignature> = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("failed to parse win_api JSON '{}': {}", source, e));

        for sig in signatures {
            let params = sig
                .params
                .into_iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_name: p.type_name,
                    enum_group: p.enum_group,
                })
                .collect();

            self.add(ApiSignature {
                name: sig.name,
                return_type: sig.return_type,
                params,
            });
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ApiSignature> {
        self.signatures.values()
    }

    fn load_kernel32(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/kernel32.json"),
            "data/win_api/kernel32.json",
        );
    }

    fn load_user32(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/user32.json"),
            "data/win_api/user32.json",
        );
    }

    fn load_ntdll(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/ntdll.json"),
            "data/win_api/ntdll.json",
        );
    }

    fn load_advapi32(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/advapi32.json"),
            "data/win_api/advapi32.json",
        );
    }

    fn load_ws2_32(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/ws2_32.json"),
            "data/win_api/ws2_32.json",
        );
    }

    fn load_winhttp(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/winhttp.json"),
            "data/win_api/winhttp.json",
        );
    }

    fn load_wininet(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/wininet.json"),
            "data/win_api/wininet.json",
        );
    }

    fn load_shell32(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/shell32.json"),
            "data/win_api/shell32.json",
        );
    }

    fn load_bcrypt(&mut self) {
        self.load_from_json_str(
            include_str!("../data/win_api/bcrypt.json"),
            "data/win_api/bcrypt.json",
        );
    }

    /// Look up a function signature by name
    pub fn get(&self, name: &str) -> Option<&ApiSignature> {
        self.signatures.get(name)
    }

    /// Get all signatures
    pub fn all(&self) -> &HashMap<String, ApiSignature> {
        &self.signatures
    }
}

impl Default for WinApiDatabase {
    fn default() -> Self {
        Self::new()
    }
}
