//! SLEIGH-runtime function discovery.
//!
//! This module owns analyzer-level function discovery. `fission-loader` only
//! contributes authoritative binary metadata; direct-control-flow recovery is
//! derived from decoded instructions here.

mod discover;
pub(crate) mod ranges;
pub(crate) mod targets;
mod types;

pub use discover::discover_functions_with_runtime;
pub use types::{FunctionDiscoveryProfile, FunctionDiscoveryReport};

#[cfg(test)]
mod tests;
