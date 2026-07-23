use crate::{SessionStore, types::StatusResponse};
use axum::{extract::State, response::Json};
use std::sync::Arc;

pub async fn handle_status(State(store): State<Arc<SessionStore>>) -> Json<StatusResponse> {
    Json(StatusResponse {
        version:         env!("CARGO_PKG_VERSION"),
        active_sessions: store.count().await,
        max_sessions:    store.max_sessions,
    })
}
