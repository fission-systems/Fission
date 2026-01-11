// Core types
mod types;
pub use types::*;

// Sub-modules
pub mod graph;
pub mod optimizer;

// NOTE: FFI module has been moved to fission-ffi crate
// to maintain clear separation between safe Rust API and unsafe FFI boundary

#[cfg(test)]
pub mod graph_tests;
