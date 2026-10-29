//! Lifted P-code representation (`PcodeOp`, CFG helpers) and the [`optimizer`] pass bundle.
//!
//! Optimizer hooks and IR shapes are consumed by the midend in `fission-pcode` when building previews.

pub mod ir;
pub use ir::*;

// Sub-modules
pub mod graph;
pub mod optimizer;

#[cfg(test)]
pub mod graph_tests;
