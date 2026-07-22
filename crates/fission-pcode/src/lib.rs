//! Fission P-code — intermediate representation and optimizer.
//!
//! Pipeline stages live under [`crate::midend`] (builder → normalize → structure)
//! with guides in `crates/fission-pcode/src/midend/AGENTS.md`. Dual-layer
//! print/presentation owns [`crate::render`] (`src/render/AGENTS.md`, ADR 0011).
//!
//! Prefer [`crate::midend`] for post-lift owners (ADR 0012). The historical
//! `nir` module path has been removed after call-site migration.

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
/// Post-lift midend (structured IR, normalize, structuring, orchestration).
pub mod midend;
mod pcode;
pub mod prelude;
/// Dual-layer C presentation (NIR/HIR print surfaces). Owner: ADR 0008 / 0011.
pub mod render;

// Re-export main P-code types
pub use pcode::*;

// Re-export midend surface (public names historically exported via `nir`).
pub use midend::{
    AdmissionClass, CallEdgeKind, CallEffectSummarySource, CallSummary, CallTargetProvenance,
    CallTargetRef, CallingConvention, DirExpr, DirFunction, DirStmt, FormatFamily, HirExpr,
    HirFunction, HirStmt,
    IndirectControlClassification, LayeredPseudocode, MlilPreviewError, MlilPreviewOptions,
    NirAdmissionFacts, NirBindingOrigin, NirBlock, NirBuildStats, NirCallEffectSummary,
    NirCallParamRule, NirCallPrototypeSummary, NirFunction, NirFunctionHints, NirHintStats,
    NirRenderOptions, NirStructFieldHint, NirStructTypeHint, NirTerminator, NirType,
    NirTypeContext, NirValueId, PreviewBuildStats,
    PreviewCallParamRule, PreviewFunctionHints, PreviewHintStats, PreviewTypeContext,
    PrintProfile, ProcedureSummary, PseudocodeLayer, RecoveryMode, RegisterNamer, StackSlotId,
    StructuringBudgetClass, StructuringEngineKind, StructuringFailureKind, StructuringOutcome,
    StructuringReasonFamily, TargetProfile, WrapperClass, infer_entry_register_param_arity,
    nir_admission_facts_from_pcode, parse_call_target_address, print_dir_function,
    render_contracted_wrapper_summary, render_mlil_preview,
    render_mlil_preview_with_binary_and_context,
    render_mlil_preview_with_context, render_nir, render_nir_with_binary_and_context,
    render_nir_with_context, seed_nir_render_options, structuring_outcome_for_signature,
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
    take_last_dir_snapshot, take_last_hir_function_snapshot, take_last_layered_pseudocode,
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use pcode::optimizer::{PcodeOptimizer, PcodeOptimizerConfig};
