//! Fission Configuration
//!
//! Centralized configuration for all tunable parameters.
//! All magic numbers and hardcoded values should be defined here.

use std::sync::LazyLock;

/// Global configuration instance
pub static CONFIG: LazyLock<Config> = LazyLock::new(Config::default);

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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            decompiler: DecompilerConfig::default(),
            analysis: AnalysisConfig::default(),
            debug: DebugConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

impl Default for DecompilerConfig {
    fn default() -> Self {
        Self {
            num_workers: 0, // 0 = auto
            max_workers: 8,
            default_function_size: 4096, // 4KB
            max_function_size: 64 * 1024, // 64KB
            min_function_size: 16,
            timeout_ms: 30000, // 30 seconds
            enable_prefetch: true,
            prefetch_count: 3,
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
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            max_string_search_size: 256 * 1024, // 256KB
            min_string_length: 4,
            auto_xref_analysis: true,
            decompile_cache_size: 100,
            function_address_range: 4096, // 4KB range for function matching
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
