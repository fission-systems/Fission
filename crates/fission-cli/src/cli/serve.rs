//! `fission serve` — local HTTP API server for fission-web.
//!
//! Starts an Axum server on `localhost:<port>` (default 7331) that exposes
//! a REST API consumed by the fission-web Next.js frontend. All computation
//! runs on the user's machine; the web frontend is purely a UI layer.
//!
//! # Endpoints
//!
//! | Method | Path                        | Description                        |
//! |--------|-----------------------------|------------------------------------|
//! | GET    | /api/status                 | Server version + loaded binary name |
//! | POST   | /api/binary                 | Upload binary (multipart/form-data) |
//! | GET    | /api/functions              | List all functions                  |
//! | POST   | /api/decompile/:addr        | Decompile function at hex address   |
//! | GET    | /api/xrefs/:addr            | Callers + callees for address       |

use anyhow::Result;
use axum::{
    Router,
    body::Bytes,
    extract::{Multipart, Path, State},
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
};
use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use fission_static::analysis::xref_index::{
    XrefKind, build_xref_index, resolve_enclosing_function,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct ServerState {
    binary:    Option<Arc<LoadedBinary>>,
    binary_name: Option<String>,
}

type Shared = Arc<RwLock<ServerState>>;

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatusResponse {
    version: &'static str,
    binary:  Option<String>,
    fn_count: usize,
}

#[derive(Serialize)]
struct FnEntry {
    addr:      u64,
    name:      String,
    is_import: bool,
    is_export: bool,
    is_thunk:  bool,
    size:      u64,
}

#[derive(Serialize)]
struct DecompileResponse {
    pseudocode: String,
    nir:        Option<String>,
    fell_back:  bool,
    reason:     Option<String>,
}

#[derive(Serialize)]
struct XrefRow {
    from_addr: u64,
    to_addr:   Option<u64>,
    kind:      String,
    symbol:    Option<String>,
    fn_name:   Option<String>,
}

#[derive(Serialize)]
struct XrefsResponse {
    callers: Vec<XrefRow>,
    callees: Vec<XrefRow>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_status(State(st): State<Shared>) -> Json<StatusResponse> {
    let s = st.read().await;
    let fn_count = s.binary.as_ref().map(|b| b.functions.len()).unwrap_or(0);
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        binary:  s.binary_name.clone(),
        fn_count,
    })
}

async fn handle_upload_binary(
    State(st): State<Shared>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut data: Option<(String, Vec<u8>)> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.file_name().unwrap_or("binary").to_string();
        match field.bytes().await {
            Ok(bytes) => { data = Some((name, bytes.to_vec())); break; }
            Err(e) => {
                return (StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": e.to_string()}))).into_response();
            }
        }
    }
    let Some((filename, bytes)) = data else {
        return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no file uploaded"}))).into_response();
    };

    let result = tokio::task::spawn_blocking(move || {
        let path = std::path::Path::new(&filename);
        fission_loader::loader::load_binary(path, &bytes)
    }).await;

    match result {
        Ok(Ok(binary)) => {
            let fn_count = binary.functions.len();
            let summary  = binary.summary.clone();
            let mut s = st.write().await;
            s.binary_name = Some(filename);
            s.binary      = Some(Arc::new(binary));
            (StatusCode::OK,
             Json(serde_json::json!({"fn_count": fn_count, "summary": summary}))).into_response()
        }
        Ok(Err(e)) => (StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn handle_list_functions(State(st): State<Shared>) -> impl IntoResponse {
    let s = st.read().await;
    let Some(binary) = s.binary.as_ref() else {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no binary loaded"}))).into_response();
    };
    let fns: Vec<FnEntry> = binary.functions.iter().map(|f| FnEntry {
        addr:      f.address,
        name:      f.name.clone(),
        is_import: f.is_import,
        is_export: f.is_export,
        is_thunk:  f.is_thunk_like,
        size:      f.size,
    }).collect();
    Json(fns).into_response()
}

async fn handle_decompile(
    State(st): State<Shared>,
    Path(addr_hex): Path<String>,
) -> impl IntoResponse {
    let addr = match u64::from_str_radix(addr_hex.trim_start_matches("0x"), 16) {
        Ok(a) => a,
        Err(_) => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid address"}))).into_response(),
    };

    let binary = {
        let s = st.read().await;
        match s.binary.clone() {
            Some(b) => b,
            None => return (StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "no binary loaded"}))).into_response(),
        }
    };

    let name = binary.functions.iter()
        .find(|f| f.address == addr)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| format!("sub_{addr:x}"));

    let result = tokio::task::spawn_blocking(move || {
        let facts = FactStore::build(&binary, false);
        let cfg   = RustSleighDecompileConfig::default();
        decompile_with_rust_sleigh_with_facts(&binary, addr, &name, &facts, &cfg)
    }).await;

    match result {
        Ok(Ok(out)) => Json(DecompileResponse {
            pseudocode: out.pseudocode,
            nir:        out.nir,
            fell_back:  out.fell_back,
            reason:     out.fallback_reason,
        }).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn handle_xrefs(
    State(st): State<Shared>,
    Path(addr_hex): Path<String>,
) -> impl IntoResponse {
    let addr = match u64::from_str_radix(addr_hex.trim_start_matches("0x"), 16) {
        Ok(a) => a,
        Err(_) => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid address"}))).into_response(),
    };

    let binary = {
        let s = st.read().await;
        match s.binary.clone() {
            Some(b) => b,
            None => return (StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "no binary loaded"}))).into_response(),
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        let index = build_xref_index(&binary, false);
        let name_map: std::collections::HashMap<u64, String> = binary.functions.iter()
            .map(|f| (f.address, f.name.clone()))
            .collect();

        let callers: Vec<XrefRow> = index.refs_to_address(addr).iter().map(|r| {
            let from = r.source.address;
            let enclosing = resolve_enclosing_function(&binary.functions, from, 512);
            XrefRow {
                from_addr: from,
                to_addr:   Some(r.target.address),
                kind:      format!("{:?}", r.kind),
                symbol:    r.target.symbol.clone(),
                fn_name:   enclosing.and_then(|a| name_map.get(&a).cloned()),
            }
        }).collect();

        let callees: Vec<XrefRow> = index.refs_from_address(addr).iter()
            .filter(|r| matches!(r.kind, XrefKind::Call | XrefKind::Jump | XrefKind::ConditionalJump))
            .map(|r| XrefRow {
                from_addr: r.source.address,
                to_addr:   Some(r.target.address),
                kind:      format!("{:?}", r.kind),
                symbol:    r.target.symbol.clone(),
                fn_name:   r.target.address.and_then(|a| name_map.get(&a).cloned()),
            }).collect();

        (callers, callees)
    }).await;

    match result {
        Ok((callers, callees)) => Json(XrefsResponse { callers, callees }).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run_serve(port: u16) -> Result<()> {
    let state: Shared = Arc::new(RwLock::new(ServerState::default()));

    let cors = CorsLayer::new()
        .allow_origin("http://localhost:3000".parse::<HeaderValue>()?)
        .allow_origin("https://fission-systems.github.io".parse::<HeaderValue>()?)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/api/status",           get(handle_status))
        .route("/api/binary",           post(handle_upload_binary))
        .route("/api/functions",        get(handle_list_functions))
        .route("/api/decompile/:addr",  post(handle_decompile))
        .route("/api/xrefs/:addr",      get(handle_xrefs))
        .with_state(state)
        .layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("fission serve  →  http://localhost:{port}");
    info!("Open fission-web in your browser and connect to this server.");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
