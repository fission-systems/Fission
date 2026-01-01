//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! This library provides the core functionality for binary analysis,
//! debugging, and decompilation.

#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(dead_code)] # TODO: Remove this in production
#![allow(unused_imports)] # TODO: Remove this in production
#![allow(unused_variables)] # TODO: Remove this in production

pub mod analysis;
pub mod core;
pub mod debug;
pub mod parser;
pub mod plugin;
pub mod script;
pub mod debug_engine;
pub mod ui;

// Re-export core utilities at crate level for convenience
pub use crate::core::config;
pub use crate::core::constants;
pub use crate::core::errors;
pub use crate::core::logging;
pub use crate::core::prelude;

