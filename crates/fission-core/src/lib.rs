//! Fission Core
//!
//! Foundational utilities shared across crates.

pub mod common;
pub mod constants;
pub mod core;
pub mod plugin;

pub use crate::core::config;
pub use crate::core::config_store;
// Re-export core::constants as core_constants to avoid name collision
pub use crate::core::constants as core_constants;
pub use crate::core::errors;
pub use crate::core::logging;
pub use crate::core::models;
pub use crate::core::path_config;
pub use crate::core::prelude;
pub use crate::core::settings;

pub use crate::core::config::{LogConfig, LogLevel};
pub use crate::core::{CONFIG, FissionError, PATHS, Result};

// Commonly used standalone utilities
pub use crate::core::path_config::find_sla_dir;
pub use crate::core::utils::{format_addr, parse_address};
pub use crate::core_constants::{
    APP_DIR_NAME, CONFIG_FILENAME, DECOMP_CACHE_DIR_NAME, DEFAULT_COMPILER_ID,
    DEFAULT_DECOMP_MEMORY_LIMIT, DEFAULT_DECOMP_TIMEOUT_MS, DEFAULT_L1_CACHE_SIZE,
    DISASM_READ_WINDOW, FISSION_VERSION, MAX_FUNCTION_SIZE, MAX_HEX_READ,
    MAX_INSTRUCTIONS_PER_FUNCTION, MAX_SCAN_PER_SECTION, MAX_XREF_DECODE, MAX_XREF_INCOMING,
    MAX_XREF_OUTGOING, MIN_STRING_LENGTH, PAGE_SIZE, PLUGIN_DIR_NAME, SETTINGS_FILENAME,
    UNKNOWN_LIBRARY,
};
