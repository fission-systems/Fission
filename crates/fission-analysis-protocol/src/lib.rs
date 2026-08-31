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
pub struct CallGraphNode {
    pub address: u64,
    pub name: String,
    pub caller_count: usize,
    pub callee_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallGraphResponse {
    pub nodes: Vec<CallGraphNode>,
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
}
