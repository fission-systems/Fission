use crate::{SessionStore, types::{ErrorResponse, FnEntry}};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use std::sync::Arc;
use uuid::Uuid;

/// GET /api/functions/:session — list all functions for the session's binary.
pub async fn handle_list_functions(
    State(store): State<Arc<SessionStore>>,
    Path(session): Path<Uuid>,
) -> impl IntoResponse {
    let Some(sess) = store.get(&session).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("session not found or expired")),
        )
            .into_response();
    };

    let fns: Vec<FnEntry> = sess.binary.functions.iter().map(|f| FnEntry {
        addr:      f.address,
        name:      f.name.clone(),
        is_import: f.is_import,
        is_export: f.is_export,
        is_thunk:  f.is_thunk_like,
        size:      f.size,
    }).collect();

    Json(fns).into_response()
}
