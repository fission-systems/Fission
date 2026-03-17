//! Project persistence — save/load annotations, snapshots, and analysis exports.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::parse_address;
use std::path::{Path, PathBuf};
use tauri::State;

fn validated_read_path(path: &str) -> CmdResult<PathBuf> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(CmdError::other("Invalid input path"));
    }

    let input = PathBuf::from(path);
    if !input.is_absolute() {
        return Err(CmdError::other("Path must be absolute"));
    }

    input
        .canonicalize()
        .map_err(|e| CmdError::other(format!("Invalid input path: {e}")))
}

fn validated_write_path(path: &str) -> CmdResult<PathBuf> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(CmdError::other("Invalid output path"));
    }

    let output = PathBuf::from(path);
    if !output.is_absolute() {
        return Err(CmdError::other("Path must be absolute"));
    }

    let parent = output
        .parent()
        .ok_or_else(|| CmdError::other("Output path must have a parent directory"))?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| CmdError::other(format!("Invalid output directory: {e}")))?;

    if !canonical_parent.is_dir() {
        return Err(CmdError::other("Output directory is not a directory"));
    }

    let file_name = output
        .file_name()
        .ok_or_else(|| CmdError::other("Output path must include a file name"))?;

    Ok(Path::new(&canonical_parent).join(file_name))
}

// ============================================================================
// Commands
// ============================================================================

/// Save the current project (user annotations) to a `.fprj` JSON file.
#[tauri::command]
pub async fn save_project(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let path = validated_write_path(&path)?;
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    // Convert u64 comment keys → hex address strings
    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(addr, text)| (format!("0x{:x}", addr), text.clone()))
        .collect();

    let renames: std::collections::HashMap<String, String> = inner
        .renamed_functions
        .iter()
        .map(|(addr, name)| (format!("0x{:x}", addr), name.clone()))
        .collect();

    let project = crate::dto::FissionProject {
        version: 1,
        binary_path: binary.path.clone(),
        comments,
        renames,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| CmdError::other(format!("Serialise failed: {e}")))?;

    std::fs::write(&path, json).map_err(|e| CmdError::other(format!("Write failed: {e}")))?;

    Ok(())
}

/// Load a `.fprj` project file.  Restores user annotations from the file.
/// The binary itself must already be (or will be) loaded separately via `open_file`.
/// Returns the recorded binary path so the frontend can reload it if needed.
#[tauri::command]
pub async fn load_project(
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<crate::dto::FissionProject> {
    let path = validated_read_path(&path)?;

    let json =
        std::fs::read_to_string(&path).map_err(|e| CmdError::other(format!("Read failed: {e}")))?;

    let project: crate::dto::FissionProject =
        serde_json::from_str(&json).map_err(|e| CmdError::other(format!("Parse failed: {e}")))?;

    // Apply user annotations to current state
    let mut inner = state.inner.lock().await;

    // Convert hex address strings back to u64 keys
    inner.comments = project
        .comments
        .iter()
        .filter_map(|(addr_str, text)| parse_address(addr_str).map(|a| (a, text.clone())))
        .collect();

    inner.renamed_functions = project
        .renames
        .iter()
        .filter_map(|(addr_str, name)| parse_address(addr_str).map(|a| (a, name.clone())))
        .collect();
    inner.manual_renamed_functions = inner.renamed_functions.keys().copied().collect();
    inner.auto_renamed_functions.clear();
    inner.rebuild_fact_store();

    inner.bookmarks = project.bookmarks.clone();

    Ok(project)
}

/// Save current annotations (comments, renames, bookmarks) to a snapshot file.
/// Unlike save_project, this does NOT require a binary to be loaded.
#[tauri::command]
pub async fn save_snapshot(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let path = validated_write_path(&path)?;
    let inner = state.inner.lock().await;

    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(addr, text)| (format!("0x{:x}", addr), text.clone()))
        .collect();
    let renames: std::collections::HashMap<String, String> = inner
        .renamed_functions
        .iter()
        .map(|(addr, name)| (format!("0x{:x}", addr), name.clone()))
        .collect();

    let snapshot = FissionProject {
        version: 1,
        binary_path: inner
            .loaded_binary
            .as_ref()
            .map(|b| b.path.clone())
            .unwrap_or_default(),
        comments,
        renames,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CmdError::other(format!("Serialise failed: {e}")))?;
    std::fs::write(&path, json).map_err(|e| CmdError::other(format!("Write failed: {e}")))?;
    Ok(())
}

/// Load a snapshot file and restore annotations.
/// Returns the binary_path stored in the snapshot so the frontend can reload it.
#[tauri::command]
pub async fn load_snapshot(path: String, state: State<'_, AppState>) -> CmdResult<FissionProject> {
    let path = validated_read_path(&path)?;

    let json =
        std::fs::read_to_string(&path).map_err(|e| CmdError::other(format!("Read failed: {e}")))?;
    let snapshot: FissionProject =
        serde_json::from_str(&json).map_err(|e| CmdError::other(format!("Parse failed: {e}")))?;

    let mut inner = state.inner.lock().await;
    inner.comments = snapshot
        .comments
        .iter()
        .filter_map(|(addr_str, text)| parse_address(addr_str).map(|a| (a, text.clone())))
        .collect();
    inner.renamed_functions = snapshot
        .renames
        .iter()
        .filter_map(|(addr_str, name)| parse_address(addr_str).map(|a| (a, name.clone())))
        .collect();
    inner.manual_renamed_functions = inner.renamed_functions.keys().copied().collect();
    inner.auto_renamed_functions.clear();
    inner.rebuild_fact_store();
    inner.bookmarks = snapshot.bookmarks.clone();
    Ok(snapshot)
}

/// Return the current git branch name, or "—" if outside a git repo.
#[tauri::command]
pub async fn get_git_branch() -> String {
    tokio::task::spawn_blocking(|| {
        std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "HEAD")
            .unwrap_or_else(|| "\u{2014}".to_string())
    })
    .await
    .unwrap_or_else(|_| "\u{2014}".to_string())
}

/// Export all analysis artefacts (functions, comments, bookmarks) to a JSON
/// file at `path`.  The caller (frontend) opens a save-file dialog and passes
/// the chosen path.
#[tauri::command]
pub async fn export_analysis_json(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let path = validated_write_path(&path)?;
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    // Collect functions with current names applied.
    let functions: Vec<ExportedFunctionDto> = binary
        .functions
        .iter()
        .map(|f| {
            let name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            let is_renamed = inner.renamed_functions.contains_key(&f.address);
            ExportedFunctionDto {
                address: format!("0x{:x}", f.address),
                name,
                is_renamed,
            }
        })
        .collect();

    // Collect comments keyed by hex address.
    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(k, v)| (format!("0x{:x}", k), v.clone()))
        .collect();

    let binary_name = std::path::Path::new(&binary.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let binary_fingerprint = format!("bytes:{}", binary.inner().data.as_slice().len());

    let exported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let export = AnalysisExportDto {
        version: 1,
        exported_at,
        binary_name,
        binary_path: binary.path.clone(),
        binary_fingerprint,
        functions,
        comments,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| CmdError::other(format!("JSON serialisation failed: {e}")))?;

    std::fs::write(&path, json).map_err(|e| CmdError::other(format!("Write failed: {e}")))?;

    Ok(())
}
