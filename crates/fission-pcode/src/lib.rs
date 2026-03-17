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
pub mod nir;
mod pcode;
pub mod prelude;

// Re-export main P-code types
pub use pcode::*;

// Re-export optimizer
pub use nir::{
    HirExpr, HirFunction, HirStmt, MlilPreviewError, MlilPreviewOptions, NirBlock, NirFunction,
    NirTerminator, NirType, NirValueId, PreviewBuildStats, PreviewCallParamRule,
    PreviewTypeContext, StackSlotId, render_mlil_preview, render_mlil_preview_with_context,
    take_last_preview_build_stats,
};
pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
