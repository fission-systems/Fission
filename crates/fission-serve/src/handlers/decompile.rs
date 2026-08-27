use crate::{
    SessionStore,
    types::{DecompileResponse, ErrorResponse},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
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

    let binary = sess.binary().await;
    let name = binary
        .functions
        .iter()
        .find(|f| f.address == addr)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| format!("sub_{addr:x}"));
    // Session-cached: built once from the binary's current function/symbol
    // set (FID/signature-database matching included), not redone on every
    // decompile request against an unchanged binary state.
    let facts = sess.facts().await;

    let result = tokio::task::spawn_blocking(move || {
        let cfg = RustSleighDecompileConfig::cli_defaults();
        decompile_with_rust_sleigh_with_facts(&binary, &facts, addr, &name, &cfg, None, None)
    })
    .await;

    match result {
        // `out.learned_facts` -- whatever this decompile discovered via
        // `DecompContext::record_inferred_type`/`record_discovered_hints`
        // -- is persisted back as the session's current facts, so a later
        // decompile in this session starts from it instead of the plain
        // loader-derived facts every session starts with.
        //
        // A chained-decompile regression was previously suspected here
        // (a trivial function taking 60+s to decompile right after an
        // unrelated one in the same session) and traced -- via
        // `FISSION_PREVIEW_DIAG` showing thousands of per-function
        // `[CFG-DIAG]` lines before the render pipeline was ever reached --
        // to `FactStore::from_binary`'s FID signature-matching loop
        // (`ingest_signature_matches_with_databases`) doing a full,
        // unparallelized CFG-building decode of every unnamed function in
        // the binary. That cost is paid once per session on the first
        // `sess.facts()` call (see below), not per decompile and not by
        // carrying facts forward; parallelizing it (same fix shape as the
        // shared-returns loop in `discover.rs`) brought a real sqlite3.dll
        // from 6+ minutes down to ~49s, and a same-session decompile
        // immediately after -- with facts now carried forward -- measured
        // at 0.08s, confirming no regression from persisting them.
        Ok(Ok(out)) => {
            if let Some(learned_facts) = out.learned_facts.clone() {
                sess.set_facts(learned_facts).await;
            }
            Json(DecompileResponse {
                pseudocode: out.code,
                nir: out.code_nir,
                fell_back: out.fell_back,
                reason: out.fallback_reason,
            })
            .into_response()
        }
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
