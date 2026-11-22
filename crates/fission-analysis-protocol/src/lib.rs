//! Result shapes that `fission_cli` prints as JSON (`list`, `callgraph`,
//! `disasm`).
//!
//! This crate deliberately contains no loader, decompiler, or filesystem
//! dependencies, so the shapes can be shared without pulling in analysis code.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnEntry {
    pub addr: u64,
    pub name: String,
    pub is_import: bool,
    pub is_export: bool,
    pub is_thunk: bool,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calling_convention: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionsResponse {
    pub functions: Vec<FnEntry>,
    /// Always `false` from the CLI, which finishes discovery before printing.
    /// Kept so `list --json` keeps the key it has always carried.
    #[serde(default)]
    pub analyzing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphEdge {
    pub caller_addr: u64,
    pub callee_addr: u64,
    #[serde(default)]
    pub call_site: u64,
    #[serde(default)]
    pub is_indirect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphNode {
    pub address: u64,
    pub name: String,
    pub caller_count: usize,
    pub callee_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphResponse {
    pub nodes: Vec<CallGraphNode>,
    #[serde(default)]
    pub edges: Vec<CallGraphEdge>,
    pub total_call_sites: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisasmRow {
    pub address: u64,
    pub bytes_hex: String,
    pub text: String,
    pub target_addr: Option<u64>,
    /// What the addresses in this instruction name -- an import's symbol, a
    /// function's name, the string at that address. Without it a listing shows
    /// `call 0x140002870` and leaves the reader to look it up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refers_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisasmResponse {
    pub rows: Vec<DisasmRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callgraph_contract_includes_edges_and_roundtrips() {
        let resp = CallGraphResponse {
            nodes: vec![
                CallGraphNode {
                    address: 0x1000,
                    name: "main".to_string(),
                    caller_count: 0,
                    callee_count: 1,
                },
                CallGraphNode {
                    address: 0x2000,
                    name: "helper".to_string(),
                    caller_count: 1,
                    callee_count: 0,
                },
            ],
            edges: vec![CallGraphEdge {
                caller_addr: 0x1000,
                callee_addr: 0x2000,
                call_site: 0x1020,
                is_indirect: false,
            }],
            total_call_sites: 1,
        };

        let json = serde_json::to_string(&resp).expect("serialize callgraph");
        let decoded: CallGraphResponse =
            serde_json::from_str(&json).expect("deserialize callgraph");
        assert_eq!(decoded, resp);
        assert_eq!(decoded.edges.len(), 1);
        assert_eq!(decoded.edges[0].call_site, 0x1020);

        // Backwards compatibility test: edges field omitted in older JSON
        let legacy_json = r#"{"nodes":[],"total_call_sites":0}"#;
        let legacy: CallGraphResponse =
            serde_json::from_str(legacy_json).expect("deserialize legacy callgraph");
        assert!(legacy.edges.is_empty());
    }

    #[test]
    fn fn_entry_includes_signatures_and_provenance() {
        let entry = FnEntry {
            addr: 0x401000,
            name: "calculate".to_string(),
            is_import: false,
            is_export: true,
            is_thunk: false,
            size: 48,
            calling_convention: Some("cdecl".to_string()),
            return_type: Some("int".to_string()),
            parameter_types: vec!["int".to_string(), "char*".to_string()],
            symbol_source: Some("export".to_string()),
        };

        let json = serde_json::to_string(&entry).expect("serialize fn_entry");
        let decoded: FnEntry = serde_json::from_str(&json).expect("deserialize fn_entry");
        assert_eq!(decoded, entry);

        // Backwards compatibility test: legacy JSON without prototype fields
        let legacy_json = r#"{"addr":4198400,"name":"legacy","is_import":false,"is_export":false,"is_thunk":false,"size":16}"#;
        let legacy: FnEntry = serde_json::from_str(legacy_json).expect("deserialize legacy fn");
        assert_eq!(legacy.calling_convention, None);
        assert_eq!(legacy.return_type, None);
        assert!(legacy.parameter_types.is_empty());
    }
}
