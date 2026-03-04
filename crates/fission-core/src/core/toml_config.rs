//! TOML Configuration File Loader
//!
//! Loads configuration from external TOML files with the following search order:
//! 1. `FISSION_CONFIG` environment variable path
//! 2. `./fission.toml` (current directory)
//! 3. `~/.config/fission/fission.toml` (user config)
//! 4. Built-in defaults

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

use super::constants::{
    CONFIG_FILENAME, DEFAULT_DECOMP_TIMEOUT_MS, DEFAULT_L1_CACHE_SIZE, MAX_HEX_READ,
    MAX_SCAN_PER_SECTION, MB,
};

use crate::core::config::{
    AnalysisConfig, Config, DebugConfig, DecompilerConfig, LogConfig, LogLevel, UiConfig,
};

/// TOML-compatible configuration structure
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TomlConfig {
    pub logging: TomlLoggingConfig,
    pub decompiler: TomlDecompilerConfig,
    pub analysis: TomlAnalysisConfig,
    pub debug: TomlDebugConfig,
    pub ui: TomlUiConfig,
    pub paths: TomlPathsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TomlLoggingConfig {
    pub level: String,
    pub file: String,
    pub console_enabled: bool,
    pub include_timestamp: bool,
    pub include_target: bool,
}

impl Default for TomlLoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: String::new(),
            console_enabled: true,
            include_timestamp: true,
            include_target: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TomlDecompilerConfig {
    pub num_workers: usize,
    pub max_workers: usize,
    pub default_function_size: usize,
    pub max_function_size: usize,
    pub min_function_size: usize,
    pub timeout_ms: u64,
    pub enable_prefetch: bool,
    pub prefetch_count: usize,
    pub sla_dir: String,
}

impl Default for TomlDecompilerConfig {
    fn default() -> Self {
        Self {
            num_workers: 0,
            max_workers: 8,
            default_function_size: 4096,
            max_function_size: 64 * 1024,
            min_function_size: 16,
            timeout_ms: DEFAULT_DECOMP_TIMEOUT_MS,
            enable_prefetch: true,
            prefetch_count: 3,
            sla_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TomlAnalysisConfig {
    pub max_string_search_size: usize,
    pub min_string_length: usize,
    pub auto_xref_analysis: bool,
    pub decompile_cache_size: usize,
    pub function_address_range: usize,
}

impl Default for TomlAnalysisConfig {
    fn default() -> Self {
        Self {
            max_string_search_size: MAX_SCAN_PER_SECTION,
            min_string_length: 4,
            auto_xref_analysis: true,
            decompile_cache_size: DEFAULT_L1_CACHE_SIZE,
            function_address_range: MAX_HEX_READ,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TomlDebugConfig {
    pub max_snapshots: usize,
    pub max_process_ids: usize,
}

impl Default for TomlDebugConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 10000,
            max_process_ids: 4096,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TomlUiConfig {
    pub show_performance: bool,
    pub auto_scroll_entry: bool,
    pub max_log_entries: usize,
    pub hex_rows_per_page: usize,
}

impl Default for TomlUiConfig {
    fn default() -> Self {
        Self {
            show_performance: false,
            auto_scroll_entry: true,
            max_log_entries: 1000,
            hex_rows_per_page: 64,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TomlPathsConfig {
    pub workspace_root: String,
    pub fid_dir: String,
    pub gdt_dir: String,
    pub die_dir: String,
    pub patterns_dir: String,
}

impl TomlConfig {
    /// Load configuration from file
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read config file: {}", e))?;

        toml::from_str(&content).map_err(|e| format!("Failed to parse TOML config: {}", e))
    }

    /// Find and load configuration file from standard locations
    pub fn load() -> Self {
        // Try loading from standard locations
        if let Some(path) = Self::find_config_file() {
            match Self::load_from_file(&path) {
                Ok(config) => {
                    eprintln!("[fission] Loaded config from: {}", path.display());
                    return config;
                }
                Err(e) => {
                    eprintln!(
                        "[fission] Warning: Failed to load {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        // Fall back to defaults
        Self::default()
    }

    /// Find config file in standard locations
    fn find_config_file() -> Option<PathBuf> {
        // 1. Environment variable
        if let Ok(env_path) = std::env::var("FISSION_CONFIG") {
            let path = PathBuf::from(&env_path);
            if path.exists() {
                return Some(path);
            }
        }

        // 2. Current directory
        let cwd_config = PathBuf::from(CONFIG_FILENAME);
        if cwd_config.exists() {
            return Some(cwd_config);
        }

        // 3. User config directory
        if let Some(config_dir) = dirs::config_dir() {
            let user_config = config_dir.join("fission").join(CONFIG_FILENAME);
            if user_config.exists() {
                return Some(user_config);
            }
        }

        // 4. Search upward from current directory (for workspace)
        if let Ok(cwd) = std::env::current_dir() {
            let mut current = cwd.as_path();
            for _ in 0..5 {
                let candidate = current.join(CONFIG_FILENAME);
                if candidate.exists() {
                    return Some(candidate);
                }
                match current.parent() {
                    Some(parent) => current = parent,
                    None => break,
                }
            }
        }

        None
    }

    /// Get the path where config was loaded from (if any)
    pub fn config_file_path() -> Option<PathBuf> {
        Self::find_config_file()
    }
}

/// Convert TomlConfig to runtime Config
impl From<TomlConfig> for Config {
    fn from(toml: TomlConfig) -> Self {
        Config {
            decompiler: DecompilerConfig {
                num_workers: if toml.decompiler.num_workers == 0 {
                    1
                } else {
                    toml.decompiler.num_workers
                },
                max_workers: toml.decompiler.max_workers,
                default_function_size: toml.decompiler.default_function_size,
                max_function_size: toml.decompiler.max_function_size,
                min_function_size: toml.decompiler.min_function_size,
                timeout_ms: toml.decompiler.timeout_ms,
                enable_prefetch: toml.decompiler.enable_prefetch,
                prefetch_count: toml.decompiler.prefetch_count,
                sla_dir: toml.decompiler.sla_dir,
            },
            analysis: AnalysisConfig {
                max_string_search_size: toml.analysis.max_string_search_size,
                min_string_length: toml.analysis.min_string_length,
                auto_xref_analysis: toml.analysis.auto_xref_analysis,
                decompile_cache_size: toml.analysis.decompile_cache_size,
                function_address_range: toml.analysis.function_address_range,
            },
            debug: DebugConfig {
                max_snapshots: toml.debug.max_snapshots,
                max_process_ids: toml.debug.max_process_ids,
            },
            ui: UiConfig {
                show_performance: toml.ui.show_performance,
                auto_scroll_entry: toml.ui.auto_scroll_entry,
                max_log_entries: toml.ui.max_log_entries,
                hex_rows_per_page: toml.ui.hex_rows_per_page,
            },
            logging: LogConfig {
                level: LogLevel::parse(&toml.logging.level).unwrap_or_default(),
                file_path: toml.logging.file,
                console_enabled: toml.logging.console_enabled,
                file_enabled: false, // Will be set based on file_path
                include_timestamp: toml.logging.include_timestamp,
                include_target: toml.logging.include_target,
                max_file_size: 10 * MB,
                max_rotated_files: 3,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_toml_config() {
        let config = TomlConfig::default();
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.decompiler.timeout_ms, DEFAULT_DECOMP_TIMEOUT_MS);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[logging]
level = "debug"

[decompiler]
timeout_ms = 60000
"#;
        let Ok(config): std::result::Result<TomlConfig, _> = toml::from_str(toml_str) else {
            panic!("toml parse should succeed")
        };
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.decompiler.timeout_ms, 60000);
    }
}
