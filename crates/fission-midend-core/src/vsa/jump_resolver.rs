/// Jump table resolution using VSA range information.
///
/// After solving ranges for all variables in a function, this module
/// attempts to refine `DirStmt::Switch` expressions by:
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
use super::transfer::{RangeEnv, eval_expr};
use crate::wave_stats::{
    add_dispatcher_shape_recoveries, add_indirect_target_set_refinements,
};
use crate::ir::{DirFunction, DirStmt};

/// Apply VSA-based switch refinement to a function's body.
///
/// Returns `true` if any changes were made.
pub fn apply_jump_resolver_pass(func: &mut DirFunction) -> bool {
    if jump_resolver_candidate_count(&func.body) == 0 {
        return false;
    }
    let env = solve(func);
    refine_stmts(&mut func.body, &env)
}

pub fn jump_resolver_candidate_count(stmts: &[DirStmt]) -> usize {
    fn count_opt_stmt(stmt: &Option<Box<DirStmt>>) -> usize {
        stmt.as_deref().map_or(0, count_stmt)
    }

    fn count_stmt(stmt: &DirStmt) -> usize {
        match stmt {
            DirStmt::Switch { cases, default, .. } => {
                let local_cost = 1 + cases.len().min(8) + usize::from(!default.is_empty());
                local_cost
                    + cases
                        .iter()
                        .map(|case| jump_resolver_candidate_count(&case.body))
                        .sum::<usize>()
                    + jump_resolver_candidate_count(default)
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                jump_resolver_candidate_count(then_body) + jump_resolver_candidate_count(else_body)
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } | DirStmt::Block(body) => {
                jump_resolver_candidate_count(body)
            }
            DirStmt::For {
                init, body, update, ..
            } => {
                count_opt_stmt(init) + jump_resolver_candidate_count(body) + count_opt_stmt(update)
            }
            _ => 0,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

fn refine_stmts(stmts: &mut Vec<DirStmt>, env: &RangeEnv) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if refine_stmt(&mut stmts[i], env) {
            changed = true;
        }
        // If the stmt was reduced to a singleton-constant switch,
        // inline the matching case in place.
        if let DirStmt::Switch {
            expr,
            cases,
            default,
        } = &stmts[i]
        {
            let range = eval_expr(expr, env);
            if let Some(v) = range.singleton_value() {
                let v_signed = v as i64;
                // Find the matching case.
                let replacement = cases
                    .iter()
                    .find(|c| c.values.contains(&v_signed))
                    .map(|c| c.body.clone())
                    .unwrap_or_else(|| default.clone());
                stmts.splice(i..=i, replacement);
                add_dispatcher_shape_recoveries(1);
                changed = true;
                // Don't advance i — the replacement stmts need processing.
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn refine_stmt(stmt: &mut DirStmt, env: &RangeEnv) -> bool {
    match stmt {
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
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
                        let mask = if range.bits() >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << range.bits()) - 1
                        };
                        if lo <= hi {
                            lo <= (v_u & mask) && (v_u & mask) < hi
                        } else {
                            // Wrapped arc.
                            (v_u & mask) >= lo || (v_u & mask) < hi
                        }
                    })
                });
                if cases.len() != before {
                    add_indirect_target_set_refinements(1);
                    add_dispatcher_shape_recoveries(1);
                    changed = true;
                }
            }
            changed
        }
        DirStmt::If {
            then_body,
            else_body,
            cond,
        } => {
            let mut changed = refine_stmts(then_body, env);
            if refine_stmts(else_body, env) {
                changed = true;
            }

            // Constant condition elimination.
            let range = eval_expr(cond, env);
            if let Some(v) = range.singleton_value() {
                let replacement = if v != 0 {
                    then_body.drain(..).collect::<Vec<_>>()
                } else {
                    else_body.drain(..).collect::<Vec<_>>()
                };
                *stmt = DirStmt::Block(replacement);
                return true;
            }
            changed
        }
        DirStmt::While { body, cond } => {
            let changed = refine_stmts(body, env);
            // If condition is provably false, remove the loop.
            let range = eval_expr(cond, env);
            if range.singleton_value() == Some(0) {
                *stmt = DirStmt::Block(vec![]);
                return true;
            }
            changed
        }
        DirStmt::DoWhile { body, cond: _ } => refine_stmts(body, env),
        DirStmt::For {
            init: _,
            body,
            update: _,
            ..
        } => refine_stmts(body, env),
        DirStmt::Block(stmts) => refine_stmts(stmts, env),
        _ => false,
    }
}
