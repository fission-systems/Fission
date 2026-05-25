use super::super::*;
use super::util::is_zero_const;

/// Simplifies bitwise-OR comparisons with zero:
/// - `(V | W) == 0` => `(V == 0) && (W == 0)`
/// - `(V | W) != 0` => `(V != 0) || (W != 0)`
///
/// Since this pass operates bottom-up, nested bitwise ORs (e.g. `(A | B | C) == 0`)
/// will automatically unfold to logical chains like `A == 0 && B == 0 && C == 0`.
pub(crate) fn apply_or_compare_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    changed |= simplify_stmts(&mut func.body);
    changed
}

fn simplify_stmts(stmts: &mut [HirStmt]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt);
    }
    changed
}

fn simplify_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= simplify_expr(rhs);
            changed |= simplify_lvalue(lhs);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body);
        }
        HirStmt::For { init, cond, update, body } => {
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
        HirStmt::If { cond, then_body, else_body } => {
            changed |= simplify_expr(cond);
            changed |= simplify_stmts(then_body);
            changed |= simplify_stmts(else_body);
        }
        HirStmt::Switch { expr, cases, default } => {
            changed |= simplify_expr(expr);
            for case in cases {
                changed |= simplify_stmts(&mut case.body);
            }
            changed |= simplify_stmts(default);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list);
        }
        _ => {}
    }
    changed
}

fn simplify_lvalue(lval: &mut HirLValue) -> bool {
    let mut changed = false;
    match lval {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            changed |= simplify_expr(ptr);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
    }
    changed
}

fn simplify_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;

    // Recurse first bottom-up
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            changed |= simplify_expr(inner);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs);
            changed |= simplify_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg);
            }
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            changed |= simplify_expr(cond);
            changed |= simplify_expr(then_expr);
            changed |= simplify_expr(else_expr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
        _ => {}
    }

    // Match comparison with zero
    if let HirExpr::Binary { op: cmp_op @ (HirBinaryOp::Eq | HirBinaryOp::Ne), lhs, rhs, .. } = expr {
        let (or_expr, is_lhs) = if is_zero_const(rhs) {
            if let HirExpr::Binary { op: HirBinaryOp::Or, .. } = lhs.as_ref() {
                (lhs.as_ref(), true)
            } else {
                return changed;
            }
        } else if is_zero_const(lhs) {
            if let HirExpr::Binary { op: HirBinaryOp::Or, .. } = rhs.as_ref() {
                (rhs.as_ref(), false)
            } else {
                return changed;
            }
        } else {
            return changed;
        };

        if let HirExpr::Binary { lhs: or_lhs, rhs: or_rhs, ty: or_ty, .. } = or_expr {
            let mut left_cmp = HirExpr::Binary {
                op: *cmp_op,
                lhs: or_lhs.clone(),
                rhs: Box::new(HirExpr::Const(0, or_ty.clone())),
                ty: NirType::Bool,
            };
            let mut right_cmp = HirExpr::Binary {
                op: *cmp_op,
                lhs: or_rhs.clone(),
                rhs: Box::new(HirExpr::Const(0, or_ty.clone())),
                ty: NirType::Bool,
            };
            
            // Recursively simplify the newly created comparisons to handle nested ORs
            simplify_expr(&mut left_cmp);
            simplify_expr(&mut right_cmp);

            let logical_op = match cmp_op {
                HirBinaryOp::Eq => HirBinaryOp::LogicalAnd,
                HirBinaryOp::Ne => HirBinaryOp::LogicalOr,
                _ => unreachable!(),
            };
            *expr = HirExpr::Binary {
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
