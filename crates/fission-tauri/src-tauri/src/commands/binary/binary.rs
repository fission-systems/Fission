//! File / Binary operations — open, inspect, and query loaded binaries.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::format_addr;
use fission_loader::detector::Detection;
use fission_loader::loader::function_view::{canonical_functions_sorted, canonical_view_counts};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{discover_functions_with_runtime, FunctionDiscoveryProfile};
use std::sync::Arc;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> CmdResult<BinaryInfo> {
    let binary = tokio::task::spawn_blocking(move || {
        let mut binary = LoadedBinary::from_file(&path)
            .map_err(|e| CmdError::other(format!("Failed to load binary: {e}")))?;
        // Keep open_file responsive: run only direct call-target discovery.
        // The aggressive branch-target analyzer is available via `deep_scan_functions`.
        let _ =
            discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Conservative);
        Ok::<LoadedBinary, CmdError>(binary)
    })
    .await
    .map_err(|e| CmdError::other(format!("Task failed: {e}")))??;

    let info = binary_to_info(&binary);
    let binary_arc = Arc::new(binary);

    // Store the binary and reset user state
    let mut inner = state.inner.lock().await;
    inner.loaded_binary = Some(binary_arc);
    inner.xref_database = None;
    inner.comments.clear();
    inner.renamed_functions.clear();
    inner.manual_renamed_functions.clear();
    inner.auto_renamed_functions.clear();
    inner.bookmarks.clear();
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
    let counts = canonical_view_counts(binary);
    let bits = binary
        .architecture
        .as_ref()
        .map(|arch| arch.bitness)
        .unwrap_or(if binary.is_64bit { 64 } else { 32 });
    let detections = fission_loader::detect(binary)
        .detections
        .iter()
        .map(detection_to_info)
        .collect();
    BinaryInfo {
        name: std::path::Path::new(&binary.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: binary.path.clone(),
        arch: binary.arch_spec.clone(),
        bits: u32::from(bits),
        format: binary.format.clone(),
        entry_point: format_addr(binary.entry_point),
        section_count: binary.sections.len(),
        function_count: counts.functions,
        import_count: counts.imports,
        export_count: counts.exports,
        image_base: format_addr(binary.image_base),
        detections,
    }
}

fn detection_to_info(detection: &Detection) -> DetectionInfo {
    DetectionInfo {
        detection_type: detection.detection_type.to_string(),
        name: detection.name.clone(),
        version: detection.version.clone(),
        confidence: detection.confidence.to_string(),
        details: detection.details.clone(),
    }
}

/// Return the category string for a function.
fn function_category(f: &fission_loader::loader::FunctionInfo) -> &'static str {
    if f.is_import || matches!(f.kind.as_deref(), Some("import")) {
        "import"
    } else if matches!(f.kind.as_deref(), Some("undefined_external")) {
        "external"
    } else if f.is_thunk_like || matches!(f.kind.as_deref(), Some("import_thunk")) {
        "thunk"
    } else if f.is_export {
        "export"
    } else if matches!(f.kind.as_deref(), Some("debug_symbol")) {
        "debug"
    } else {
        "internal"
    }
}

/// Map every function in `binary` to a [`FunctionDto`], applying any renames.
pub(crate) fn functions_to_dtos(
    binary: &fission_loader::loader::LoadedBinary,
    renames: &std::collections::HashMap<u64, String>,
) -> Vec<FunctionDto> {
    canonical_functions_sorted(binary)
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
                is_import: f.is_import,
                is_export: f.is_export,
                origin: f.origin.clone(),
                kind: f.kind.clone(),
                source_section: f.source_section.clone(),
                external_library: f.external_library.clone(),
                is_thunk_like: f.is_thunk_like,
                category: function_category(f).to_string(),
            }
        })
        .collect()
}
