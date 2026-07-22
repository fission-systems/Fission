//! Solver-backed DIR/HIR symbolic equivalence (the symbolic tier).
//!
//! Orchestrates [`crate::lower_sym`] (AST -> `SymExpr`) and
//! `fission_solver::Solver` (the real SAT/SMT-style solver): lower DIR's and
//! HIR's return value to one closed-form `SymExpr` each over a *shared* set
//! of parameter `Var`s, assert they're unequal, and ask the solver to
//! decide. `Unsat` is a genuine proof of equivalence (for the modeled
//! subset -- see [`crate::lower_sym`]'s scope notes), not "no counterexample
//! found among a few samples" the way [`crate::diff`]'s boundary-value
//! sweep is. `Sat` gives a real counterexample -- the exact input the
//! solver found the two sides diverge on, immediately replayable through
//! [`crate::ground_truth`] to see what the real machine actually computes
//! there.

use fission_midend_core::ir::HirFunction;
use fission_midend_dir::DirFunction;
use fission_solver::Solver;
use fission_solver::ast::SymExpr;
use fission_solver::solver::SatResult;

use crate::eval::width_of;
use crate::lower_sym;
use crate::report::UnsupportedReason;

/// A solver-found counterexample: DIR and HIR provably diverge on `args`
/// (converted from the solver's raw `u64` model values by the caller, which
/// knows each parameter's declared signedness/width).
pub struct Counterexample {
    pub args: Vec<u64>,
}

pub enum SymbolicOutcome {
    /// `Unsat` -- proved equivalent for the modeled subset.
    Equivalent,
    /// `Sat` -- a genuine, solver-found counterexample.
    Diverged(Counterexample),
    /// Not checkable at this tier (contains a loop/unsupported construct),
    /// or the solver returned `Unknown`.
    Unsupported(UnsupportedReason),
}

/// Check whether `dir` and `hir` compute the same return value for every
/// input, for the symbolic tier's modeled subset (see [`crate::lower_sym`]'s
/// scope notes -- acyclic, `Bool`/`Int` only, no `Div`/`Mod`/`Sar`).
pub fn check_symbolic_equivalence(dir: &DirFunction, hir: &HirFunction) -> SymbolicOutcome {
    if lower_sym::dir::contains_loop(&dir.body) || lower_sym::hir::contains_loop(&hir.body) {
        return SymbolicOutcome::Unsupported(UnsupportedReason::ContainsLoop);
    }
    if dir.params.len() != hir.params.len() {
        return SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
            "DIR and HIR have a different parameter count -- can't share one set of symbolic \
             argument variables between them",
        ));
    }

    // One shared `SymExpr::Var` per parameter index -- fed into *both*
    // DIR-side and HIR-side lowering. This is what makes "same input"
    // meaningful: two independently-minted Vars would make the equivalence
    // assertion vacuous (nothing would ever force them to represent the
    // same concrete value).
    let param_vars: Vec<SymExpr> = hir
        .params
        .iter()
        .map(|p| SymExpr::new_var(&p.name, width_of(&p.ty).clamp(1, 64)))
        .collect();

    let dir_result = match lower_sym::dir::lower(&dir.body, &dir.params, &dir.locals, &param_vars) {
        Ok(e) => e,
        Err(err) => {
            tracing::debug!("lower_sym: DIR side unsupported: {err}");
            return SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
                "DIR side uses a construct the symbolic tier doesn't model (see debug logs)",
            ));
        }
    };
    let hir_result = match lower_sym::hir::lower(&hir.body, &hir.params, &hir.locals, &param_vars) {
        Ok(e) => e,
        Err(err) => {
            tracing::debug!("lower_sym: HIR side unsupported: {err}");
            return SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
                "HIR side uses a construct the symbolic tier doesn't model (see debug logs)",
            ));
        }
    };

    // Both sides' results may have different bit widths if DIR/HIR declare
    // different return types for the same logical value -- compare over
    // the wider of the two, zero-extending the narrower (a real width
    // mismatch would then show up as a genuine divergence, not a panic).
    let dir_bits = sym_bits(&dir_result);
    let hir_bits = sym_bits(&hir_result);
    let cmp_bits = dir_bits.max(hir_bits);
    let dir_cmp = zext_to(dir_result, dir_bits, cmp_bits);
    let hir_cmp = zext_to(hir_result, hir_bits, cmp_bits);

    let mut solver = Solver::new();
    for v in &param_vars {
        // Registers the Var's node so `get_value` can read a counterexample
        // model back for it after a `Sat` result.
        solver.register_node(v.clone());
    }
    solver.assert(SymExpr::new_neq(dir_cmp, hir_cmp));

    match solver.check_sat() {
        Ok(SatResult::Unsat) => SymbolicOutcome::Equivalent,
        Ok(SatResult::Sat) => {
            let args = param_vars
                .iter()
                .map(|v| match v {
                    SymExpr::Var { id, .. } => solver.get_value(*id).unwrap_or(0),
                    _ => 0,
                })
                .collect();
            SymbolicOutcome::Diverged(Counterexample { args })
        }
        Ok(SatResult::Unknown) => SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
            "solver returned Unknown -- should be rare/never with no memory theory engaged",
        )),
        Err(err) => {
            tracing::debug!("lower_sym: solver error: {err}");
            SymbolicOutcome::Unsupported(UnsupportedReason::Construct("solver query failed"))
        }
    }
}

fn sym_bits(e: &SymExpr) -> u32 {
    use fission_solver::ast::Sort;
    match e.get_sort() {
        Sort::BitVector(bits) => bits,
        _ => 64,
    }
}

fn zext_to(e: SymExpr, from_bits: u32, to_bits: u32) -> SymExpr {
    if to_bits <= from_bits {
        return e;
    }
    let extra = to_bits - from_bits;
    SymExpr::Concat(Box::new(SymExpr::Const { val: 0, size: extra }), Box::new(e))
}
