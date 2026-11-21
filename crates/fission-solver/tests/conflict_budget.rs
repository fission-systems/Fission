//! A search that is told to stop says it stopped.
//!
//! `Solver::set_conflict_budget` exists so that a caller with a deadline --
//! and the z3 differential, which cannot afford a formula that takes this
//! solver an afternoon -- gets `Unknown` instead of a wait. `Unknown` is
//! not a verdict, and none of the questions built on `check_sat` may read it
//! as one.

use fission_solver::cnf::Lit;
use fission_solver::sat::SatSolver;
use fission_solver::{SatResult, Solver, SymExpr, UnknownReason};

/// `x * y != y * x`: unsatisfiable, and a search has to earn it -- there is
/// no assignment to stumble on, and the two multipliers share no gates.
fn multiplication_does_not_commute(width: u32) -> SymExpr {
    let (x, y) = (SymExpr::new_var("x", width), SymExpr::new_var("y", width));
    SymExpr::Neq(
        Box::new(SymExpr::Mul(Box::new(x.clone()), Box::new(y.clone()))),
        Box::new(SymExpr::Mul(Box::new(y), Box::new(x))),
    )
}

#[test]
fn a_query_that_runs_out_of_conflicts_is_unknown_and_says_why() {
    let mut solver = Solver::new();
    solver.set_conflict_budget(Some(10));
    solver.assert(multiplication_does_not_commute(6));
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unknown);
    assert_eq!(solver.last_unknown(), Some(&UnknownReason::ConflictBudget));

    // Giving up is not a state: the same solver, allowed to finish, does --
    // and this is also the control, that ten conflicts were too few rather
    // than the problem being one this solver cannot decide.
    solver.set_conflict_budget(None);
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unsat);
    assert_eq!(solver.last_unknown(), None);
}

#[test]
fn a_budget_that_is_enough_changes_nothing() {
    let mut solver = Solver::new();
    solver.set_conflict_budget(Some(1_000_000));
    solver.assert(multiplication_does_not_commute(6));
    assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unsat);
    assert_eq!(solver.last_unknown(), None);
}

/// "Could not tell" must not read as "always": `is_true` needs a refutation,
/// and a search that stopped early has not produced one.
#[test]
fn a_question_that_ran_out_is_not_proven() {
    let (x, y) = (SymExpr::new_var("x", 6), SymExpr::new_var("y", 6));
    let commutes = SymExpr::Eq(
        Box::new(SymExpr::Mul(Box::new(x.clone()), Box::new(y.clone()))),
        Box::new(SymExpr::Mul(Box::new(y), Box::new(x))),
    );

    let mut solver = Solver::new();
    solver.set_conflict_budget(Some(10));
    assert!(!solver.is_true(&commutes), "ten conflicts prove nothing");
    assert_eq!(solver.last_unknown(), Some(&UnknownReason::ConflictBudget));
    assert!(!solver.is_false(&commutes));

    solver.set_conflict_budget(None);
    assert!(solver.is_true(&commutes), "and it is in fact always true");
    assert_eq!(solver.last_unknown(), None);
}

/// Pigeonhole, `pigeons` into `holes`: the classic problem with no short
/// refutation.
fn pigeonhole(sat: &mut SatSolver, pigeons: u32, holes: u32) {
    let var = |p: u32, h: u32| p * holes + h + 1;
    for p in 0..pigeons {
        sat.add_clause((0..holes).map(|h| Lit::new(var(p, h), false)).collect());
    }
    for h in 0..holes {
        for p in 0..pigeons {
            for q in p + 1..pigeons {
                sat.add_clause(vec![Lit::new(var(p, h), true), Lit::new(var(q, h), true)]);
            }
        }
    }
}

#[test]
fn the_sat_core_reports_no_verdict_and_stays_usable() {
    let mut sat = SatSolver::new();
    pigeonhole(&mut sat, 7, 6);
    sat.set_conflict_budget(Some(5));
    assert_eq!(sat.solve_limited(None, &[]), None, "five conflicts");
    // The entry points without a budget in their type never give up.
    assert!(!sat.solve(), "seven pigeons do not fit six holes");

    // And a satisfiable problem interrupted the same way is still satisfiable.
    let mut sat = SatSolver::new();
    pigeonhole(&mut sat, 6, 6);
    sat.set_conflict_budget(Some(0));
    let limited = sat.solve_limited(None, &[]);
    assert_ne!(limited, Some(false), "a budget never makes a problem UNSAT");
    assert!(sat.solve());
}
