//! Fission P-code — intermediate representation and optimizer.
//!
//! Pipeline stages and conventions live under [`crate::nir`] with directory guides in
//! `crates/fission-pcode/src/nir/AGENTS.md` and child `*/AGENTS.md` files.

// CI runs `cargo clippy ... -D warnings`; `-D warnings` cannot be selectively reversed via `-A clippy::*`
// on the command line for all lint kinds, so suppress Clippy for this crate until policy is tightened again.
#![allow(clippy::all)]
// Large IR/structuring surface carries staged helpers and telemetry enums; rustc `-D warnings` still applies unless relaxed here.
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod arch;
pub mod cfg;
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
    NirBindingOrigin, NirBlock, NirBuildStats, NirCallEffectSummary, NirCallParamRule,
    NirCallPrototypeSummary, NirFunction, NirFunctionHints, NirHintStats, NirRenderOptions,
    NirTerminator, NirType, NirTypeContext, NirValueId, PreviewBuildStats, PreviewCallParamRule,
    PreviewFunctionHints, PreviewHintStats, PreviewTypeContext, ProcedureSummary, RecoveryMode,
    RegisterNamer, StackSlotId, StructuringBudgetClass, StructuringEngineKind,
    StructuringFailureKind, StructuringOutcome, StructuringReasonFamily, TargetProfile,
    WrapperClass, infer_entry_register_param_arity, parse_call_target_address,
    render_contracted_wrapper_summary, render_mlil_preview,
    render_mlil_preview_with_binary_and_context, render_mlil_preview_with_context, render_nir,
    render_nir_with_binary_and_context, render_nir_with_context, structuring_outcome_for_signature,
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
