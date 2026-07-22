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
use crate::prelude::*;

pub fn apply_post_assign_value_representative_pass(func: &mut DirFunction) -> bool {
    let mut reps = PureExprMap::default();
    stabilize_stmts(&mut func.body, &mut reps)
}

fn stabilize_stmts(stmts: &mut [DirStmt], reps: &mut PureExprMap) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= stabilize_stmt(stmt, reps);
    }
    changed
}

fn stabilize_stmt(stmt: &mut DirStmt, reps: &mut PureExprMap) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            if let DirLValue::Var(name) = lhs {
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
        DirStmt::If {
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
        DirStmt::While { cond, body } => {
            changed |= stabilize_expr(cond, reps);
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(body, &mut body_reps);
            reps.clear();
        }
        DirStmt::DoWhile { body, cond } => {
            let mut body_reps = reps.clone();
            changed |= stabilize_stmts(body, &mut body_reps);
            changed |= stabilize_expr(cond, &mut body_reps);
            reps.clear();
        }
        DirStmt::For {
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
        DirStmt::Switch {
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
        DirStmt::Block(body) => {
            changed |= stabilize_stmts(body, reps);
        }
        DirStmt::Expr(expr)
        | DirStmt::Return(Some(expr))
        | DirStmt::VaStart { va_list: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
        }
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => {
            reps.clear();
        }
    }
    changed
}

fn stabilize_lvalue(lhs: &mut DirLValue, reps: &PureExprMap) -> bool {
    match lhs {
        DirLValue::Var(_) => false,
        DirLValue::Deref { ptr, .. } => stabilize_expr(ptr, reps),
        DirLValue::Index { base, index, .. } => {
            let base_changed = stabilize_expr(base, reps);
            let index_changed = stabilize_expr(index, reps);
            base_changed || index_changed
        }
        DirLValue::FieldAccess { base, .. } => stabilize_expr(base, reps),
    }
}

fn stabilize_expr(expr: &mut DirExpr, reps: &PureExprMap) -> bool {
    if let Some(key) = pure_expr_key(expr) {
        if let Some(name) = reps.get(&key) {
            *expr = DirExpr::Var(name.clone());
            return true;
        }
    }

    let mut changed = false;
    match expr {
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => {
            changed |= stabilize_expr(expr, reps);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= stabilize_expr(lhs, reps);
            changed |= stabilize_expr(rhs, reps);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= stabilize_expr(arg, reps);
            }
        }
        DirExpr::Index { base, index, .. } => {
            changed |= stabilize_expr(base, reps);
            changed |= stabilize_expr(index, reps);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= stabilize_expr(cond, reps);
            changed |= stabilize_expr(then_expr, reps);
            changed |= stabilize_expr(else_expr, reps);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }
    changed
}

fn is_representable_expr(expr: &DirExpr) -> bool {
    !matches!(
        expr,
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _)
    ) && pure_expr_key(expr).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_do_while_condition() {
        let mut func = DirFunction {
            name: "test_post_assign_do_while".to_string(),
            int_param_offsets: Vec::new(),
            return_type: NirType::Unknown,
            body: vec![DirStmt::DoWhile {
                body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("p".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(DirExpr::Const(2, int(64))),
                        ty: int(64),
                    },
                }],
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(var("p")),
                        rhs: Box::new(DirExpr::Const(2, int(64))),
                        ty: int(64),
                    }),
                    rhs: Box::new(var("end")),
                    ty: int(64),
                },
            }],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let DirStmt::DoWhile { cond, .. } = &func.body[0] else {
            panic!("expected do-while");
        };
        let DirExpr::Binary { lhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(lhs.as_ref(), &var("p"));
    }

    #[test]
    fn stabilizes_self_referential_assignment_before_if_condition() {
        let mut func = DirFunction {
            name: "test_post_assign_if".to_string(),
            int_param_offsets: Vec::new(),
            return_type: NirType::Unknown,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(DirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Add,
                            lhs: Box::new(var("i")),
                            rhs: Box::new(DirExpr::Const(1, int(32))),
                            ty: int(32),
                        }),
                        ty: int(32),
                    },
                    then_body: vec![DirStmt::Goto("loop".to_string())],
                    else_body: vec![],
                },
            ],
            ..Default::default()
        };

        assert!(apply_post_assign_value_representative_pass(&mut func));
        let DirStmt::If { cond, .. } = &func.body[1] else {
            panic!("expected if");
        };
        let DirExpr::Binary { rhs, .. } = cond else {
            panic!("expected binary condition");
        };
        assert_eq!(rhs.as_ref(), &var("i"));
    }
}
