//! Fission static analysis and decompilation engine.

#![allow(clippy::all)]

#[cfg(feature = "native_decomp")]
compile_error!(
    "feature 'native_decomp' is deprecated and blocked. Use the Rust-only NIR pipeline."
);

#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub mod analysis;
pub mod prelude;
pub mod utils;

pub use fission_core as core;
pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};
