use crate::{
    SessionStore,
    types::{DisasmResponse, DisasmRow, ErrorResponse},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use std::sync::Arc;
use uuid::Uuid;

/// GET /api/disasm/:session/:addr — raw disassembly of the function at addr.
pub async fn handle_disasm(
    State(store): State<Arc<SessionStore>>,
    Path((session, addr_hex)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let addr = match parse_hex(&addr_hex) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(e))).into_response(),
    };

    let Some(sess) = store.get(&session).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("session not found or expired")),
        )
            .into_response();
    };

    let binary = sess.binary().await;
    let result = tokio::task::spawn_blocking(move || {
        fission_decompiler::disasm::disassemble_function(&binary, addr)
    })
    .await;

    match result {
        Ok(Ok(rows)) => {
            let rows = rows
                .into_iter()
                .map(|row| DisasmRow {
                    address: row.address,
                    bytes_hex: row.bytes_hex,
                    text: row.text,
                    target_addr: row.target_addr,
                    refers_to: row.refers_to,
                })
                .collect();
            Json(DisasmResponse { rows }).into_response()
        }
        Ok(Err(e)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse::new(e)),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}

fn parse_hex(s: &str) -> Result<u64, String> {
    u64::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|_| format!("invalid hex address: {s}"))
}
