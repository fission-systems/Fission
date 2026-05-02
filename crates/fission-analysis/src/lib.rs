//! Fission compatibility facade.
//!
//! This crate preserves the historical `fission-analysis` public paths while
//! the implementation is split into dedicated crates:
//! - `fission-static`
//! - `fission-dynamic`
//! - private product/AI layers kept outside this compatibility crate

#![allow(clippy::all)]

#[cfg(feature = "native_decomp")]
compile_error!(
    "feature 'native_decomp' is deprecated and blocked. Use fission-static Rust-only paths."
);

pub mod prelude;

pub use fission_core as core;

pub use fission_core::{config, constants, errors, logging, prelude as core_prelude, settings};
pub use fission_static::{analysis, utils};

#[cfg(feature = "unpacker_runtime")]
pub use fission_dynamic::unpacker;
#[cfg(feature = "interactive_runtime")]
pub use fission_dynamic::{app, debug, plugin};
