//! Pass trait and execution context for the action pipeline.

use super::super::ir::{DecompFacts, HirFunction, NirBuildStats};
use super::budget::hir_shape;
use super::concept::{GhidraActionConcept, record_ghidra_action_stage};
use std::time::Instant;
use tracing::{debug, debug_span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassOutcome {
    Changed,
    Unchanged,
}

impl PassOutcome {
    pub(crate) fn changed(self) -> bool {
        matches!(self, PassOutcome::Changed)
    }

    pub(crate) fn from_bool(changed: bool) -> Self {
        if changed {
            PassOutcome::Changed
        } else {
            PassOutcome::Unchanged
        }
    }
}

pub(crate) struct PassCtx<'a> {
    pub func: &'a mut HirFunction,
    pub perf: bool,
    pub diag: bool,
    pub stats: Option<&'a mut NirBuildStats>,
    pub decomp_facts: Option<&'a mut dyn DecompFacts>,
}

impl<'a> PassCtx<'a> {
    pub(crate) fn record_stage(&mut self, concept: GhidraActionConcept) {
        if let Some(stats) = self.stats.as_deref_mut() {
            record_ghidra_action_stage(stats, concept);
        }
    }
}

pub(crate) trait Pass {
    fn name(&self) -> &'static str;
    fn concept(&self) -> GhidraActionConcept;
    fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome;
}

/// Wraps an existing `fn(&mut HirFunction) -> bool` pass.
pub(crate) struct FnPass {
    pub(crate) name: &'static str,
    pub(crate) concept: GhidraActionConcept,
    pub(crate) f: fn(&mut HirFunction) -> bool,
}

impl Pass for FnPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn concept(&self) -> GhidraActionConcept {
        self.concept
    }

    fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome {
        PassOutcome::from_bool((self.f)(ctx.func))
    }
}

pub(crate) fn fn_pass(
    name: &'static str,
    concept: GhidraActionConcept,
    f: fn(&mut HirFunction) -> bool,
) -> Box<dyn Pass> {
    Box::new(FnPass { name, concept, f })
}

/// Runs a pass with telemetry, timing, and wave stats recording.
pub(crate) fn run_pass_logged<P: Pass + ?Sized>(ctx: &mut PassCtx<'_>, pass: &P) -> PassOutcome {
    let pass_name = pass.name();
    let _span = debug_span!(
        "normalize_pass",
        fn_name = %ctx.func.name,
        pass = pass_name
    )
    .entered();

    let (before_stmts, before_locals) = hir_shape(ctx.func);
    let start = Instant::now();
    let outcome = pass.run(ctx);
    let (after_stmts, after_locals) = hir_shape(ctx.func);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let changed = outcome.changed();

    crate::midend::wave_stats::add_pass_metric(
        pass_name,
        elapsed_ms,
        changed,
        before_stmts,
        after_stmts,
        before_locals,
        after_locals,
    );

    debug!(
        changed,
        elapsed_ms,
        stmts_reduced = (before_stmts as isize - after_stmts as isize),
        locals_reduced = (before_locals as isize - after_locals as isize),
        "pass completed"
    );

    if ctx.perf {
        eprintln!(
            "[PERF] normalize pass: fn={} pass={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
            ctx.func.name,
            pass_name,
            changed,
            elapsed_ms,
            before_stmts,
            after_stmts,
            before_locals,
            after_locals,
        );
    }

    outcome
}
