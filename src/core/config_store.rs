use std::path::PathBuf;
use std::fs;
use anyhow::{Result, Context};
use crate::ui::gui::SettingsState;

const CONFIG_DIR: &str = ".fission";
const CONFIG_FILE: &str = "config.toml";

/// Get the path to the configuration file
fn get_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(home.join(CONFIG_DIR).join(CONFIG_FILE))
}

/// Load settings from disk, or return default if not found
pub fn load() -> SettingsState {
    match load_internal() {
        Ok(settings) => {
            log::info!("Loaded configuration from disk");
            settings
        },
        Err(e) => {
            log::warn!("Failed to load config, using defaults: {}", e);
            SettingsState::default()
        }
    }
}

fn load_internal() -> Result<SettingsState> {
    let path = get_config_path()?;
    if !path.exists() {
        return Ok(SettingsState::default());
    }

    let content = fs::read_to_string(&path)?;
    let settings: SettingsState = toml::from_str(&content)?;
    Ok(settings)
}

/// Save settings to disk
pub fn save(settings: &SettingsState) -> Result<()> {
    let path = get_config_path()?;
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(settings)?;
    fs::write(path, content)?;
    Ok(())
}
