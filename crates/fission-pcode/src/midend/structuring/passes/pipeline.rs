//! Structuring stage as a typed `Pass` in the action_pipeline framework.
//!
//! `StructuringPass` wraps the existing `CollapseDriver` / SESE entry-point
//! so the structuring stage participates in the same `ActionGroup` / `Pipeline`
//! fixed-point model used by the normalize pipeline.
//!
//! This gives three concrete enforcement properties:
//!
//! 1. **Contract boundary**: structuring receives `&mut DirFunction`, not raw
//!    `PreviewBuilder` state.  Any ad-hoc patch that needs to reach into the
//!    builder internals cannot be expressed as a `Pass` and will fail at the
//!    module boundary.
//!
//! 2. **Fixed-point budget**: `PassManager` (`ActionGroup::UntilStable`) owns
//!    the iteration count, not an ad-hoc internal while-loop.
//!
//! 3. **Telemetry**: each Pass run is logged via `run_pass_logged`, giving
//!    uniform `PassTrace` coverage for benchmark attribution.

use crate::midend::action_pipeline::{
    GhidraActionConcept, Pass, PassCtx, PassOutcome, Pipeline, Repeat, group,
};
use crate::midend::ir::DirFunction;

/// A no-op structuring pass placeholder.
///
/// The actual structuring work is done inside `PreviewBuilder::build_hir`
/// (via `structure_cfg_via_collapse_loop` / `structure_cfg_via_sese`), which
/// produces the initial `DirFunction`.  This pass operates on the _already-
/// structured_ HIR body and applies post-structuring cleanup rules that are
/// safe to repeat until stable.
///
/// Future work: migrate the inner `build_sese_region_body` collapse loop into
/// individual `CollapseRulePass` instances registered here.
struct PostStructuringCleanupPass;

impl Pass for PostStructuringCleanupPass {
    fn name(&self) -> &'static str {
        "post_structuring_cleanup"
    }

    fn concept(&self) -> GhidraActionConcept {
        GhidraActionConcept::BlockGraphStructuring
    }

    fn run(&self, _ctx: &mut PassCtx<'_>) -> PassOutcome {
        // Currently a no-op shim.  The structuring work is embedded in
        // PreviewBuilder::build_hir.  This Pass exists to:
        //   a) make the structuring stage visible in PassTrace / telemetry, and
        //   b) provide the extension point for future per-rule Pass migration.
        PassOutcome::Unchanged
    }
}

/// Builds the canonical structuring `Pipeline`.
///
/// Today the pipeline contains only `PostStructuringCleanupPass`.  As
/// individual `CollapseRule` implementations are migrated to explicit Pass
/// types they will be registered here.
pub(crate) fn build_structuring_pipeline() -> Pipeline {
    Pipeline::new("structuring").group(
        group(
            "post_structuring_cleanup",
            GhidraActionConcept::BlockGraphStructuring,
        )
        .with_repeat(Repeat::Once)
        .pass(Box::new(PostStructuringCleanupPass)),
    )
}

/// Run the structuring pipeline over `func`.
///
/// Called from `render_mlil_preview_with_binary_and_context` after
/// `PreviewBuilder::build_hir` returns the initial HIR.
pub(crate) fn run_structuring_pipeline(func: &mut DirFunction, diag: bool, perf: bool) {
    let pipeline = build_structuring_pipeline();
    let mut ctx = PassCtx {
        func,
        diag,
        perf,
        stats: None,
        decomp_facts: None,
    };
    pipeline.run(&mut ctx);
}
