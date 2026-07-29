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
use fission_static::analysis::{FunctionDiscoveryProfile, discover_functions_with_runtime};
use std::sync::Arc;
use uuid::Uuid;

/// POST /api/binary — upload a binary, returns a session_id.
pub async fn handle_upload_binary(
    State(store): State<Arc<SessionStore>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Extract the file field from the multipart body
    let mut data: Option<(String, Vec<u8>)> = None;
    if let Ok(Some(field)) = multipart.next_field().await {
        let name = field.file_name().unwrap_or("binary").to_string();
        match field.bytes().await {
            Ok(bytes) => {
                data = Some((name, bytes.to_vec()));
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

    // Parse only (exports/imports/COFF/PDB symbol harvest) on a blocking
    // thread -- fast, no CFG walk. This alone never finds unexported
    // internal functions (e.g. every export in an MSVC-built DLL can be a
    // 5-byte `jmp` thunk to the real, unnamed implementation elsewhere in
    // .text, which the loader has no way to follow), so it's not the final
    // function list -- just enough to respond and open a session with.
    let filename_clone = filename.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<LoadedBinary> {
        Ok(LoadedBinary::from_bytes(bytes, filename_clone)?)
    })
    .await;

    match result {
        Ok(Ok(binary)) => {
            let fn_count = binary.functions.len();
            let summary  = binary.summary().to_string();
            match store.create(binary, filename, true).await {
                Ok(session_id) => {
                    // CFG-based discovery (`fission_cli decomp`'s own
                    // default profile is Conservative; matched here so a
                    // served binary's function list isn't just the
                    // export/import table by itself -- confirmed missing
                    // hundreds of real functions on a real sqlite3.dll
                    // before this was added) can take far longer than
                    // parsing on a large binary. Run it after responding
                    // instead of blocking the upload on it; the session
                    // starts usable with loader-only symbols and
                    // `analyzing` flips false once discovery lands.
                    tokio::spawn(run_discovery_in_background(store.clone(), session_id));

                    (
                        StatusCode::OK,
                        Json(UploadResponse {
                            session_id: session_id.to_string(),
                            fn_count,
                            summary,
                            analyzing: true,
                        }),
                    )
                        .into_response()
                }
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

async fn run_discovery_in_background(store: Arc<SessionStore>, session_id: Uuid) {
    let Some(session) = store.get(&session_id).await else {
        return;
    };
    let mut binary = (*session.binary().await).clone();
    let discovered = tokio::task::spawn_blocking(move || {
        // Balanced matches Ghidra's own function set almost exactly on a
        // real-world test binary (~99% overlap) where Conservative misses
        // a large fraction of real, non-exported functions. It used to be
        // impractically slow (19+ minutes on that same binary, from a
        // handful of accidentally-sequential validation loops in the
        // scanners); those are now fixed, and it runs in the same
        // ballpark as Conservative used to. Discovery already runs here in
        // the background, off the upload response's critical path, so the
        // added cost doesn't block the client either way.
        discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Balanced);
        binary
    })
    .await;
    match discovered {
        Ok(binary) => session.set_binary(binary).await,
        Err(e) => tracing::warn!("background function discovery panicked: {e}"),
    }
    session.set_analyzing(false);
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
