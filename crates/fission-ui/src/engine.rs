//! Blocking helpers (native) and HTTP fetch helpers (WASM) for GUI binary analysis.
//!
//! # Architecture
//!
//! - **Native (not wasm32)**: all computation runs locally in `spawn_blocking`.
//! - **WASM (web)**: analysis is delegated to a local `fission serve` HTTP server.
//!   The engine calls the REST API (`/api/binary`, `/api/decompile/:addr`, etc.)
//!   and deserialises the JSON response into the shared GUI types.
//!
//! The async wrappers (`run_load`, `run_decompile`, `run_xrefs`) have identical
//! signatures on both platforms — callers (sidebar, dropzone) need no changes.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::sync::Arc;

// ── Shared types ─────────────────────────────────────────────────────────────

/// Result of loading a binary.
/// `binary` is `Some` on native, `None` on WASM (server holds the binary).
/// `session_id` is `Some` on WASM (identifies the server-side session).
pub struct LoadResult {
    pub binary:     Option<Arc<LoadedBinary>>,
    pub functions:  Vec<FunctionInfo>,
    pub summary:    String,
    pub session_id: Option<String>,
}

/// CFG edge classification for the GUI renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgEdgeKind {
    Unconditional,
    ConditionalTrue,
    ConditionalFalse,
    Fallthrough,
    Return,
    Indirect,
}

