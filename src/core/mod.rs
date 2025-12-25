//! Core Utilities Module
//!
//! Contains fundamental utilities used across the entire codebase:
//! - config: Centralized configuration management
//! - constants: Magic bytes, offsets, and other constants
//! - errors: Unified error types and Result alias
//! - logging: Level-based logging with file output
//! - prelude: Common imports for convenience

pub mod config;
pub mod constants;
pub mod errors;
pub mod events;
pub mod modules;
pub mod config_store;
pub mod logging;
pub mod prelude;

// Re-export commonly used items at the core level
pub use config::CONFIG;
pub use errors::{FissionError, Result};
pub use constants::*;
