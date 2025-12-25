//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! This library provides the core functionality for binary analysis,
//! debugging, and decompilation.

#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

pub mod analysis;
pub mod core;
pub mod debug;
pub mod plugin;
pub mod script;
pub mod ui;

// Re-export core utilities at crate level for convenience
pub use crate::core::config;
pub use crate::core::constants;
pub use crate::core::errors;
pub use crate::core::logging;
pub use crate::core::prelude;