impl CfgEdgeKind {
    pub fn svg_color(&self) -> &'static str {
        match self {
            Self::ConditionalTrue  => "#4ec97b",
            Self::ConditionalFalse => "#f47067",
            Self::Unconditional    => "#8d97a5",
            Self::Fallthrough      => "#6b7785",
            Self::Return           => "#c099ff",
            Self::Indirect         => "#ffb347",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::ConditionalTrue  => "T",
            Self::ConditionalFalse => "F",
            Self::Return           => "ret",
            Self::Indirect         => "ind",
            _                      => "",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CfgNodeData {
    pub index:    usize,
    pub address:  u64,
    pub op_count: usize,
    pub is_entry: bool,
    pub is_exit:  bool,
}

impl CfgNodeData {
    pub fn label(&self) -> String { format!("0x{:x}", self.address) }
    pub fn node_height(&self) -> f64 {
        (36.0 + (self.op_count as f64).min(8.0) * 4.5).min(72.0)
    }
}

#[derive(Debug, Clone)]
pub struct CfgEdgeData {
    pub from:    usize,
    pub to:      usize,
    pub kind:    CfgEdgeKind,
    pub is_back: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CfgGraphData {
    pub nodes:       Vec<CfgNodeData>,
    pub edges:       Vec<CfgEdgeData>,
    pub cyclomatic:  usize,
    pub block_count: usize,
    pub edge_count:  usize,
}

/// Build CfgGraphData from native decompile evidence (native only).
#[cfg(not(target_arch = "wasm32"))]
impl CfgGraphData {
    pub fn from_evidence(
        evidence: &fission_decompiler::RustSleighPipelineEvidence,
    ) -> Option<Self> {
        let blocks = &evidence.raw_pcode_blocks;
        if blocks.is_empty() { return None; }
        let n = blocks.len();

        let mut visited  = vec![false; n];
        let mut in_stack = vec![false; n];
        let mut back_set = std::collections::HashSet::new();

        let adj: Vec<Vec<usize>> = blocks.iter().map(|b| {
            b.successors.iter().map(|&s| s as usize).filter(|&s| s < n).collect()
        }).collect();

        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        visited[0]  = true;
        in_stack[0] = true;

        while let Some((u, ci)) = stack.last_mut() {
            let u = *u;
            if *ci < adj[u].len() {
                let v = adj[u][*ci]; *ci += 1;
                if in_stack[v] {
                    back_set.insert((u, v));
                } else if !visited[v] {
                    visited[v]  = true;
                    in_stack[v] = true;
                    stack.push((v, 0));
                }
            } else {
                in_stack[u] = false;
                stack.pop();
            }
        }

        let nodes: Vec<CfgNodeData> = blocks.iter().enumerate().map(|(i, b)| {
            let is_exit = b.terminal_opcode.as_deref()
                .map_or(false, |op| op.contains("Return") || op.contains("BranchInd"));
            CfgNodeData { index: i, address: b.start_address, op_count: b.op_count,
                          is_entry: i == 0, is_exit }
        }).collect();

        let mut edges = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            let term       = block.terminal_opcode.as_deref();
            let is_cbranch = term.map_or(false, |t| t.contains("CBranch"));
            let is_branch  = term.map_or(false, |t| t.contains("Branch") && !t.contains("CBranch") && !t.contains("BranchInd"));
            let is_ret     = term.map_or(false, |t| t.contains("Return"));
            let is_ind     = term.map_or(false, |t| t.contains("BranchInd"));
            if is_ret { continue; }
            for (si, &raw_succ) in block.successors.iter().enumerate() {
                let to = raw_succ as usize;
                if to >= n { continue; }
                let kind = if is_cbranch {
                    if si == 0 { CfgEdgeKind::ConditionalTrue } else { CfgEdgeKind::ConditionalFalse }
                } else if is_branch { CfgEdgeKind::Unconditional
                } else if is_ind   { CfgEdgeKind::Indirect
                } else             { CfgEdgeKind::Fallthrough };
                edges.push(CfgEdgeData { from: i, to, kind, is_back: back_set.contains(&(i, to)) });
            }
        }
        let e = edges.len();
        Some(CfgGraphData { nodes, edges, cyclomatic: e.saturating_sub(n) + 2,
                            block_count: n, edge_count: e })
    }
}

/// Decompile output returned by both native and WASM paths.
pub struct DecompileOutput {
    pub code:            String,
    pub code_nir:        Option<String>,
    pub fell_back:       bool,
    pub fallback_reason: Option<String>,
    pub cfg:             Option<CfgGraphData>,
}

/// Lightweight xref row shared by both platforms.
#[derive(Debug, Clone)]
pub struct XrefRow {
    pub from_addr: u64,
    pub to_addr:   Option<u64>,
    pub kind:      String,
    pub symbol:    Option<String>,
    pub fn_name:   Option<String>,
}

/// Minimal batch result (native only, used by Analyse All worker).
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub addr:      u64,
    pub name:      String,
    pub ok:        bool,
    pub fell_back: bool,
    pub error:     Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// NATIVE — blocking helpers + async wrappers using spawn_blocking
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
    use fission_static::analysis::decomp::facts::FactStore;
    use fission_static::analysis::xref_index::{
        XrefKind, build_xref_index, resolve_enclosing_function,
    };

    pub fn load_binary_from_bytes_blocking(data: Vec<u8>, name: &str) -> Result<LoadResult, String> {
        let binary = LoadedBinary::from_bytes(data, name.to_string())
            .map_err(|e| format!("Load failed: {e}"))?;
        let mut functions = binary.functions.clone();
        functions.sort_by_key(|f| f.address);
        let summary = format!(
            "{} | {} | {} functions | entry 0x{:x}",
            binary.format,
            if binary.is_64bit { "64-bit" } else { "32-bit" },
            functions.len(),
            binary.entry_point,
        );
        Ok(LoadResult { binary: Some(Arc::new(binary)), functions, summary, session_id: None })
    }

    pub fn decompile_blocking(
        binary: &Arc<LoadedBinary>, addr: u64, name: &str,
    ) -> Result<DecompileOutput, String> {
        let facts = FactStore::from_binary(binary.as_ref());
        let mut config = RustSleighDecompileConfig::cli_defaults();
        config.nir_timeout_ms = Some(10_000);
        let result = decompile_with_rust_sleigh_with_facts(
            binary.as_ref(), &facts, addr, name, &config, None, None,
        ).map_err(|e| e.to_string())?;
        let cfg = CfgGraphData::from_evidence(&result.evidence);
        Ok(DecompileOutput {
            code: result.code, code_nir: result.code_nir,
            fell_back: result.fell_back, fallback_reason: result.fallback_reason, cfg,
        })
    }

    pub fn xrefs_for_function_blocking(
        binary: &Arc<LoadedBinary>, fn_addr: u64,
    ) -> (Vec<XrefRow>, Vec<XrefRow>) {
        let index    = build_xref_index(binary.as_ref(), false);
        let name_map: std::collections::HashMap<u64, String> =
            binary.functions.iter().map(|f| (f.address, f.name.clone())).collect();

        let callers = index.refs_to_address(fn_addr).iter().map(|r| {
            let from = r.source.address;
            let enc  = resolve_enclosing_function(&binary.functions, from, 512);
            XrefRow { from_addr: from, to_addr: r.target.address,
                      kind: format!("{:?}", r.kind), symbol: r.target.symbol.clone(),
                      fn_name: enc.and_then(|a| name_map.get(&a).cloned()) }
        }).collect();

        let callees = index.refs_from_address(fn_addr).iter()
            .filter(|r| matches!(r.kind, XrefKind::Call | XrefKind::Jump | XrefKind::ConditionalJump))
            .map(|r| XrefRow {
                from_addr: r.source.address, to_addr: r.target.address,
                kind: format!("{:?}", r.kind), symbol: r.target.symbol.clone(),
                fn_name: r.target.address.and_then(|a| name_map.get(&a).cloned()),
            }).collect();

        (callers, callees)
    }

    pub fn batch_decompile_one(binary: &Arc<LoadedBinary>, addr: u64, name: &str) -> BatchResult {
        match decompile_blocking(binary, addr, name) {
            Ok(out) => BatchResult { addr, name: name.to_string(), ok: true,
                                     fell_back: out.fell_back, error: out.fallback_reason },
            Err(e)  => BatchResult { addr, name: name.to_string(), ok: false,
                                     fell_back: false, error: Some(e) },
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    batch_decompile_one, decompile_blocking, load_binary_from_bytes_blocking,
    xrefs_for_function_blocking,
};

// ─────────────────────────────────────────────────────────────────────────────
// WASM — server URL state + HTTP fetch helpers
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_api {
    use super::*;
    use gloo_net::http::Request;
    use serde::Deserialize;

    // Thread-local server URL (configurable from UI)
    thread_local! {
        static SERVER_URL: std::cell::RefCell<String> =
            std::cell::RefCell::new("http://localhost:7331".to_string());
    }

    pub fn set_server_url(url: String) {
        SERVER_URL.with(|u| *u.borrow_mut() = url);
    }

    pub fn get_server_url() -> String {
        SERVER_URL.with(|u| u.borrow().clone())
    }

    // ── Wire types (mirror serve.rs) ──────────────────────────────────────

    #[derive(Deserialize)]
    struct ApiFnEntry {
        addr:      u64,
        name:      String,
        is_import: bool,
        is_export: bool,
        is_thunk:  bool,
        size:      u64,
    }

    #[derive(Deserialize)]
    struct ApiUploadResponse {
        session_id: String,   // UUID from fission-serve
        fn_count:   usize,
        summary:    String,
    }

    #[derive(Deserialize)]
    struct ApiDecompileResponse {
        pseudocode: String,
        nir:        Option<String>,
        fell_back:  bool,
        reason:     Option<String>,
    }

    #[derive(Deserialize)]
    struct ApiXrefRow {
        from_addr: u64,
        to_addr:   Option<u64>,
        kind:      String,
        symbol:    Option<String>,
        fn_name:   Option<String>,
    }

    #[derive(Deserialize)]
    struct ApiXrefsResponse {
        callers: Vec<ApiXrefRow>,
        callees: Vec<ApiXrefRow>,
    }

    fn to_xref_row(r: ApiXrefRow) -> XrefRow {
        XrefRow { from_addr: r.from_addr, to_addr: r.to_addr,
                  kind: r.kind, symbol: r.symbol, fn_name: r.fn_name }
    }

    // ── WASM async helpers ────────────────────────────────────────────────

    pub async fn run_load(data: Vec<u8>, name: String) -> Result<LoadResult, String> {
        let base = get_server_url();

        // Multipart upload using raw fetch (gloo-net doesn't support multipart form yet)
        let form = web_sys::FormData::new().map_err(|e| format!("{e:?}"))?;
        let array = js_sys::Uint8Array::from(data.as_slice());
        let blob  = web_sys::Blob::new_with_u8_array_sequence(&js_sys::Array::of1(&array.into()))
            .map_err(|e| format!("{e:?}"))?;
        let file  = web_sys::File::new_with_blob_sequence_and_options(
            &js_sys::Array::of1(&blob.into()),
            &name,
            &web_sys::FilePropertyBag::new(),
        ).map_err(|e| format!("{e:?}"))?;
        form.append_with_blob_and_filename("file", &file, &name).map_err(|e| format!("{e:?}"))?;

        let resp = Request::post(&format!("{base}/api/binary"))
            .body(form)
            .map_err(|e| format!("Upload form body error: {e:?}"))?
            .send()
            .await
            .map_err(|e| format!("Upload failed: {e:?}"))?;

        if !resp.ok() {
            return Err(format!("Server error {}: upload", resp.status()));
        }
        let upload: ApiUploadResponse = resp.json().await
            .map_err(|e| format!("Parse upload response: {e:?}"))?;

        // Fetch function list scoped to this session
        let fn_resp = Request::get(&format!("{base}/api/functions/{}", upload.session_id))
            .send().await
            .map_err(|e| format!("Functions fetch failed: {e:?}"))?;

        let api_fns: Vec<ApiFnEntry> = fn_resp.json().await
            .map_err(|e| format!("Parse functions: {e:?}"))?;

        let functions: Vec<FunctionInfo> = api_fns.into_iter().map(|f| FunctionInfo {
            name:          f.name,
            address:       f.addr,
            size:          f.size,
            is_import:     f.is_import,
            is_export:     f.is_export,
            is_thunk_like: f.is_thunk,
            ..Default::default()
        }).collect();

        Ok(LoadResult {
            binary:     None,   // server holds the binary
            functions,
            summary:    upload.summary,
            session_id: Some(upload.session_id),
        })
    }

    pub async fn run_decompile(
        _binary: Option<Arc<LoadedBinary>>,   // unused on WASM
        session_id: Option<String>,
        addr: u64,
        _name: String,
    ) -> Result<DecompileOutput, String> {
        let base = get_server_url();
        let sid = session_id.ok_or("no active session — upload a binary first")?;
        let resp = Request::post(&format!("{base}/api/decompile/{sid}/{addr:x}"))
            .send().await
            .map_err(|e| format!("Decompile request failed: {e}"))?;
        if !resp.ok() {
            return Err(format!("Server error {}: decompile", resp.status()));
        }
        let out: ApiDecompileResponse = resp.json().await
            .map_err(|e| format!("Parse decompile response: {e}"))?;
        Ok(DecompileOutput {
            code: out.pseudocode, code_nir: out.nir,
            fell_back: out.fell_back, fallback_reason: out.reason,
            cfg: None,  // CFG not yet serialised over API
        })
    }

    pub async fn run_xrefs(
        _binary: Option<Arc<LoadedBinary>>,
        session_id: Option<String>,
        fn_addr: u64,
    ) -> (Vec<XrefRow>, Vec<XrefRow>) {
        let base = get_server_url();
        let Some(sid) = session_id else { return (vec![], vec![]); };
        let resp = Request::get(&format!("{base}/api/xrefs/{sid}/{fn_addr:x}"))
            .send().await;
        match resp {
            Ok(r) if r.ok() => {
                if let Ok(x) = r.json::<ApiXrefsResponse>().await {
                    return (
                        x.callers.into_iter().map(to_xref_row).collect(),
                        x.callees.into_iter().map(to_xref_row).collect(),
                    );
                }
            }
            _ => {}
        }
        (vec![], vec![])
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_api::{get_server_url, set_server_url};

// ─────────────────────────────────────────────────────────────────────────────
// Unified async wrappers (same signature on both platforms)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_load(data: Vec<u8>, name: String) -> Result<LoadResult, String> {
    tokio::task::spawn_blocking(move || load_binary_from_bytes_blocking(data, &name))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(target_arch = "wasm32")]
pub async fn run_load(data: Vec<u8>, name: String) -> Result<LoadResult, String> {
    wasm_api::run_load(data, name).await
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_decompile(
    binary: Option<Arc<LoadedBinary>>,
    _session_id: Option<String>,
    addr: u64,
    name: String,
) -> Result<DecompileOutput, String> {
    let binary = binary.ok_or_else(|| "No binary loaded".to_string())?;
    tokio::task::spawn_blocking(move || decompile_blocking(&binary, addr, &name))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(target_arch = "wasm32")]
pub async fn run_decompile(
    binary: Option<Arc<LoadedBinary>>,
    session_id: Option<String>,
    addr: u64,
    name: String,
) -> Result<DecompileOutput, String> {
    wasm_api::run_decompile(binary, session_id, addr, name).await
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_xrefs(
    binary: Option<Arc<LoadedBinary>>,
    _session_id: Option<String>,
    fn_addr: u64,
) -> (Vec<XrefRow>, Vec<XrefRow>) {
    let Some(bin) = binary else { return (vec![], vec![]); };
    tokio::task::spawn_blocking(move || xrefs_for_function_blocking(&bin, fn_addr))
        .await
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub async fn run_xrefs(
    binary: Option<Arc<LoadedBinary>>,
    session_id: Option<String>,
    fn_addr: u64,
) -> (Vec<XrefRow>, Vec<XrefRow>) {
    wasm_api::run_xrefs(binary, session_id, fn_addr).await
}

/// Thin helper for keyboard nav — delegates to sidebar's decompile path.
#[cfg(not(target_arch = "wasm32"))]
pub async fn run_nav_decompile(
    state: dioxus::prelude::Signal<crate::state::AppState>,
    binary: Option<Arc<LoadedBinary>>,
    session_id: Option<String>,
    addr: u64,
    name: String,
) {
    crate::components::sidebar::run_decompile(state, binary, session_id, addr, name).await;
}

#[cfg(target_arch = "wasm32")]
pub async fn run_nav_decompile(
    state: dioxus::prelude::Signal<crate::state::AppState>,
    _binary: Option<Arc<LoadedBinary>>,
    session_id: Option<String>,
    addr: u64,
    name: String,
) {
    crate::components::sidebar::run_decompile(state, None, session_id, addr, name).await;
}
