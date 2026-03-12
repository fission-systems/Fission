//! File / Binary operations — open, inspect, and query loaded binaries.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::{find_sla_dir, format_addr};
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use tauri::State;
use tracing::{error, warn};

// ============================================================================
// Commands
// ============================================================================

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> CmdResult<BinaryInfo> {
    let binary = tokio::task::spawn_blocking(move || {
        let mut binary = LoadedBinary::from_file(&path)
            .map_err(|e| CmdError::other(format!("Failed to load binary: {e}")))?;
        // Automatic multi-pass function discovery (runs in the worker thread)
        binary.discover_internal_functions(); // Pass 1: CALL target scan
        binary.discover_functions_by_prologue(); // Pass 2: prologue pattern scan
        Ok::<LoadedBinary, CmdError>(binary)
    })
    .await
    .map_err(|e| CmdError::other(format!("Task failed: {e}")))??;

    let info = binary_to_info(&binary);

    let binary_arc = Arc::new(binary);

    // Initialize decompiler if native_decomp feature is enabled
    #[cfg(feature = "native_decomp")]
    {
        use fission_static::analysis::decomp::{
            prepare_native_decompiler_for_binary, PrepareOptions,
        };

        let sla_dir = find_sla_dir();
        match fission_static::analysis::decomp::CachingDecompiler::new(&binary_arc, &sla_dir, 200)
        {
            Ok(mut decomp) => {
                let bin_ref = binary_arc.clone();
                let compiler_id = bin_ref.get_ghidra_compiler_id();
                let config = fission_core::config::Config::default();
                let gdt_path_owned = fission_core::PATHS
                    .get_gdt_path(bin_ref.is_64bit)
                    .and_then(|p| p.to_str().map(String::from));
                let mut options = PrepareOptions {
                    verbose: false,
                    compiler_id: compiler_id.as_deref(),
                    gdt_path: gdt_path_owned.as_deref(),
                    timeout_ms: Some(config.decompiler.timeout_ms),
                    timings: None,
                    signatures_json: None,
                };

                if let Err(e) = prepare_native_decompiler_for_binary(
                    decomp.inner_mut(),
                    &bin_ref,
                    bin_ref.data.as_slice(),
                    &mut options,
                ) {
                    warn!(error = %e, "failed to prepare decompiler for binary");
                } else {
                    // Store decompiler in its own separate Mutex
                    let mut decomp_lock = state.decompiler.lock().await;
                    *decomp_lock = Some(decomp);
                    drop(decomp_lock);
                    let mut inner = state.inner.lock().await;
                    inner.decompiler_loaded = true;
                }
            }
            Err(e) => {
                error!(error = %e, "failed to initialize decompiler");
            }
        }
    }

    // Store the binary and reset user state
    let mut inner = state.inner.lock().await;
    inner.loaded_binary = Some(binary_arc);
    inner.comments.clear();
    inner.renamed_functions.clear();
    inner.bookmarks.clear();

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
