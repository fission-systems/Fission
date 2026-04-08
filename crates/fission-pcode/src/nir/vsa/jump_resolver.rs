/// Jump table resolution using VSA range information.
///
/// After solving ranges for all variables in a function, this module
/// attempts to refine `HirStmt::Switch` expressions by:
///
/// 1. Detecting switch index variables whose range is `[0, N)`.
/// 2. Propagating the inferred case count down to switches that were
///    previously emitted with a synthetic contiguous range.
/// 3. Dead-branch elimination: if the switch discriminant is a singleton,
///    replace the entire switch with the matching case body.
///
/// References:
/// - Ghidra `jumptable.hh`: `JumpModel`, `PathMeld`
/// - Ghidra `rangeutil.hh`: `CircleRange`, `ValueSetSolver`
use super::solver::solve;
use super::transfer::{eval_expr, RangeEnv};
use crate::nir::{HirFunction, HirStmt};

/// Apply VSA-based switch refinement to a function's body.
///
/// Returns `true` if any changes were made.
pub(crate) fn apply_jump_resolver_pass(func: &mut HirFunction) -> bool {
    let env = solve(func);
    refine_stmts(&mut func.body, &env)
}

fn refine_stmts(stmts: &mut Vec<HirStmt>, env: &RangeEnv) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if refine_stmt(&mut stmts[i], env) {
            changed = true;
        }
        // If the stmt was reduced to a singleton-constant switch,
        // inline the matching case in place.
        if let HirStmt::Switch { expr, cases, default } = &stmts[i] {
            let range = eval_expr(expr, env);
            if let Some(v) = range.singleton_value() {
                let v_signed = v as i64;
                // Find the matching case.
                let replacement = cases.iter().find(|c| c.values.contains(&v_signed))
                    .map(|c| c.body.clone())
                    .unwrap_or_else(|| default.clone());
                stmts.splice(i..=i, replacement);
                changed = true;
                // Don't advance i — the replacement stmts need processing.
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn refine_stmt(stmt: &mut HirStmt, env: &RangeEnv) -> bool {
    match stmt {
        HirStmt::Switch { expr, cases, default } => {
            let mut changed = false;

            // Recurse into case bodies.
            for case in cases.iter_mut() {
                if refine_stmts(&mut case.body, env) {
                    changed = true;
                }
            }
            if refine_stmts(default, env) {
                changed = true;
            }

            // Attempt to narrow the switch to known live cases.
            // If expr's range is [lo, hi) we can prune cases outside that range.
            let range = eval_expr(expr, env);
            if !range.is_top() && !range.is_bottom() {
                let before = cases.len();
                cases.retain(|c| {
                    c.values.iter().any(|&v| {
                        let v_u = v as u64;
                        // Check if v_u is in range [lo, hi).
                        let lo = range.lo();
                        let hi = range.hi();
                        let mask = if range.bits() >= 64 { u64::MAX } else { (1u64 << range.bits()) - 1 };
                        if lo <= hi {
                            lo <= (v_u & mask) && (v_u & mask) < hi
                        } else {
                            // Wrapped arc.
                            (v_u & mask) >= lo || (v_u & mask) < hi
                        }
                    })
                });
                if cases.len() != before {
                    changed = true;
                }
            }
            changed
        }
        HirStmt::If { then_body, else_body, cond } => {
            let mut changed = refine_stmts(then_body, env);
            if refine_stmts(else_body, env) { changed = true; }

            // Constant condition elimination.
            let range = eval_expr(cond, env);
            if let Some(v) = range.singleton_value() {
                let replacement = if v != 0 {
                    then_body.drain(..).collect::<Vec<_>>()
                } else {
                    else_body.drain(..).collect::<Vec<_>>()
                };
                *stmt = HirStmt::Block(replacement);
                return true;
            }
            changed
        }
        HirStmt::While { body, cond } => {
            let mut changed = refine_stmts(body, env);
            // If condition is provably false, remove the loop.
            let range = eval_expr(cond, env);
            if range.singleton_value() == Some(0) {
                *stmt = HirStmt::Block(vec![]);
                return true;
            }
            changed
        }
        HirStmt::DoWhile { body, cond: _ } => refine_stmts(body, env),
        HirStmt::For { init: _, body, update: _, .. } => refine_stmts(body, env),
        HirStmt::Block(stmts) => refine_stmts(stmts, env),
        _ => false,
    }
}
