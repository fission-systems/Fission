//! Fission - Next-Gen Dynamic Instrumentation Platform
//!
//! This library provides the core functionality for binary analysis,
//! debugging, and decompilation.

#![warn(clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::multiple_crate_versions)]
// Allow deprecated pyo3 methods (migration to be done separately)
#![allow(deprecated)]

#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub mod analysis;
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub mod app;
#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub mod debug;
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub mod plugin;
pub mod prelude;
#[allow(clippy::too_many_lines)]
pub mod script;
#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#[cfg(feature = "unpacker_runtime")]
pub mod unpacker;

pub use fission_core as core;

// Re-export core utilities at crate level for convenience
pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};

// NOTE: FFI functions are now exclusively exported through fission-ffi crate
// to maintain clear separation of concerns. Do not re-export FFI here.
