//! Function analysis — discovery, deep scan, and signature identification (FID).

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Discover internal functions by scanning for CALL targets in executable sections.
/// Updates the loaded binary in-place and returns the full (updated) function list.
#[tauri::command]
pub async fn analyze_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let mut inner = state.inner.lock().await;

    // Clone the LoadedBinary so we can mutate it
    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    let before = binary.functions.len();
    binary.discover_internal_functions();
    let found = binary.functions.len().saturating_sub(before);

    // Snapshot renames before we move the binary back into the Arc
    let renames = inner.renamed_functions.clone();

    // Replace the Arc with the updated binary
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());
    let _ = found; // delta surfaced to the frontend via the returned slice length

    let functions = super::binary::functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

/// Discover functions by scanning for common prologue byte patterns (push rbp / push ebp etc.).
/// This is a deeper heuristic scan that can find obfuscated or tail-call functions missed by
/// `analyze_functions`.  Returns the full updated function list.
#[tauri::command]
pub async fn deep_scan_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let mut inner = state.inner.lock().await;

    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    binary.discover_functions_by_prologue();

    let renames = inner.renamed_functions.clone();
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());

    let functions = super::binary::functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

/// Scan all known functions in the loaded binary against the built-in MSVC/CRT
/// signature database and rename any that match.
///
/// Returns a [`FidResultDto`] with the count of matched functions and their
/// names so the frontend can refresh the function list.
#[tauri::command]
pub async fn run_fid(state: State<'_, AppState>) -> CmdResult<FidResultDto> {
    use fission_signatures::SignatureDatabase;

    let mut inner = state.inner.lock().await;

    // Collect everything we need from `binary` inside a block so the immutable
    // borrow of `inner` ends before we mutably update `renamed_functions`.
    let (data, image_base, func_list, prev_names) = {
        let binary = inner
            .loaded_binary
            .as_ref()
            .ok_or_else(|| CmdError::other("No binary loaded"))?;

        let mut prev_names: std::collections::HashMap<u64, String> =
            std::collections::HashMap::new();
        let func_list: Vec<(u64, String)> = binary
            .functions
            .iter()
            .map(|f| {
                let current = inner
                    .renamed_functions
                    .get(&f.address)
                    .cloned()
                    .unwrap_or_else(|| f.name.clone());
                prev_names.insert(f.address, current.clone());
                (f.address, current)
            })
            .collect();

        let data: Vec<u8> = binary.inner().data.as_slice().to_vec();
        let image_base = binary.image_base;

        (data, image_base, func_list, prev_names)
    }; // `binary` (and immutable borrow of `inner`) dropped here

    let total_scanned = func_list.len();

    // Run identification in a blocking thread (CPU-bound).
    let identified = tokio::task::spawn_blocking(move || {
        let db = SignatureDatabase::new();
        db.identify_functions_in_binary(&data, &func_list, image_base)
    })
    .await
    .map_err(|e| CmdError::other(format!("FID task failed: {e}")))?;

    // Apply renames to the state and collect match details.
    let mut matches = Vec::new();
    for (addr, new_name) in &identified {
        let prev_name = prev_names.get(addr).cloned().unwrap_or_default();
        inner.renamed_functions.insert(*addr, new_name.clone());
        matches.push(FidMatchDto {
            address: format!("0x{:x}", addr),
            name: new_name.clone(),
            previous_name: prev_name,
        });
    }

    let matched = matches.len();
    Ok(FidResultDto {
        matched,
        total_scanned,
        matches,
    })
}
