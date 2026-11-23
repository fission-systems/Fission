//! SLEIGH-runtime function discovery.
//!
//! This module owns analyzer-level function discovery. `fission-loader` only
//! contributes authoritative binary metadata; direct-control-flow recovery is
//! derived from decoded instructions here.

mod candidate_validation;
mod control_flow_scan;
mod coverage_decode;
mod data_references;
mod ghidra_patterns;
mod instruction_boundaries;
mod load_config;
mod msvc_eh;
mod pipeline;
pub(crate) mod ranges;
mod tail_calls;
pub(crate) mod targets;
mod thumb;
mod types;

pub use pipeline::discover_functions_with_runtime;
pub use thumb::{decode_context_for_address, image_executes_thumb};
pub use types::FunctionDiscoveryReport;

#[cfg(test)]
mod tests;

fn discovery_diag_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_DISCOVERY_DIAG").is_some())
}
