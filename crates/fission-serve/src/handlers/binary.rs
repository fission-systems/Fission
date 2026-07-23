use crate::{
    SessionStore,
    types::{ErrorResponse, UploadResponse},
};
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use uuid::Uuid;

/// POST /api/binary — upload a binary, returns a session_id.
pub async fn handle_upload_binary(
    State(store): State<Arc<SessionStore>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Extract the file field from the multipart body
    let mut data: Option<(String, Vec<u8>)> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.file_name().unwrap_or("binary").to_string();
        match field.bytes().await {
            Ok(bytes) => {
                data = Some((name, bytes.to_vec()));
                break;
            }
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(e.to_string())),
                )
                    .into_response();
            }
        }
    }

    let Some((filename, bytes)) = data else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("no file uploaded")),
        )
            .into_response();
    };

    // Parse the binary on a blocking thread
    let filename_clone = filename.clone();
    let result = tokio::task::spawn_blocking(move || {
        LoadedBinary::from_bytes(bytes, filename_clone)
    })
    .await;

    match result {
        Ok(Ok(binary)) => {
            let fn_count = binary.functions.len();
            let summary  = binary.summary().to_string();
            match store.create(binary, filename).await {
                Ok(session_id) => (
                    StatusCode::OK,
                    Json(UploadResponse { session_id, fn_count, summary }),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ErrorResponse::new(e)),
                )
                    .into_response(),
            }
        }
        Ok(Err(e)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

/// DELETE /api/session/:session — explicitly release a session.
pub async fn handle_delete_session(
    State(store): State<Arc<SessionStore>>,
    Path(session): Path<Uuid>,
) -> impl IntoResponse {
    if store.remove(&session).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
