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
pub mod arch;
pub(crate) mod fast_hash;
pub mod nir;
mod pcode;
pub mod prelude;

// Re-export main P-code types
pub use pcode::*;

// Re-export optimizer
pub use nir::{
    AdmissionClass, CallEdgeKind, CallEffectSummarySource, CallSummary, CallTargetProvenance,
    CallTargetRef, CallingConvention, FormatFamily, HirExpr, HirFunction, HirStmt,
    IndirectControlClassification, MlilPreviewError, MlilPreviewOptions, NirAdmissionFacts,
    NirBindingOrigin, NirBlock, NirBuildStats, NirCallEffectSummary, NirCallParamRule, NirFunction,
    NirFunctionHints, NirHintStats, NirRenderOptions, NirTerminator, NirType, NirTypeContext,
    NirValueId, PreviewBuildStats, PreviewCallParamRule, PreviewFunctionHints, PreviewHintStats,
    PreviewTypeContext, RecoveryMode, StackSlotId, StructuringBudgetClass, StructuringEngineKind,
    StructuringFailureKind, StructuringOutcome, StructuringReasonFamily, TargetProfile,
    parse_call_target_address, render_mlil_preview, render_mlil_preview_with_binary_and_context,
    render_mlil_preview_with_context, render_nir, render_nir_with_binary_and_context,
    render_nir_with_context, structuring_outcome_for_signature, take_last_nir_build_stats,
    take_last_nir_hint_stats, take_last_preview_build_stats, take_last_preview_hint_stats,
};
pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
