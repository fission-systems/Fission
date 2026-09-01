//! SLEIGH-runtime function discovery.
//!
//! This module owns analyzer-level function discovery. `fission-loader` only
//! contributes authoritative binary metadata; direct-control-flow recovery is
//! derived from decoded instructions here.

mod discover;
mod load_config;
mod msvc_eh;
pub(crate) mod ranges;
pub(crate) mod targets;
mod thumb;
mod types;

pub use discover::discover_functions_with_runtime;
pub use thumb::{decode_context_for_address, image_executes_thumb};
pub use types::{FunctionDiscoveryProfile, FunctionDiscoveryReport};

#[cfg(test)]
mod tests;
