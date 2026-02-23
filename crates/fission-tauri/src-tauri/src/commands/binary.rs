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
        let sla_dir = find_sla_dir();
        match fission_analysis::analysis::decomp::CachingDecompiler::new(
            &binary_arc,
            &sla_dir,
            200,
        ) {
            Ok(mut decomp) => {
                let bin_ref = binary_arc.clone();
                let sleigh_id = bin_ref.arch_spec.clone();
                let compiler_id = bin_ref.get_ghidra_compiler_id();
                if let Err(e) = decomp.inner_mut().load_binary(
                    bin_ref.data.as_slice(),
                    bin_ref.image_base,
                    bin_ref.is_64bit,
                    Some(sleigh_id.as_str()),
                    compiler_id.as_deref(),
                ) {
                    warn!(error = %e, "failed to load binary into decompiler");
                } else {
                    let inner_decomp = decomp.inner_mut();

                    // Register PE sections so Ghidra can map virtual addresses to file data
                    for section in &bin_ref.sections {
                        let _ = inner_decomp.add_memory_block(
                            &section.name,
                            section.virtual_address,
                            section.virtual_size,
                            section.file_offset,
                            section.file_size,
                            section.is_executable,
                            section.is_writable,
                        );
                    }

                    // Register IAT symbols (import functions)
                    let iat_symbols: std::collections::HashMap<u64, String> = bin_ref
                        .imports()
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    inner_decomp.add_symbols(&iat_symbols);

                    // Register all functions as global symbols
                    let global_symbols: std::collections::HashMap<u64, String> = bin_ref
                        .functions
                        .iter()
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    inner_decomp.add_global_symbols(&global_symbols);

                    // Add function entries so Ghidra knows about them
                    for func in &bin_ref.functions {
                        let _ = inner_decomp.add_function(func.address, Some(&func.name));
                    }

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
pub(super) fn functions_to_dtos(
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
