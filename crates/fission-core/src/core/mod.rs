//! Core Utilities Module
//!
//! Contains fundamental utilities used across the entire codebase:
//! - config: Centralized configuration management
//! - path_config: Resource path resolution (FID, GDT, signatures)
//! - resources: `ResourceProvider` — single entry for runtime resource paths
//! - evidence_policy: Central numeric thresholds for identity / packed-score evidence
//! - constants: Magic bytes, offsets, and other constants
//! - errors: Unified error types and Result alias
//! - logging: Level-based logging with file output
//! - prelude: Common imports for convenience

pub mod calling_convention;
pub mod config;
pub mod config_store;
pub mod constants;
pub mod errors;
pub mod evidence_policy;
pub mod ghidra_no_return;
pub mod logging;
pub mod models;
pub mod path_config;
pub mod prelude;
pub mod resource_roots;
pub mod resources;
pub mod settings;
pub mod toml_config;
pub mod utils;

// Re-export commonly used items at the core level
pub use calling_convention::CallingConvention;
pub use config::{CONFIG, Config};
pub use constants::*;
pub use errors::{FissionError, Result};
pub use models::*;
pub use path_config::PATHS;
