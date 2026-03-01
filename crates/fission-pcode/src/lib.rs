//! Fission P-code - Intermediate representation and optimizer
//!
//! This crate provides the P-code IR (intermediate representation) used for
//! binary analysis and decompilation, along with optimization passes.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]

// Re-export fission-disasm directly (no wrapper needed)
pub use fission_disasm as disasm;
mod pcode;
pub mod prelude;

// Re-export main P-code types
pub use pcode::*;

// Re-export optimizer
pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
