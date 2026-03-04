//! Fission Configuration
//!
//! Centralized configuration for all tunable parameters.
//! All magic numbers and hardcoded values should be defined here.

use std::sync::LazyLock;

use super::constants::{
    DEFAULT_DECOMP_TIMEOUT_MS, DEFAULT_L1_CACHE_SIZE, MAX_FUNCTION_SIZE, MAX_HEX_READ, MB,
    PAGE_SIZE,
};

/// Global configuration instance
pub static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

/// Fission configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Decompiler settings
    pub decompiler: DecompilerConfig,
    /// Analysis settings
    pub analysis: AnalysisConfig,
    /// Debug settings
    pub debug: DebugConfig,
    /// UI settings
    pub ui: UiConfig,
    /// Logging settings
    pub logging: LogConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self::load()
    }
}

impl Config {
    /// Load configuration from TOML file or environment
    pub fn load() -> Self {
        crate::core::toml_config::TomlConfig::load().into()
    }
}

/// Decompiler configuration
#[derive(Debug, Clone)]
pub struct DecompilerConfig {
    /// Number of decompiler worker threads (0 = auto based on CPU cores)
    pub num_workers: usize,
    /// Maximum workers (caps auto-detection)
    pub max_workers: usize,
    /// Default function size when unknown (bytes)
    pub default_function_size: usize,
    /// Maximum function size to decompile (bytes)
    pub max_function_size: usize,
    /// Minimum function size (bytes)
    pub min_function_size: usize,
    /// Decompilation timeout (milliseconds, 0 = no timeout)
    pub timeout_ms: u64,
    /// Enable background prefetching
    pub enable_prefetch: bool,
    /// Number of functions to prefetch
    pub prefetch_count: usize,
    /// SLA (Sleigh) directory override (if empty, resolve from environment)
    pub sla_dir: String,
}

/// Analysis configuration
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Maximum binary size for string/signature search (bytes)
    pub max_string_search_size: usize,
    /// Minimum string length to detect
    pub min_string_length: usize,
    /// Enable cross-reference analysis on load
    pub auto_xref_analysis: bool,
    /// Cache size for decompiled functions (max entries)
    pub decompile_cache_size: usize,
    /// Function address search range for navigation (bytes)
    pub function_address_range: usize,
}

/// Debug/TTD configuration
#[derive(Debug, Clone)]
pub struct DebugConfig {
    /// Maximum snapshots to keep in TTD recorder
    pub max_snapshots: usize,
    /// Maximum process IDs to enumerate
    pub max_process_ids: usize,
}

/// UI configuration
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// Show performance metrics
    pub show_performance: bool,
    /// Auto-scroll to entry point on load
    pub auto_scroll_entry: bool,
    /// Maximum log entries to keep
    pub max_log_entries: usize,
    /// Hex view rows per page
    pub hex_rows_per_page: usize,
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Minimum log level to output (trace, debug, info, warn, error)
    pub level: LogLevel,
    /// Log file path (if empty, file logging disabled)
    pub file_path: String,
    /// Enable console output
    pub console_enabled: bool,
    /// Enable file output
    pub file_enabled: bool,
    /// Include timestamps in output
    pub include_timestamp: bool,
    /// Include module/target path in output
    pub include_target: bool,
    /// Maximum log file size in bytes (0 = unlimited)
    pub max_file_size: usize,
    /// Number of rotated log files to keep
    pub max_rotated_files: usize,
}

/// Log level enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

impl LogLevel {
    /// Parse log level from string (case-insensitive)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }

    /// Convert to tracing::Level
    pub fn to_tracing_level(&self) -> tracing::Level {
        match self {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        // Check environment variables for configuration
        let level = std::env::var("FISSION_LOG_LEVEL")
            .ok()
            .and_then(|s| LogLevel::parse(&s))
            .unwrap_or(LogLevel::Info);

        let file_path = std::env::var("FISSION_LOG_FILE").unwrap_or_default();
        let file_enabled = !file_path.is_empty();

        Self {
            level,
            file_path,
            console_enabled: true,
            file_enabled,
            include_timestamp: true,
            include_target: false,
            max_file_size: 10 * MB, // 10MB
            max_rotated_files: 3,
        }
    }
}

impl LogConfig {
    /// Create a config for quiet operation (errors only)
    pub fn quiet() -> Self {
        Self {
            level: LogLevel::Error,
            console_enabled: true,
            ..Default::default()
        }
    }

