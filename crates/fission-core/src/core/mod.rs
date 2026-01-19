//! Core Utilities Module
//!
//! Contains fundamental utilities used across the entire codebase:
//! - config: Centralized configuration management
//! - path_config: Resource path resolution (FID, GDT, signatures)
//! - constants: Magic bytes, offsets, and other constants
//! - errors: Unified error types and Result alias
//! - logging: Level-based logging with file output
//! - prelude: Common imports for convenience

pub mod config;
pub mod config_store;
pub mod constants;
pub mod errors;
pub mod logging;
pub mod models;
pub mod path_config;
pub mod prelude;
pub mod settings;
pub mod toml_config;
pub mod utils;

// Re-export commonly used items at the core level
pub use config::{CONFIG, Config};
pub use constants::*;
pub use errors::{FissionError, Result};
pub use models::*;
pub use path_config::PATHS;
