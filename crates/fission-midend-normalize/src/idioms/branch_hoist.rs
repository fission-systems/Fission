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
use crate::prelude::*;

const MAX_HOIST_PREFIX: usize = 32;

/// Hoist common pure prefixes on `if` / `else` arms.  Returns `true` if changed.
pub fn apply_branch_prefix_hoist_pass(func: &mut PreHirFunction) -> bool {
    hoist_stmts(&mut func.body)
}

fn hoist_stmts(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= hoist_stmt_deep(stmt);
    }
    let mut i = 0;
    while i < stmts.len() {
        if let PreHirStmt::If {
            cond,
            then_body,
            else_body,
            ..
        } = &mut stmts[i]
        {
            let n = common_hoist_prefix_len(cond, then_body.as_slice(), else_body.as_slice());
            if n > 0 {
                let lifted: Vec<PreHirStmt> = std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body)
                    .drain(0..n)
                    .collect();
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body).drain(0..n);
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

fn hoist_stmt_deep(stmt: &mut PreHirStmt) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::For {
            init, body, update, ..
        } => {
            if let Some(s) = init {
                changed |= hoist_stmt_deep(s);
            }
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            if let Some(s) = update {
                changed |= hoist_stmt_deep(s);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
        }
        PreHirStmt::Block(body) => {
            changed |= hoist_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        _ => {}
    }
    changed
}

fn common_hoist_prefix_len(
    cond: &PreHirExpr,
    then_body: &[PreHirStmt],
    else_body: &[PreHirStmt],
) -> usize {
    if expr_has_side_effects(cond) {
        return 0;
    }
    let max = MAX_HOIST_PREFIX.min(then_body.len()).min(else_body.len());
    let mut n = 0;
    while n < max {
        if same_hoistable_pair(&then_body[n], &else_body[n]) {
            if let PreHirStmt::Assign {
                lhs: PreHirLValue::Var(var_name),
                ..
            } = &then_body[n]
            {
                if expr_uses_var(cond, var_name) {
                    break;
                }
            }
            n += 1;
        } else {
            break;
        }
    }
    n
}

fn expr_uses_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(v) => v == name,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_uses_var(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_uses_var(lhs, name) || expr_uses_var(rhs, name),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var(cond, name)
                || expr_uses_var(then_expr, name)
                || expr_uses_var(else_expr, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_uses_var(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_uses_var(base, name) || expr_uses_var(index, name)
        }
        PreHirExpr::Const(..)
        | PreHirExpr::AddressOfGlobal(..)
        | PreHirExpr::AddressOfLocal(..) => false,
    }
}

/// Both statements must be `Assign { lhs: Var(same), rhs }` with identical
/// pure expression keys and no RHS side effects (Call, Load, etc.).
fn same_hoistable_pair(a: &PreHirStmt, b: &PreHirStmt) -> bool {
    let (
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(na),
            rhs: ra,
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(nb),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    #[test]
    fn does_not_hoist_assignment_to_variable_used_in_condition() {
        let mut func = PreHirFunction {
            name: "hoist_clobber_test".into(),
            body: vec![PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(PreHirExpr::Var("x".into())),
                    rhs: Box::new(PreHirExpr::Const(5, u32_ty())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("x".into()),
                        rhs: PreHirExpr::Const(0, u32_ty()),
                    },
                    PreHirStmt::Return(None),
                ]
                .into(),
                else_body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("x".into()),
                        rhs: PreHirExpr::Const(0, u32_ty()),
                    },
                    PreHirStmt::Return(None),
                ]
                .into(),
            }],
            ..Default::default()
        };

        assert!(
            !apply_branch_prefix_hoist_pass(&mut func),
            "x = 0 must NOT be hoisted before if (x == 5)"
        );
        assert_eq!(func.body.len(), 1);
    }

    #[test]
    fn hoists_unrelated_common_assignment() {
        let mut func = PreHirFunction {
            name: "hoist_unrelated_test".into(),
            body: vec![PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    lhs: Box::new(PreHirExpr::Var("x".into())),
                    rhs: Box::new(PreHirExpr::Const(5, u32_ty())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("y".into()),
                        rhs: PreHirExpr::Const(42, u32_ty()),
                    },
                    PreHirStmt::Return(None),
                ]
                .into(),
                else_body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("y".into()),
                        rhs: PreHirExpr::Const(42, u32_ty()),
                    },
                    PreHirStmt::Return(None),
                ]
                .into(),
            }],
            ..Default::default()
        };

        assert!(
            apply_branch_prefix_hoist_pass(&mut func),
            "y = 42 should be hoisted"
        );
        assert_eq!(func.body.len(), 2);
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } if name == "y"
        ));
    }
}
