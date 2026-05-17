//! Lifted P-code representation (`PcodeOp`, CFG helpers) and the [`optimizer`] pass bundle.
//!
//! Safe Rust boundary here; FFI to Ghidra-era surfaces lives in the separate `fission-ffi` crate.
//!
//! Optimizer hooks and IR shapes are consumed by [`crate::nir`] when building previews.

pub mod ir;
pub use ir::*;

// Sub-modules
pub mod graph;
pub mod optimizer;

// NOTE: FFI module has been moved to fission-ffi crate
// to maintain clear separation between safe Rust API and unsafe FFI boundary

#[cfg(test)]
pub mod graph_tests;
