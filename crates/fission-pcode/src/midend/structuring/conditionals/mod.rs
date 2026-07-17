//! Conditional structuring re-exports (ADR 0012).
//!
//! Free-function owners: `fission-midend-structuring::conditionals`.
//! Collapse dispatch calls them via `apply_collapse_rule` — no PreviewBuilder
//! inherent thin wraps.

pub use fission_midend_structuring::conditionals::is_trivial_structuring_stmt;
pub use fission_midend_structuring::{
    try_lower_if, try_lower_if_else, try_lower_return_chain_arm, try_lower_short_circuit_and,
    try_lower_short_circuit_and_else, try_lower_short_circuit_if, try_lower_short_circuit_or,
    try_reduce_if_else_with_follow,
};
