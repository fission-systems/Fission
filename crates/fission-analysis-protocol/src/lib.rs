//! Target-independent contracts shared by Fission analysis backends and clients.
//!
//! This crate deliberately contains no loader, decompiler, HTTP, filesystem, or
//! browser dependencies. Native processes, local HTTP services, and future
//! browser workers can therefore expose the same session and result shapes.

use serde::{Deserialize, Serialize};

/// Execution boundary that owns the analysis session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisBackendKind {
    NativeProcess,
    LocalHttp,
    CloudHttp,
    BrowserWorker,
}

/// How an analysis backend obtains SLEIGH and signature resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccessMode {
    /// The backend resolves resources from its host filesystem.
    HostFilesystem,
    /// The browser user explicitly selected a resource bundle.
    BrowserSelectedBundle,
    /// Versioned artifacts are packaged with or fetched by the application.
    PackagedArtifacts,
}

/// Stable capability description for selecting an analysis backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub backend: AnalysisBackendKind,
    pub resource_access: ResourceAccessMode,
    pub accepts_binary_upload: bool,
    pub executes_on_client: bool,
    pub supports_user_resource_root: bool,
    pub requires_authentication: bool,
}

impl BackendCapabilities {
    #[must_use]
    pub const fn local_http() -> Self {
        Self {
            backend: AnalysisBackendKind::LocalHttp,
            resource_access: ResourceAccessMode::HostFilesystem,
            accepts_binary_upload: true,
            executes_on_client: false,
            supports_user_resource_root: true,
            requires_authentication: false,
        }
    }

    #[must_use]
    pub const fn cloud_http() -> Self {
        Self {
            backend: AnalysisBackendKind::CloudHttp,
            resource_access: ResourceAccessMode::PackagedArtifacts,
            accepts_binary_upload: true,
            executes_on_client: false,
            supports_user_resource_root: false,
            requires_authentication: true,
        }
    }
}

/// Availability only; local filesystem paths are intentionally not put on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisResourceStatus {
    pub sleigh_artifacts: bool,
    pub signatures: bool,
    pub fid: bool,
    pub type_information: bool,
    pub patterns: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub version: String,
    pub active_sessions: usize,
    pub max_sessions: usize,
    pub capabilities: BackendCapabilities,
    pub resources: AnalysisResourceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResponse {
    /// Opaque session token; include it in all subsequent requests.
    pub session_id: String,
    /// Functions known at response time (loader symbols only). CFG-based
    /// discovery runs in the background after this response is sent --
    /// see `analyzing`.
    pub fn_count: usize,
    pub summary: String,
    /// `true` while CFG-based function discovery is still running in the
    /// background. `GET /api/functions/:session` returns the same flag;
    /// poll it until it flips to `false` to get the full function list
    /// (discovery on a large binary can take significantly longer than
    /// the upload itself, so the response isn't held open for it).
    #[serde(default)]
    pub analyzing: bool,
}

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
    /// See `UploadResponse::analyzing`.
    #[serde(default)]
    pub analyzing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecompileResponse {
    pub pseudocode: String,
    pub nir: Option<String>,
    pub fell_back: bool,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_map: Vec<(u32, u64)>, // (line_number, instruction_address)
}

impl DecompileResponse {
    #[must_use]
    pub fn success(pseudocode: impl Into<String>) -> Self {
        Self {
            pseudocode: pseudocode.into(),
            nir: None,
            fell_back: false,
            reason: None,
            warnings: Vec::new(),
            duration_ms: 0,
            line_map: Vec::new(),
        }
    }

    #[must_use]
    pub fn fallback(pseudocode: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pseudocode: pseudocode.into(),
            nir: None,
            fell_back: true,
            reason: Some(reason.into()),
            warnings: Vec::new(),
            duration_ms: 0,
            line_map: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefRow {
    pub from_addr: u64,
    pub to_addr: Option<u64>,
    pub kind: String,
    pub symbol: Option<String>,
    pub fn_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefsResponse {
    pub callers: Vec<XrefRow>,
    pub callees: Vec<XrefRow>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    #[must_use]
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_contract_round_trips_across_backend_boundary() {
        let status = StatusResponse {
            version: "0.1.0".to_string(),
            active_sessions: 2,
            max_sessions: 8,
            capabilities: BackendCapabilities::local_http(),
            resources: AnalysisResourceStatus {
                sleigh_artifacts: true,
                signatures: true,
                fid: false,
                type_information: true,
                patterns: false,
            },
        };

        let json = serde_json::to_string(&status).expect("serialize status");
        let decoded: StatusResponse = serde_json::from_str(&json).expect("deserialize status");
        assert_eq!(decoded, status);
        assert!(json.contains("\"resource_access\":\"host_filesystem\""));
    }

    #[test]
    fn upload_session_id_remains_opaque() {
        let json = r#"{"session_id":"worker:42","fn_count":3,"summary":"ELF"}"#;
        let response: UploadResponse = serde_json::from_str(json).expect("deserialize upload");
        assert_eq!(response.session_id, "worker:42");
    }

    #[test]
    fn cloud_capabilities_require_auth_and_packaged_resources() {
        let capabilities = BackendCapabilities::cloud_http();
        assert_eq!(capabilities.backend, AnalysisBackendKind::CloudHttp);
        assert_eq!(
            capabilities.resource_access,
            ResourceAccessMode::PackagedArtifacts
        );
        assert!(capabilities.requires_authentication);
        assert!(!capabilities.supports_user_resource_root);
    }

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

    #[test]
    fn decompile_response_roundtrips_with_diagnostics() {
        let resp = DecompileResponse {
            pseudocode: "void main() {}".to_string(),
            nir: Some("NIR { ... }".to_string()),
            fell_back: false,
            reason: None,
            warnings: vec!["indirect branch unmodeled".to_string()],
            duration_ms: 42,
            line_map: vec![(1, 0x401000), (2, 0x401008)],
        };

        let json = serde_json::to_string(&resp).expect("serialize decompile");
        let decoded: DecompileResponse =
            serde_json::from_str(&json).expect("deserialize decompile");
        assert_eq!(decoded, resp);
        assert_eq!(decoded.duration_ms, 42);
        assert_eq!(decoded.warnings.len(), 1);
        assert_eq!(decoded.line_map.len(), 2);

        // Backwards compatibility
        let legacy_json = r#"{"pseudocode":"int x;","nir":null,"fell_back":false,"reason":null}"#;
        let legacy: DecompileResponse =
            serde_json::from_str(legacy_json).expect("deserialize legacy decompile");
        assert_eq!(legacy.duration_ms, 0);
        assert!(legacy.warnings.is_empty());
        assert!(legacy.line_map.is_empty());
    }
}
