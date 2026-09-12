//! Binary inventory helpers for Rhai host bindings.

pub mod binary;

/// A live emulated machine: the half a one-shot command line cannot reach.
#[cfg(feature = "emulator")]
pub mod machine;
