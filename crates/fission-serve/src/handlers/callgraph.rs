use crate::{
    SessionStore,
    types::{CallGraphNode, CallGraphResponse, ErrorResponse},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_static::analysis::CallGraph;
use std::sync::Arc;
use uuid::Uuid;

/// GET /api/callgraph/:session — whole-binary caller/callee counts per
/// function, for the sidebar's Call Graph browser. Unlike xrefs (scoped to
/// one function), this returns every function at once; the client sorts
/// and filters client-side rather than re-fetching per sort change.
pub async fn handle_callgraph(
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

    let binary = sess.binary().await;
    // `xref_index()` is already session-cached (built once, on whichever
    // request needs it first -- xrefs or this) -- aggregating it into a
    // call graph is a cheap in-memory pass, not another disassembly sweep,
    // so no separate cache is needed for that step.
    let index = sess.xref_index().await;
    let result = tokio::task::spawn_blocking(move || {
        let graph = CallGraph::build_from_xref_index(&binary.functions, &index, 0x40);
        let nodes: Vec<CallGraphNode> = binary
            .functions
            .iter()
            .filter(|f| !f.is_import)
            .map(|f| CallGraphNode {
                address: f.address,
                name: f.name.clone(),
                caller_count: graph.callers_of(f.address).len(),
                callee_count: graph.callees_of(f.address).len(),
            })
            .collect();
        (nodes, graph.total_call_sites())
    })
    .await;

    match result {
        Ok((nodes, total_call_sites)) => Json(CallGraphResponse {
            nodes,
            total_call_sites,
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
            .into_response(),
    }
}
