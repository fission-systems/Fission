//! Fission CLI library.

#![allow(clippy::all)]
#![allow(unused_assignments)]

#[cfg(feature = "native_decomp")]
compile_error!(
    "feature 'native_decomp' is deprecated and blocked. Use the Rust-only decompiler pipeline instead."
);

pub mod cli;

pub use fission_core as core;
pub use fission_static::analysis;
