use crate::{
    ServeRuntimeInfo, SessionStore,
    types::{ErrorResponse, UploadResponse},
};
use axum::{
    Extension,
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;
use fission_static::analysis::{FunctionDiscoveryProfile, discover_functions_with_runtime};
use std::sync::Arc;
use uuid::Uuid;

/// POST /api/binary — upload a binary, returns a session_id.
pub async fn handle_upload_binary(
    State(store): State<Arc<SessionStore>>,
    Extension(runtime): Extension<ServeRuntimeInfo>,
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
            let summary = binary.summary().to_string();
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
                    tokio::spawn(run_discovery_in_background(
                        store.clone(),
                        session_id,
                        runtime.cloud_mode,
                    ));

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
                Err(e) => {
                    (StatusCode::SERVICE_UNAVAILABLE, Json(ErrorResponse::new(e))).into_response()
                }
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

async fn run_discovery_in_background(store: Arc<SessionStore>, session_id: Uuid, cloud_mode: bool) {
    let Some(session) = store.get(&session_id).await else {
        return;
    };
    let t0 = std::time::Instant::now();
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
    tracing::info!("[PERF] discovery phase: {:?}", t0.elapsed());
    match discovered {
        Ok(binary) => {
            session.set_binary(binary.clone()).await;
            // `analyzing` flips false here, right after discovery -- not
            // after the arity pre-analysis below. On a real ~27k-function
            // binary that pre-analysis (decoding every non-import function
            // once) was observed taking 30+ minutes, which meant the whole
            // session sat in "analyzing..." for that entire time even
            // though discovery itself (the only thing the function list and
            // a plain FID-matched decompile actually depend on) finishes in
            // ~15s. The arity pre-analysis stays valuable -- it's what lets
            // a *first* decompile of a callee already show a widened
            // signature learned from its callers -- but it's a quality
            // enrichment applied to `session.facts` whenever it finishes,
            // not something any UI state should block on.
            session.set_analyzing(false);
            tracing::info!(
                "[PERF] total background phase (pre-arity): {:?}",
                t0.elapsed()
            );

            // Whole-program call-arity pre-analysis: decode every function
            // once now (background, off the request path) so a session's
            // *first-ever* decompile of a callee can already show a widened
            // signature learned from its callers -- rather than requiring
            // the caller to have been decompiled first in this same session
            // (that per-request-only path is `record_interprocedural_arity_facts`,
            // wired separately in `render.rs`). Built here, once, instead of
            // lazily in `SessionData::facts()`, since that path is also on
            // the hot per-request critical path for the *first* decompile
            // call otherwise.
            //
            // Skipped entirely in cloud mode: this decodes every non-import
            // function and builds full HIR for each one, which is dramatically
            // more expensive than the FID-matching pass alone. Observed live
            // on Railway not finishing within a session's whole 30-minute TTL
            // for an ordinary ~6500-function binary (not the earlier-diagnosed
            // 27k-function outlier -- this is now the *typical* case), and
            // while running it saturates the container's rayon pool badly
            // enough to starve that same session's own foreground decompile
            // requests for CPU. `fission_cli`'s oneshot `decomp` never calls
            // this at all, so cloud mode without it now matches that same,
            // already-proven-adequate shape: FID-matched names from the plain
            // `FactStore` (`SessionData::facts()`'s own lazy, now-de-stampeded
            // build) plus whatever `record_interprocedural_arity_facts` learns
            // per-request as callers get decompiled -- just without the
            // upfront whole-program head start. Self-hosted/local `fission_
            // serve` keeps it, since an operator running their own instance
            // controls (and presumably has more of) its CPU budget.
            if !cloud_mode {
                let t1 = std::time::Instant::now();
                let fn_count = binary.functions.len();
                let facts = tokio::task::spawn_blocking(move || {
                    let mut facts = FactStore::from_binary(&binary);
                    let t_fid = t1.elapsed();
                    fission_decompiler::facts::seed_whole_program_call_arity_facts(
                        &binary, &mut facts,
                    );
                    (facts, t_fid)
                })
                .await;
                match facts {
                    Ok((facts, t_fid)) => {
                        tracing::info!(
                            "[PERF] arity phase: {:?} total (fid/facts-build sub-portion: {:?}), fn_count={fn_count}",
                            t1.elapsed(),
                            t_fid
                        );
                        session.set_facts(facts).await;
                    }
                    Err(e) => tracing::warn!("background arity pre-analysis panicked: {e}"),
                }
            } else {
                // Cloud mode skips the arity phase above, but a session's
                // *first* decompile still has to build `session.facts()`
                // (FID matching) somewhere -- doing that lazily on the
                // first decompile request works, but means that request
                // pays the full cost synchronously. Warming it here
                // instead means a session sitting idle for even a few
                // seconds after upload (reading the function list, picking
                // what to click) already has it ready by the time a
                // decompile request lands. Safe to just call `facts()` and
                // let it race with a real decompile request: `SessionData::
                // facts()`'s lock is now held across the whole build, not
                // just the check, so a concurrent caller awaits this same
                // in-flight build instead of redundantly starting its own.
                let t1 = std::time::Instant::now();
                session.facts().await;
                tracing::info!("[PERF] fid-warmup phase: {:?}", t1.elapsed());
            }
        }
        Err(e) => {
            tracing::warn!("background function discovery panicked: {e}");
            session.set_analyzing(false);
        }
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
