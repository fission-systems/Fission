//! File / Binary operations — open, inspect, and query loaded binaries.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::services::cross_image::{apply_propagated_renames, collect_folder_propagated_renames};
use crate::state::AppState;
use fission_core::format_addr;
use fission_loader::loader::{FunctionDiscoveryProfile, LoadedBinary};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use tracing::warn;

// ============================================================================
// Commands
// ============================================================================

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> CmdResult<BinaryInfo> {
    let binary = tokio::task::spawn_blocking(move || {
        let mut binary = LoadedBinary::from_file(&path)
            .map_err(|e| CmdError::other(format!("Failed to load binary: {e}")))?;
        // Keep open_file responsive: run only the lightweight pass on initial load.
        // The heavier prologue scan is available via `deep_scan_functions`.
        binary.discover_internal_functions_with_profile(FunctionDiscoveryProfile::Conservative);
        // Pass 1: CALL target scan
        Ok::<LoadedBinary, CmdError>(binary)
    })
    .await
    .map_err(|e| CmdError::other(format!("Task failed: {e}")))??;

    let info = binary_to_info(&binary);
    let binary_arc = Arc::new(binary);
    let propagation_folder = Path::new(&binary_arc.path)
        .parent()
        .map(|path| path.to_path_buf());
    let propagated_renames = if let Some(folder) = propagation_folder {
        let binary_for_propagation = binary_arc.clone();
        match tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                collect_folder_propagated_renames(binary_for_propagation.as_ref(), &folder)
            }),
        )
        .await
        {
            Ok(joined) => joined
                .map_err(|e| CmdError::other(format!("Propagation task failed: {e}")))?,
            Err(_) => {
                warn!("cross-image propagation timed out during open_file; skipping for responsiveness");
                Default::default()
            }
        }
    } else {
        Default::default()
    };

    // Store the binary and reset user state
    let mut inner = state.inner.lock().await;
    inner.loaded_binary = Some(binary_arc);
    inner.comments.clear();
    inner.renamed_functions.clear();
    inner.manual_renamed_functions.clear();
    inner.auto_renamed_functions.clear();
    inner.bookmarks.clear();
    let loaded_binary = inner.loaded_binary.clone();
    if let Some(binary) = loaded_binary.as_ref() {
        let manual = inner.manual_renamed_functions.clone();
        let mut renamed_functions = std::mem::take(&mut inner.renamed_functions);
        let mut auto_renamed_functions = std::mem::take(&mut inner.auto_renamed_functions);
        let _ = apply_propagated_renames(
            binary,
            &mut renamed_functions,
            &manual,
            &mut auto_renamed_functions,
            propagated_renames,
        );
        inner.renamed_functions = renamed_functions;
        inner.auto_renamed_functions = auto_renamed_functions;
    }
    inner.rebuild_fact_store();

    // Enable binary-dependent menu items
    if let Some(handles) = state.menu_handles.get() {
        handles.set_binary_loaded(true);
    }

    Ok(info)
}

/// Get all functions from the loaded binary.
#[tauri::command]
pub async fn get_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let functions = functions_to_dtos(binary, &inner.renamed_functions);

    Ok(functions)
}

/// Get info about the currently loaded binary.
#[tauri::command]
pub async fn get_binary_info(state: State<'_, AppState>) -> CmdResult<Option<BinaryInfo>> {
    let inner = state.inner.lock().await;
    let info = inner.loaded_binary.as_ref().map(|b| binary_to_info(b));
    Ok(info)
}

// ============================================================================
// Helpers (pub(super) so sibling modules can access via super::binary::*)
// ============================================================================

/// Build a [`BinaryInfo`] DTO from a [`LoadedBinary`].
pub(super) fn binary_to_info(binary: &fission_loader::loader::LoadedBinary) -> BinaryInfo {
    BinaryInfo {
        name: std::path::Path::new(&binary.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: binary.path.clone(),
        arch: binary.arch_spec.clone(),
        format: binary.format.clone(),
        entry_point: format_addr(binary.entry_point),
        section_count: binary.sections.len(),
        function_count: binary.functions.len(),
        image_base: format_addr(binary.image_base),
    }
}

/// Return the category string for a function.
fn function_category(f: &fission_loader::loader::FunctionInfo) -> &'static str {
    if f.is_import {
        "import"
    } else if f.is_export {
        "export"
    } else {
        "internal"
    }
}

/// Map every function in `binary` to a [`FunctionDto`], applying any renames.
pub(crate) fn functions_to_dtos(
    binary: &fission_loader::loader::LoadedBinary,
    renames: &std::collections::HashMap<u64, String>,
) -> Vec<FunctionDto> {
    binary
        .functions
        .iter()
        .map(|f| {
            let display_name = renames
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            FunctionDto {
                address: format_addr(f.address),
                name: display_name,
                size: f.size,
                category: function_category(f).to_string(),
            }
        })
        .collect()
}
