//! Pass trait and execution context for the action pipeline.

use crate::ir::{DecompFacts, DirFunction, NirBuildStats};
use super::budget::hir_shape;
use super::concept::{GhidraActionConcept, record_ghidra_action_stage};
use std::time::Instant;
use tracing::{debug, debug_span};

// TEMP DIAGNOSTIC: per-pass HIR body hash, gated by FISSION_NORM_TRACE.
// Bisects which normalize pass first diverges between a "good" and "bad"
// process run of the same ELF-format-specific nondeterminism repro (see
// PROJECT.md) -- FISSION_TEMP_TRACE already proved temp-name *allocation*
// is identical between runs, so the divergence must show up as a body hash
// change at some pass boundary here (this framework) or in the legacy
// free-function driver's own run_pass_logged (fission-midend-normalize::
// pipeline::run), which carries the same instrumentation.
pub fn norm_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_NORM_TRACE").is_some())
}

pub fn norm_trace_hash(func: &DirFunction) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Deliberately excludes callee_observed_max_arity/callee_summaries and
    // other bookkeeping fields -- hashing the *whole* DirFunction produced
    // a different hash on every run even when the final rendered output
    // was byte-identical, so something outside body/locals/params/
    // return_type has non-deterministic Debug output (a real bug on its
    // own, but a distraction from this one). Only hash what actually
    // determines the rendered text.
    format!("{:?}", (&func.body, &func.locals, &func.params, &func.return_type)).hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassOutcome {
    Changed,
    Unchanged,
}

impl PassOutcome {
    pub fn changed(self) -> bool {
        matches!(self, PassOutcome::Changed)
    }

    pub fn from_bool(changed: bool) -> Self {
        if changed {
            PassOutcome::Changed
        } else {
            PassOutcome::Unchanged
        }
    }
}

pub struct PassCtx<'a> {
    pub func: &'a mut DirFunction,
    pub perf: bool,
    pub diag: bool,
    pub stats: Option<&'a mut NirBuildStats>,
    pub decomp_facts: Option<&'a mut dyn DecompFacts>,
}

impl<'a> PassCtx<'a> {
    pub fn record_stage(&mut self, concept: GhidraActionConcept) {
        if let Some(stats) = self.stats.as_deref_mut() {
            record_ghidra_action_stage(stats, concept);
        }
    }
}

pub trait Pass {
    fn name(&self) -> &'static str;
    fn concept(&self) -> GhidraActionConcept;
    fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome;
}

/// Wraps an existing `fn(&mut DirFunction) -> bool` pass.
pub struct FnPass {
    pub name: &'static str,
    pub concept: GhidraActionConcept,
    pub f: fn(&mut DirFunction) -> bool,
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

pub fn fn_pass(
    name: &'static str,
    concept: GhidraActionConcept,
    f: fn(&mut DirFunction) -> bool,
) -> Box<dyn Pass> {
    Box::new(FnPass { name, concept, f })
}

/// Runs a pass with telemetry, timing, and wave stats recording.
pub fn run_pass_logged<P: Pass + ?Sized>(ctx: &mut PassCtx<'_>, pass: &P) -> PassOutcome {
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

    if norm_trace_enabled() {
        eprintln!(
            "[NORM_TRACE] fn={} pass={} changed={} hash={:016x}",
            ctx.func.name,
            pass_name,
            changed,
            norm_trace_hash(ctx.func)
        );
    }

    crate::wave_stats::add_pass_metric(
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
