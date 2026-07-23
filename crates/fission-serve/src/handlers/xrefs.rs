use crate::{SessionStore, types::{ErrorResponse, XrefRow, XrefsResponse}};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use fission_static::analysis::xref_index::{
    XrefKind, build_xref_index, resolve_enclosing_function,
};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

/// GET /api/xrefs/:session/:addr — callers and callees for the function at addr.
pub async fn handle_xrefs(
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
    let result = tokio::task::spawn_blocking(move || {
        let index = build_xref_index(&binary, false);
        let name_map: HashMap<u64, String> = binary.functions.iter()
            .map(|f| (f.address, f.name.clone()))
            .collect();

        let callers: Vec<XrefRow> = index.refs_to_address(addr).iter().map(|r| {
            let from = r.source.address;
            let enc  = resolve_enclosing_function(&binary.functions, from, 512);
            XrefRow {
                from_addr: from,
                to_addr:   r.target.address,
                kind:      format!("{:?}", r.kind),
                symbol:    r.target.symbol.clone(),
                fn_name:   enc.and_then(|a| name_map.get(&a).cloned()),
            }
        }).collect();

        let callees: Vec<XrefRow> = index.refs_from_address(addr).iter()
            .filter(|r| matches!(r.kind,
                XrefKind::Call | XrefKind::Jump | XrefKind::ConditionalJump))
            .map(|r| XrefRow {
                from_addr: r.source.address,
                to_addr:   r.target.address,
                kind:      format!("{:?}", r.kind),
                symbol:    r.target.symbol.clone(),
                fn_name:   r.target.address.and_then(|a| name_map.get(&a).cloned()),
            })
            .collect();

        (callers, callees)
    })
    .await;

    match result {
        Ok((callers, callees)) => Json(XrefsResponse { callers, callees }).into_response(),
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
