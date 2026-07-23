//! Blocking helpers for the GUI — binary load and decompile.
//!
//! All Fission core APIs are synchronous; these helpers wrap them so they can
//! be called from `tokio::task::spawn_blocking` without blocking the UI thread.

use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::Path;
use std::sync::Arc;

// ── Load ────────────────────────────────────────────────────────────────────

pub struct LoadResult {
    pub binary: Arc<LoadedBinary>,
    pub functions: Vec<FunctionInfo>,
    pub summary: String,
}

/// Load a binary from disk (blocking).  
/// Returns the parsed binary together with a sorted function list and a
/// human-readable summary string for the log panel.
pub fn load_binary_blocking(path: &Path) -> Result<LoadResult, String> {
    let binary =
        LoadedBinary::from_file(path).map_err(|e| format!("Load failed: {e}"))?;

    let mut functions = binary.functions.clone();
    functions.sort_by_key(|f| f.address);

    let summary = format!(
        "{} | {} | {} functions | entry 0x{:x}",
        binary.format,
        if binary.is_64bit { "64-bit" } else { "32-bit" },
        functions.len(),
        binary.entry_point,
    );

    Ok(LoadResult { binary: Arc::new(binary), functions, summary })
}

// ── Decompile ───────────────────────────────────────────────────────────────

pub struct DecompileOutput {
    pub code: String,
    pub code_nir: Option<String>,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
}

/// Decompile a single function (blocking).
pub fn decompile_blocking(
    binary: &Arc<LoadedBinary>,
    addr: u64,
    name: &str,
) -> Result<DecompileOutput, String> {
    let facts = FactStore::from_binary(binary.as_ref());

    let mut config = RustSleighDecompileConfig::cli_defaults();
    config.nir_timeout_ms = Some(10_000); // 10 s GUI timeout

    let result = decompile_with_rust_sleigh_with_facts(
        binary.as_ref(),
        &facts,
        addr,
        name,
        &config,
        None,
        None,
    )?;

    Ok(DecompileOutput {
        code: result.code,
        code_nir: result.code_nir,
        fell_back: result.fell_back,
        fallback_reason: result.fallback_reason,
    })
}
