//! Plugin management — load, unload, list, enable, and disable native plugins.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use tauri::State;

// ============================================================================
// Private helpers
// ============================================================================

/// Convert fission-core PluginInfo → PluginInfoDto.
fn plugin_info_to_dto(info: &fission_plugin::plugin::api::PluginInfo) -> PluginInfoDto {
    use fission_plugin::plugin::api::PluginType;
    PluginInfoDto {
        id: info.id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        author: info.author.clone(),
        description: info.description.clone(),
        plugin_type: match info.plugin_type {
            PluginType::Native => PluginTypeDto::Native,
            _ => PluginTypeDto::Unknown,
        },
        enabled: info.enabled,
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Load a Rust native plugin (.so / .dylib / .dll) from disk.
/// Returns the plugin metadata on success.
#[tauri::command]
pub async fn load_plugin(path: String, state: State<'_, AppState>) -> CmdResult<PluginInfoDto> {
    let mut mgr = state.plugin_manager.lock().await;
    let plugin_id = mgr.load_plugin(&path)?;
    let info = mgr
        .get_plugin(&plugin_id)
        .ok_or_else(|| CmdError::other(format!("Plugin '{}' not found after load", plugin_id)))?;
    Ok(plugin_info_to_dto(info))
}

/// Unload a plugin by its ID.
#[tauri::command]
pub async fn unload_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.unload_plugin(&plugin_id).map_err(CmdError::other)
}

/// List all currently loaded plugins.
#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> CmdResult<Vec<PluginInfoDto>> {
    let mgr = state.plugin_manager.lock().await;
    let mut plugins: Vec<PluginInfoDto> = mgr
        .list_plugins()
        .into_iter()
        .map(plugin_info_to_dto)
        .collect();
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plugins)
}

/// Enable a loaded plugin.
#[tauri::command]
pub async fn enable_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.enable_plugin(&plugin_id).map_err(CmdError::other)
}

/// Disable a loaded plugin (keeps it in memory but marks it inactive).
#[tauri::command]
pub async fn disable_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.disable_plugin(&plugin_id).map_err(CmdError::other)
}