    /// Create a config for verbose operation (trace level)
    pub fn verbose() -> Self {
        Self {
            level: LogLevel::Trace,
            include_target: true,
            ..Default::default()
        }
    }

    /// Create a config with file logging enabled
    pub fn with_file(path: &str) -> Self {
        Self {
            file_path: path.to_string(),
            file_enabled: true,
            ..Default::default()
        }
    }

    /// Check if any logging is enabled
    pub fn is_enabled(&self) -> bool {
        self.console_enabled || self.file_enabled
    }

    /// Get environment variable used for C++ logger initialization
    pub fn get_cpp_log_file_env(&self) -> Option<(&'static str, String)> {
        if self.file_enabled && !self.file_path.is_empty() {
            Some(("FISSION_LOG_FILE", self.file_path.clone()))
        } else {
            None
        }
    }
}

impl Default for DecompilerConfig {
    fn default() -> Self {
        Self {
            num_workers: 1, // 1 = serialize requests (C++ server is single-threaded)
            max_workers: 8,
            default_function_size: PAGE_SIZE,     // 4KB
            max_function_size: MAX_FUNCTION_SIZE, // 64KB
            min_function_size: 16,
            timeout_ms: DEFAULT_DECOMP_TIMEOUT_MS,
            enable_prefetch: true,
            prefetch_count: 3,
            sla_dir: String::new(),
        }
    }
}

impl DecompilerConfig {
    /// Get effective number of workers (handles auto-detection)
    pub fn effective_num_workers(&self) -> usize {
        if self.num_workers == 0 {
            let num_cpus = std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4);
            num_cpus.min(self.max_workers)
        } else {
            self.num_workers.min(self.max_workers)
        }
    }

    pub fn resolve_sla_directory(&self) -> Result<String, String> {
        if !self.sla_dir.is_empty() {
            let path = std::path::Path::new(&self.sla_dir);
            if path.exists() && path.is_dir() {
                return Ok(self.sla_dir.clone());
            }
            return Err(format!(
                "Config sla_dir is set but path does not exist: {}",
                self.sla_dir
            ));
        }

        if let Ok(env_path) = std::env::var("FISSION_SLA_DIR") {
            let path = std::path::Path::new(&env_path);
            if path.exists() && path.is_dir() {
                return Ok(env_path);
            }
            return Err(format!(
                "FISSION_SLA_DIR is set but path does not exist: {}",
                env_path
            ));
        }

        if let Ok(cwd) = std::env::current_dir() {
            let local_path = cwd.join("ghidra_decompiler").join("languages");
            if local_path.exists() && local_path.is_dir() {
                return Ok(local_path.to_string_lossy().into_owned());
            }

            if let Some(parent) = cwd.parent() {
                let parent_path = parent.join("ghidra_decompiler").join("languages");
                if parent_path.exists() && parent_path.is_dir() {
                    return Ok(parent_path.to_string_lossy().into_owned());
                }
            }
        }

        Err("SLA directory not found. Expected at: \
             ./ghidra_decompiler/languages or set FISSION_SLA_DIR environment variable"
            .to_string())
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_string_search_size: 2 * MB, // 2MB
            min_string_length: 4,
            auto_xref_analysis: true,
            decompile_cache_size: DEFAULT_L1_CACHE_SIZE,
            function_address_range: MAX_HEX_READ, // 4KB range for function matching
        }
    }
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 10000,
            max_process_ids: 4096,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_performance: false,
            auto_scroll_entry: true,
            max_log_entries: 1000,
            hex_rows_per_page: 64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.decompiler.effective_num_workers() >= 1);
        assert!(config.decompiler.effective_num_workers() <= 8);
        assert_eq!(config.decompiler.default_function_size, 4096);
        assert_eq!(config.analysis.max_string_search_size, 256 * 1024);
    }
}
