//! Hoist identical **pure** statement prefixes from both arms of `if (cond)`.
//!
//! When `then_body` and `else_body` begin with the same sequence of
//! `x = <pure_expr>` assignments (same variable name and same `pure_expr_key`
//! for the RHS), those statements are moved before the `if`, preserving
//! semantics when the RHS has no side effects and does not include `Load` (loads
//! are excluded because `pure_expr_key` does not cover them — memory must not
//! be re-read across the hoisting boundary without analysis).
//!
//! This is **not** LICM (loops) and **not** local CSE (single block); it is a
//! minimal **partial redundancy elimination** for 2-way branches.
//!
//! At most [`MAX_HOIST_PREFIX`] statements are hoisted per `if` to bound work.

use super::super::analysis::expr_key::pure_expr_key;
use super::super::cleanup::expr_has_side_effects;
use super::super::*;

const MAX_HOIST_PREFIX: usize = 32;

/// Hoist common pure prefixes on `if` / `else` arms.  Returns `true` if changed.
pub(crate) fn apply_branch_prefix_hoist_pass(func: &mut HirFunction) -> bool {
    hoist_stmts(&mut func.body)
}

fn hoist_stmts(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= hoist_stmt_deep(stmt);
    }
    let mut i = 0;
    while i < stmts.len() {
        if let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &mut stmts[i]
        {
            let n = common_hoist_prefix_len(then_body.as_slice(), else_body.as_slice());
            if n > 0 {
                let lifted: Vec<HirStmt> = then_body.drain(0..n).collect();
                else_body.drain(0..n);
                for s in lifted.into_iter().rev() {
                    stmts.insert(i, s);
                }
                changed = true;
                i += n + 1;
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn hoist_stmt_deep(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            changed |= hoist_stmts(then_body);
            changed |= hoist_stmts(else_body);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= hoist_stmts(body);
        }
        HirStmt::For {
            init, body, update, ..
        } => {
            if let Some(s) = init {
                changed |= hoist_stmt_deep(s);
            }
            changed |= hoist_stmts(body);
            if let Some(s) = update {
                changed |= hoist_stmt_deep(s);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                changed |= hoist_stmts(&mut case.body);
            }
            changed |= hoist_stmts(default);
        }
        HirStmt::Block(body) => {
            changed |= hoist_stmts(body);
        }
        _ => {}
    }
    changed
}

fn common_hoist_prefix_len(then_body: &[HirStmt], else_body: &[HirStmt]) -> usize {
    let max = MAX_HOIST_PREFIX.min(then_body.len()).min(else_body.len());
    let mut n = 0;
    while n < max {
        if same_hoistable_pair(&then_body[n], &else_body[n]) {
            n += 1;
        } else {
            break;
        }
    }
    n
}

/// Both statements must be `Assign { lhs: Var(same), rhs }` with identical
/// pure expression keys and no RHS side effects (Call, Load, etc.).
fn same_hoistable_pair(a: &HirStmt, b: &HirStmt) -> bool {
    let (
        HirStmt::Assign {
            lhs: HirLValue::Var(na),
            rhs: ra,
        },
        HirStmt::Assign {
            lhs: HirLValue::Var(nb),
            rhs: rb,
        },
    ) = (a, b)
    else {
        return false;
    };
    if na != nb {
        return false;
    }
    if expr_has_side_effects(ra) || expr_has_side_effects(rb) {
        return false;
    }
    match (pure_expr_key(ra), pure_expr_key(rb)) {
        (Some(ka), Some(kb)) => ka == kb,
        _ => false,
    }
}
