//! Application settings — load and persist user preferences.

use crate::dto::DecompilerOptions;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::SETTINGS_FILENAME;
use tauri::Manager as _;
use tracing::warn;

// ============================================================================
// Private helpers
// ============================================================================

fn normalize_decompiler_options(options: DecompilerOptions) -> DecompilerOptions {
    // Legacy engine values from older settings are deserialized into Auto via aliasing.
    options
}

fn normalize_settings(mut settings: crate::dto::AppSettings) -> crate::dto::AppSettings {
    settings.decompiler_options = settings
        .decompiler_options
        .map(normalize_decompiler_options);
    settings
}

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
    Ok(normalize_settings(
        serde_json::from_str(&json).unwrap_or_else(|_| {
            warn!(
                file = SETTINGS_FILENAME,
                "settings invalid or schema changed, using defaults"
            );
            crate::dto::AppSettings::default()
        }),
    ))
}

/// Persist application settings.
#[tauri::command]
pub async fn save_settings(
    settings: crate::dto::AppSettings,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let path = settings_path(&app_handle)?;
    let settings = normalize_settings(settings);
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| CmdError::other(format!("Serialise settings failed: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write settings failed: {e}")))?;
    Ok(())
}

/// Get current decompiler options (returns defaults if not yet configured).
#[tauri::command]
pub async fn get_decompiler_options(app_handle: tauri::AppHandle) -> CmdResult<DecompilerOptions> {
    let settings = get_settings(app_handle).await?;
    Ok(settings.decompiler_options.unwrap_or_default())
}

/// Persist decompiler options for Rust-only decompilation.
#[tauri::command]
pub async fn apply_decompiler_options(
    options: DecompilerOptions,
    _state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let options = normalize_decompiler_options(options);
    // Persist to settings.json
    let mut settings = get_settings(app_handle.clone()).await?;
    settings.decompiler_options = Some(options);
    save_settings(settings, app_handle).await?;

    Ok(())
}
