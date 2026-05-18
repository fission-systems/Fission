//! Function analysis — discovery, deep scan, and signature identification (FID).

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::services::cross_image::AutoRenameKind;
use crate::state::AppState;
use fission_static::analysis::{discover_functions_with_runtime, FunctionDiscoveryProfile};
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Discover internal functions from SLEIGH-decoded direct call targets.
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
    let report = discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Balanced);
    let found = binary.functions.len().saturating_sub(before);

    // Snapshot renames before we move the binary back into the Arc
    let renames = inner.renamed_functions.clone();

    // Replace the Arc with the updated binary
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());
    inner.xref_database = None;
    inner.rebuild_fact_store();
    tracing::debug!(
        decoded = report.decoded_instruction_count,
        calls = report.call_target_count,
        jumps = report.jump_target_count,
        accepted = report.accepted_function_count,
        unsupported_runtime = report.unsupported_runtime,
        "SLEIGH function discovery completed"
    );
    let _ = found; // delta surfaced to the frontend via the returned slice length

    let functions = crate::commands::binary::functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

/// Discover functions from SLEIGH-decoded direct call and branch targets.
/// This is a deeper analyzer pass than `analyze_functions`, but it does not
/// use byte-pattern prologue scans.
#[tauri::command]
pub async fn deep_scan_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let mut inner = state.inner.lock().await;

    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    let report = discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Aggressive);

    let renames = inner.renamed_functions.clone();
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());
    inner.xref_database = None;
    inner.rebuild_fact_store();
    tracing::debug!(
        decoded = report.decoded_instruction_count,
        calls = report.call_target_count,
        jumps = report.jump_target_count,
        accepted = report.accepted_function_count,
        unsupported_runtime = report.unsupported_runtime,
        "SLEIGH aggressive function discovery completed"
    );

    let functions = crate::commands::binary::functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

/// Scan all known functions in the loaded binary against the built-in MSVC/CRT
/// signature database and rename any that match.
///
/// Returns a [`FidResultDto`] with the count of matched functions and their
/// names so the frontend can refresh the function list.
#[tauri::command]
pub async fn run_fid(state: State<'_, AppState>) -> CmdResult<FidResultDto> {
    use fission_signatures::{FidDatabaseSet, SignatureDatabase};

    // Collect everything we need from `binary` inside a block so the immutable
    // borrow of `inner` ends before further mutable work.
    let (binary_for_fid, func_list, prev_names) = {
        let inner = state.inner.lock().await;
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

        (binary.clone(), func_list, prev_names)
    };

    let total_scanned = func_list.len();
    let fidbf_set = FidDatabaseSet::discover_for_load_spec(
        None,
        None,
        Some(&binary_for_fid.format),
        binary_for_fid.is_64bit,
    );
    let fidbf_loaded = fidbf_set.databases.len();
    let fidbf_failed = fidbf_set.errors.len();
    let fidbf_attempted = fidbf_loaded + fidbf_failed;

    // Run built-in byte-pattern identification in a blocking thread (CPU-bound).
    // Clone func_list so it remains available after the closure consumes its capture.
    let func_list_for_fid = func_list.clone();
    let identified = tokio::task::spawn_blocking(move || {
        let db = SignatureDatabase::new();
        let mut identified = std::collections::HashMap::new();
        for (addr, _current_name) in &func_list_for_fid {
            let available = binary_for_fid
                .available_execution_bytes(*addr)
                .unwrap_or(0)
                .min(32);
            if available == 0 {
                continue;
            }
            let Some(func_bytes) = binary_for_fid.view_bytes(*addr, available) else {
                continue;
            };
            if let Some(sig) = db.identify(func_bytes) {
                identified.insert(*addr, sig.name.clone());
            }
        }
        identified
    })
    .await
    .map_err(|e| CmdError::other(format!("FID task failed: {e}")))?;

    // Apply renames to the state and collect match details.
    let mut inner = state.inner.lock().await;
    let mut matches = Vec::new();
    for (addr, new_name) in &identified {
        let prev_name = prev_names.get(addr).cloned().unwrap_or_default();
        inner.renamed_functions.insert(*addr, new_name.clone());
        inner
            .auto_renamed_functions
            .insert(*addr, AutoRenameKind::StrongFid);
        matches.push(FidMatchDto {
            address: format!("0x{:x}", addr),
            name: new_name.clone(),
            previous_name: prev_name,
        });
    }
    inner.rebuild_fact_store();

    let matched = matches.len();

    Ok(FidResultDto {
        matched,
        total_scanned,
        fidbf_attempted,
        fidbf_loaded,
        fidbf_failed,
        matches,
    })
}
