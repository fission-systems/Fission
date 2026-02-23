//! Application settings — load and persist user preferences.

use crate::error::{CmdError, CmdResult};
use fission_core::SETTINGS_FILENAME;
use tauri::Manager as _;
use tracing::warn;

// ============================================================================
// Private helpers
// ============================================================================

/// Path to the settings file inside the OS app-data directory.
fn settings_path(app_handle: &tauri::AppHandle) -> CmdResult<std::path::PathBuf> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| CmdError::other(format!("Cannot resolve app-data dir: {e}")))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| CmdError::other(format!("Cannot create app-data dir: {e}")))?;
    Ok(data_dir.join("settings.json"))
}

// ============================================================================
// Commands
// ============================================================================

/// Load persisted application settings (or defaults if none saved yet).
#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> CmdResult<crate::dto::AppSettings> {
    let path = settings_path(&app_handle)?;
    if !path.exists() {
        return Ok(crate::dto::AppSettings::default());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CmdError::other(format!("Read settings failed: {e}")))?;
    // If schema is corrupt or outdated, fall back to defaults silently
    Ok(serde_json::from_str(&json).unwrap_or_else(|_| {
        warn!(file = SETTINGS_FILENAME, "settings invalid or schema changed, using defaults");
        crate::dto::AppSettings::default()
    }))
}

/// Persist application settings.
#[tauri::command]
pub async fn save_settings(
    settings: crate::dto::AppSettings,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let path = settings_path(&app_handle)?;
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| CmdError::other(format!("Serialise settings failed: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write settings failed: {e}")))?;
    Ok(())
}
