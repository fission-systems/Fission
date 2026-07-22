use crate::prelude::*;
use super::util::is_zero_const;

/// Simplifies bitwise-OR comparisons with zero:
/// - `(V | W) == 0` => `(V == 0) && (W == 0)`
/// - `(V | W) != 0` => `(V != 0) || (W != 0)`
///
/// Since this pass operates bottom-up, nested bitwise ORs (e.g. `(A | B | C) == 0`)
/// will automatically unfold to logical chains like `A == 0 && B == 0 && C == 0`.
pub fn apply_or_compare_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;
    changed |= simplify_stmts(&mut func.body);
    changed
}

fn simplify_stmts(stmts: &mut [DirStmt]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt);
    }
    changed
}

fn simplify_stmt(stmt: &mut DirStmt) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            changed |= simplify_expr(rhs);
            changed |= simplify_lvalue(lhs);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= simplify_stmt(i.as_mut());
            }
            if let Some(c) = cond {
                changed |= simplify_expr(c);
            }
            if let Some(u) = update {
                changed |= simplify_stmt(u.as_mut());
            }
            changed |= simplify_stmts(body);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond);
            changed |= simplify_stmts(then_body);
            changed |= simplify_stmts(else_body);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= simplify_expr(expr);
            for case in cases {
                changed |= simplify_stmts(&mut case.body);
            }
            changed |= simplify_stmts(default);
        }
        DirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list);
        }
        _ => {}
    }
    changed
}

fn simplify_lvalue(lval: &mut DirLValue) -> bool {
    let mut changed = false;
    match lval {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => {
            changed |= simplify_expr(ptr);
        }
        DirLValue::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
        DirLValue::FieldAccess { base, .. } => {
            changed |= simplify_expr(base);
        }
    }
    changed
}

fn simplify_expr(expr: &mut DirExpr) -> bool {
    let mut changed = false;

    // Recurse first bottom-up
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            changed |= simplify_expr(inner);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs);
            changed |= simplify_expr(rhs);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= simplify_expr(cond);
            changed |= simplify_expr(then_expr);
            changed |= simplify_expr(else_expr);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
        _ => {}
    }

    // Match OR-of-zero (RuleOrPredicate):
    // (cond ? val : 0) | other  =>  cond ? (val | other) : other
    // (cond ? 0 : val) | other  =>  cond ? other : (val | other)
    if let DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    {
        let mut target = None;
        if let DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } = lhs.as_ref()
        {
            if is_zero_const(then_expr) {
                target = Some((
                    true,
                    cond.clone(),
                    then_expr.clone(),
                    else_expr.clone(),
                    rhs.clone(),
                ));
            } else if is_zero_const(else_expr) {
                target = Some((
                    false,
                    cond.clone(),
                    then_expr.clone(),
                    else_expr.clone(),
                    rhs.clone(),
                ));
            }
        }
        if target.is_none() {
            if let DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } = rhs.as_ref()
            {
                if is_zero_const(then_expr) {
                    target = Some((
                        true,
                        cond.clone(),
                        then_expr.clone(),
                        else_expr.clone(),
                        lhs.clone(),
                    ));
                } else if is_zero_const(else_expr) {
                    target = Some((
                        false,
                        cond.clone(),
                        then_expr.clone(),
                        else_expr.clone(),
                        lhs.clone(),
                    ));
                }
            }
        }
        if let Some((then_is_zero, cond, then_expr, else_expr, other)) = target {
            let new_then = if then_is_zero {
                other.clone()
            } else {
                Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Or,
                    lhs: then_expr,
                    rhs: other.clone(),
                    ty: ty.clone(),
                })
            };
            let new_else = if then_is_zero {
                Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Or,
                    lhs: else_expr,
                    rhs: other,
                    ty: ty.clone(),
                })
            } else {
                other
            };
            *expr = DirExpr::Select {
                cond,
                then_expr: new_then,
                else_expr: new_else,
                ty: ty.clone(),
            };
            changed = true;
            simplify_expr(expr);
        }
    }

    // Match comparison with zero
    if let DirExpr::Binary {
        op: cmp_op @ (DirBinaryOp::Eq | DirBinaryOp::Ne),
        lhs,
        rhs,
        ..
    } = expr
    {
        let (or_expr, is_lhs) = if is_zero_const(rhs) {
            if let DirExpr::Binary {
                op: DirBinaryOp::Or,
                ..
            } = lhs.as_ref()
            {
                (lhs.as_ref(), true)
            } else {
                return changed;
            }
        } else if is_zero_const(lhs) {
            if let DirExpr::Binary {
                op: DirBinaryOp::Or,
                ..
            } = rhs.as_ref()
            {
                (rhs.as_ref(), false)
            } else {
                return changed;
            }
        } else {
            return changed;
        };

        if let DirExpr::Binary {
            lhs: or_lhs,
            rhs: or_rhs,
            ty: or_ty,
            ..
        } = or_expr
        {
            let mut left_cmp = DirExpr::Binary {
                op: *cmp_op,
                lhs: or_lhs.clone(),
                rhs: Box::new(DirExpr::Const(0, or_ty.clone())),
                ty: NirType::Bool,
            };
            let mut right_cmp = DirExpr::Binary {
                op: *cmp_op,
                lhs: or_rhs.clone(),
                rhs: Box::new(DirExpr::Const(0, or_ty.clone())),
                ty: NirType::Bool,
            };

            // Recursively simplify the newly created comparisons to handle nested ORs
            simplify_expr(&mut left_cmp);
            simplify_expr(&mut right_cmp);

            let logical_op = match cmp_op {
                DirBinaryOp::Eq => DirBinaryOp::LogicalAnd,
                DirBinaryOp::Ne => DirBinaryOp::LogicalOr,
                _ => unreachable!(),
            };
            *expr = DirExpr::Binary {
                op: logical_op,
                lhs: Box::new(left_cmp),
                rhs: Box::new(right_cmp),
                ty: NirType::Bool,
            };
            changed = true;
        }
    }

    changed
}
