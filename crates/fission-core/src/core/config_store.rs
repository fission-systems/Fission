use crate::core::errors::{FissionError, Result};
use crate::core::settings::SettingsState;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CONFIG_DIR: &str = ".fission";
const CONFIG_FILE: &str = "config.toml";
static TEMP_FILE_SERIAL: AtomicU64 = AtomicU64::new(0);

/// Write `content` to `path` atomically via a sibling temp file and rename.
pub fn write_file_atomically(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let mut temp_name = path.as_os_str().to_os_string();
    temp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        TEMP_FILE_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let temp_path = PathBuf::from(temp_name);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    let write_result = (|| -> std::io::Result<()> {
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_path, path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

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
    save_to_path(&path, settings)
}

/// Save settings atomically to an explicit path.
pub fn save_to_path(path: &Path, settings: &SettingsState) -> Result<()> {
    let content = toml::to_string_pretty(settings)
        .map_err(|e| FissionError::config(format!("Failed to serialize config: {}", e)))?;
    write_file_atomically(path, content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_to_path_writes_atomically_and_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("fission_cfg_store_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("config.toml");

        let first = SettingsState::default();
        save_to_path(&path, &first).expect("initial save");
        let second = SettingsState::default();
        save_to_path(&path, &second).expect("overwrite save");

        assert!(path.is_file());
        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["config.toml".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }
}
