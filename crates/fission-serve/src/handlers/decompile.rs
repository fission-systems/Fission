use crate::{SessionStore, types::{DecompileResponse, ErrorResponse}};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
use fission_static::analysis::decomp::facts::FactStore;
use std::sync::Arc;
use uuid::Uuid;

/// POST /api/decompile/:session/:addr — decompile the function at hex addr.
pub async fn handle_decompile(
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

    let binary = sess.binary.clone();
    let name = binary.functions.iter()
        .find(|f| f.address == addr)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| format!("sub_{addr:x}"));

    let result = tokio::task::spawn_blocking(move || {
        let facts = FactStore::from_binary(&binary);
        let cfg   = RustSleighDecompileConfig::cli_defaults();
        decompile_with_rust_sleigh_with_facts(&binary, &facts, addr, &name, &cfg, None, None)
    })
    .await;

    match result {
        Ok(Ok(out)) => Json(DecompileResponse {
            pseudocode: out.code,
            nir:        out.code_nir,
            fell_back:  out.fell_back,
            reason:     out.fallback_reason,
        })
        .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
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

fn parse_hex(s: &str) -> Result<u64, String> {
    u64::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|_| format!("invalid hex address: {s}"))
}
