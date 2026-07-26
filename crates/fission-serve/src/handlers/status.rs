use crate::{
    SessionStore,
    types::{AnalysisResourceStatus, BackendCapabilities, StatusResponse},
};
use axum::{extract::State, response::Json};
use fission_core::resource_roots::resource_status_snapshot;
use fission_sleigh::compiler::checked_in_compiled_sla_available;
use std::sync::Arc;

pub async fn handle_status(State(store): State<Arc<SessionStore>>) -> Json<StatusResponse> {
    let snapshot = resource_status_snapshot();
    let resources = snapshot.resources;
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        active_sessions: store.count().await,
        max_sessions: store.max_sessions,
        capabilities: BackendCapabilities::local_http(),
        resources: AnalysisResourceStatus {
            sleigh_artifacts: checked_in_compiled_sla_available(),
            signatures: resources.signatures_base.is_some(),
            fid: resources.fid_present || resources.fidb_java_present,
            type_information: resources.win32_typeinfo_present
                || resources.win_api_pipe_text_present
                || resources.generic_clib_pipe_text_present
                || resources.generic_clib_64_pipe_text_present
                || resources.mac_osx_pipe_text_present,
            patterns: resources.patterns_present,
        },
    })
}
