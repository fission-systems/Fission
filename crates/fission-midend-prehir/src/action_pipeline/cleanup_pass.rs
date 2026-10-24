//! Reusable `Pass` primitives for the budget-gated "cleanup after a change"
//! idiom that recurs throughout hand-written normalize stage functions:
//! `if run_pass_logged(pass) { run_cleanup_block(...) }`.
//!
//! These let a stage's imperative if-chain be expressed as ordinary
//! [`ActionGroup`](super::ActionGroup) pass entries instead of free-function
//! control flow, one conditional chain at a time.

use super::budget::hir_shape;
use super::concept::GhidraActionConcept;
use super::gates::body_exceeds_early_cleanup_budget;
use super::pass::{Pass, PassCtx, PassOutcome, norm_trace_hash};
use crate::ir::PreHirFunction;

/// A structural cleanup block, admission-gated the same way
/// `run_cleanup_block` in normalize's legacy driver is: skipped (with
/// `wave_stats::add_cleanup_budget_skips` telemetry) once the body already
/// exceeds the early-cleanup stmt/block budget, otherwise run once and
/// reported `Changed` iff the stmt/local shape or body content moved.
pub struct CleanupPass {
    pub name: &'static str,
    pub concept: GhidraActionConcept,
    pub block: fn(&mut PreHirFunction),
}

impl Pass for CleanupPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn concept(&self) -> GhidraActionConcept {
        self.concept
    }

    fn run(&self, ctx: &mut PassCtx<'_, '_>) -> PassOutcome {
        if body_exceeds_early_cleanup_budget(&ctx.func.body) {
            fission_midend_core::wave_stats::add_cleanup_budget_skips(1);
            return PassOutcome::Unchanged;
        }
        let (before_stmts, before_locals) = hir_shape(ctx.func);
        let before_hash = norm_trace_hash(ctx.func);
        (self.block)(ctx.func);
        let (after_stmts, after_locals) = hir_shape(ctx.func);
        let changed = before_stmts != after_stmts
            || before_locals != after_locals
            || before_hash != norm_trace_hash(ctx.func);
        PassOutcome::from_bool(changed)
    }
}

pub fn cleanup_pass(
    name: &'static str,
    concept: GhidraActionConcept,
    block: fn(&mut PreHirFunction),
) -> Box<dyn Pass> {
    Box::new(CleanupPass {
        name,
        concept,
        block,
    })
}

/// Runs `then` (in order) only when `cond` itself reports `Changed`. Mirrors
/// `if run_pass_logged(func, name, perf, cond_fn) { ...then... }`, as a
/// single composable `Pass` an `ActionGroup` can register directly.
///
/// `cond` is run un-wrapped (the enclosing `ActionGroup` already logs the
/// whole `GatedFollowupPass` under `cond`'s name via `run_pass_logged`); each
/// `then` step is executed directly, preventing double-counting of telemetry.
pub struct GatedFollowupPass {
    pub cond: Box<dyn Pass>,
    pub then: Vec<Box<dyn Pass>>,
}

impl Pass for GatedFollowupPass {
    fn name(&self) -> &'static str {
        self.cond.name()
    }

    fn concept(&self) -> GhidraActionConcept {
        self.cond.concept()
    }

    fn run(&self, ctx: &mut PassCtx<'_, '_>) -> PassOutcome {
        let cond_changed = self.cond.run(ctx).changed();
        if cond_changed {
            for step in &self.then {
                step.run(ctx);
            }
        }
        PassOutcome::from_bool(cond_changed)
    }
}

pub fn gated_followup(cond: Box<dyn Pass>, then: Vec<Box<dyn Pass>>) -> Box<dyn Pass> {
    Box::new(GatedFollowupPass { cond, then })
}

/// Runs `inner` only when `admits(func)` holds; otherwise records the skip
/// via `on_skip()` (if given) and reports `Unchanged` without touching
/// `func` or logging `inner`. Mirrors the
/// `let eligible = admission_summary(&func.body).eligible; if eligible { .. }`
/// data-driven admission-gate idiom (e.g. SCCP eligibility) as a single
/// composable `Pass`.
pub struct AdmissionGatedPass {
    pub admits: fn(&PreHirFunction) -> bool,
    pub on_skip: Option<fn()>,
    pub inner: Box<dyn Pass>,
}

impl Pass for AdmissionGatedPass {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn concept(&self) -> GhidraActionConcept {
        self.inner.concept()
    }

    fn run(&self, ctx: &mut PassCtx<'_, '_>) -> PassOutcome {
        if !(self.admits)(ctx.func) {
            if let Some(on_skip) = self.on_skip {
                on_skip();
            }
            return PassOutcome::Unchanged;
        }
        self.inner.run(ctx)
    }
}

pub fn admission_gated(
    admits: fn(&PreHirFunction) -> bool,
    on_skip: Option<fn()>,
    inner: Box<dyn Pass>,
) -> Box<dyn Pass> {
    Box::new(AdmissionGatedPass {
        admits,
        on_skip,
        inner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{PreHirExpr, PreHirLValue, PreHirStmt};

    #[test]
    fn cleanup_pass_detects_in_place_mutation_with_constant_counts() {
        fn rewrite_constant(func: &mut PreHirFunction) {
            if let Some(PreHirStmt::Assign { rhs, .. }) = func.body.first_mut() {
                *rhs = PreHirExpr::Const(42, crate::ir::NirType::Bool);
            }
        }

        let pass = CleanupPass {
            name: "test_cleanup",
            concept: GhidraActionConcept::Normalize,
            block: rewrite_constant,
        };

        let mut func = PreHirFunction {
            name: "test_fn".to_string(),
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(1, crate::ir::NirType::Bool),
            }],
            ..Default::default()
        };

        let mut ctx = PassCtx {
            func: &mut func,
            perf: false,
            diag: false,
            stats: None,
            decomp_facts: None,
        };

        let outcome = pass.run(&mut ctx);
        assert_eq!(outcome, PassOutcome::Changed);
    }
}
