//! Wire types — JSON request/response shapes shared across handlers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Status ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusResponse {
    pub version:         &'static str,
    pub active_sessions: usize,
    pub max_sessions:    usize,
}

// ── Binary upload ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UploadResponse {
    /// Opaque session token — include in all subsequent requests.
    pub session_id: Uuid,
    pub fn_count:   usize,
    pub summary:    String,
}

// ── Function list ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct FnEntry {
    pub addr:      u64,
    pub name:      String,
    pub is_import: bool,
    pub is_export: bool,
    pub is_thunk:  bool,
    pub size:      u64,
}

// ── Decompile ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DecompileResponse {
    pub pseudocode: String,
    pub nir:        Option<String>,
    pub fell_back:  bool,
    pub reason:     Option<String>,
}

// ── Xrefs ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct XrefRow {
    pub from_addr: u64,
    pub to_addr:   Option<u64>,
    pub kind:      String,
    pub symbol:    Option<String>,
    pub fn_name:   Option<String>,
}

#[derive(Serialize)]
pub struct XrefsResponse {
    pub callers: Vec<XrefRow>,
    pub callees: Vec<XrefRow>,
}

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}
