use crate::core::errors::{FissionError, Result};
use crate::core::settings::SettingsState;
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".fission";
const CONFIG_FILE: &str = "config.toml";

/// Get the path to the configuration file
fn get_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| FissionError::config("Failed to determine home directory"))?;
    Ok(home.join(CONFIG_DIR).join(CONFIG_FILE))
}

/// Load settings from disk, or return default if not found
pub fn load() -> SettingsState {
    match load_internal() {
        Ok(settings) => {
            crate::core::logging::info("Loaded configuration from disk");
            settings
        }
        Err(e) => {
            crate::core::logging::warn(&format!("Failed to load config, using defaults: {}", e));
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
    let settings: SettingsState = toml::from_str(&content)
        .map_err(|e| FissionError::config(format!("Invalid config format: {}", e)))?;
    Ok(settings)
}

/// Save settings to disk
pub fn save(settings: &SettingsState) -> Result<()> {
    let path = get_config_path()?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(settings)
        .map_err(|e| FissionError::config(format!("Failed to serialize config: {}", e)))?;
    fs::write(path, content)?;
    Ok(())
}
