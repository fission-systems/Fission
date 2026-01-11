//! Fission Core
//!
//! Foundational utilities shared across crates.

pub mod core;

pub use crate::core::config;
pub use crate::core::config_store;
pub use crate::core::constants;
pub use crate::core::errors;
pub use crate::core::logging;
pub use crate::core::models;
pub use crate::core::prelude;
pub use crate::core::settings;

pub use crate::core::{CONFIG, FissionError, Result};
