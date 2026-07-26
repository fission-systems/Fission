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
    pub fn_count: usize,
    pub summary: String,
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
