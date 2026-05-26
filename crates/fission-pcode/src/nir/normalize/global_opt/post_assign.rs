//! Post-assignment value representative recovery.
//!
//! The HIR builder lowers p-code SSA values into mutable C-like variables.  In
//! straight-line regions, a branch condition can still contain the exact pure
//! expression that was just assigned to a representative variable:
//!
//! ```text
//! x = x + 2;
//! if (x + 2 - end) goto loop;
//! ```
//!
//! In the original SSA value graph the condition consumes the assigned value,
//! not the post-assignment C expression `x + 2`.  This pass tracks pure
//! expression representatives inside a single straight-line statement list and
//! rewrites control expressions to use the latest representative.

use super::super::analysis::expr_key::{PureExprMap, invalidate_pure_map, pure_expr_key};
use super::super::*;

pub(crate) fn apply_post_assign_value_representative_pass(func: &mut HirFunction) -> bool {
    let mut reps = PureExprMap::new();
    stabilize_stmts(&mut func.body, &mut reps)
}

fn stabilize_stmts(stmts: &mut [HirStmt], reps: &mut PureExprMap) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= stabilize_stmt(stmt, reps);
    }
    changed
}

fn stabilize_stmt(stmt: &mut HirStmt, reps: &mut PureExprMap) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            if let HirLValue::Var(name) = lhs {
                invalidate_pure_map(reps, name);
                if is_representable_expr(rhs) {
                    if let Some(key) = pure_expr_key(rhs) {
                        reps.insert(key, name.clone());
                    }
                }
            } else {
                changed |= stabilize_lvalue(lhs, reps);
                changed |= stabilize_expr(rhs, reps);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= stabilize_expr(cond, reps);
            let mut then_reps = reps.clone();
            let mut else_reps = reps.clone();
            changed |= stabilize_stmts(then_body, &mut then_reps);
            changed |= stabilize_stmts(else_body, &mut else_reps);
            reps.clear();
        }
        HirStmt::While { cond, body } => {
            changed |= stabilize_expr(cond, reps);
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(body, &mut body_reps);
            reps.clear();
        }
        HirStmt::DoWhile { body, cond } => {
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(body, &mut body_reps);
            changed |= stabilize_expr(cond, &mut body_reps);
            reps.clear();
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= stabilize_stmt(init, reps);
            }
            if let Some(cond) = cond {
                changed |= stabilize_expr(cond, reps);
            }
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(body, &mut body_reps);
            if let Some(update) = update {
                changed |= stabilize_stmt(update, &mut body_reps);
            }
            reps.clear();
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= stabilize_expr(expr, reps);
            for case in cases {
                let mut case_reps = reps.clone();
                changed |= stabilize_stmts(&mut case.body, &mut case_reps);
            }
            let mut default_reps = reps.clone();
            changed |= stabilize_stmts(default, &mut default_reps);
            reps.clear();
        }
        HirStmt::Block(body) => {
            changed |= stabilize_stmts(body, reps);
        }
        HirStmt::Expr(expr)
        | HirStmt::Return(Some(expr))
        | HirStmt::VaStart { va_list: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {
            reps.clear();
        }
    }
    changed
}

fn stabilize_lvalue(lhs: &mut HirLValue, reps: &PureExprMap) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => stabilize_expr(ptr, reps),
        HirLValue::Index { base, index, .. } => {
            let base_changed = stabilize_expr(base, reps);
            let index_changed = stabilize_expr(index, reps);
            base_changed || index_changed
        }
        HirLValue::FieldAccess { base, .. } => stabilize_expr(base, reps),
    }
}

fn stabilize_expr(expr: &mut HirExpr, reps: &PureExprMap) -> bool {
    if let Some(key) = pure_expr_key(expr) {
        if let Some(name) = reps.get(&key) {
            *expr = HirExpr::Var(name.clone());
            return true;
        }
    }

    let mut changed = false;
    match expr {
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= stabilize_expr(lhs, reps);
            changed |= stabilize_expr(rhs, reps);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= stabilize_expr(arg, reps);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= stabilize_expr(base, reps);
            changed |= stabilize_expr(index, reps);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= stabilize_expr(cond, reps);
            changed |= stabilize_expr(then_expr, reps);
            changed |= stabilize_expr(else_expr, reps);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn is_representable_expr(expr: &HirExpr) -> bool {
    !matches!(
        expr,
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _)
    ) && pure_expr_key(expr).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_do_while_condition() {
        let mut func = HirFunction {
            name: "test_post_assign_do_while".to_string(),
            return_type: NirType::Unknown,
            body: vec![HirStmt::DoWhile {
                body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("p".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(HirExpr::Const(2, int(64))),
                        ty: int(64),
                    },
                }],
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(HirExpr::Const(2, int(64))),
                        ty: int(64),
                    }),
                    rhs: Box::new(var("end")),
                    ty: int(64),
                },
            }],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let HirStmt::DoWhile { cond, .. } = &func.body[0] else {
            panic!("expected do-while");
        };
        let HirExpr::Binary { lhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(lhs.as_ref(), &var("p"));
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_if_condition() {
        let mut func = HirFunction {
            name: "test_post_assign_if".to_string(),
            return_type: NirType::Unknown,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(HirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(var("i")),
                            rhs: Box::new(HirExpr::Const(1, int(32))),
                            ty: int(32),
                        }),
                        ty: int(32),
                    },
                    then_body: vec![HirStmt::Goto("loop".to_string())],
                    else_body: vec![],
                },
            ],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let HirStmt::If { cond, .. } = &func.body[1] else {
            panic!("expected if");
        };
        let HirExpr::Binary { rhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(rhs.as_ref(), &var("i"));
    }
}
