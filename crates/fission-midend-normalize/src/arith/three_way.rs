use crate::prelude::*;

/// Simplifies comparisons involving 3-way comparisons.
/// Detects expressions of the form `zext(a < b) + zext(a <= b) - 1` (or similar)
/// compared to constants (-1, 0, 1) and replaces them with direct comparisons.
pub fn apply_three_way_compare_pass(func: &mut DirFunction) -> bool {
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

    // Recurse first
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. } => {
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

    // Attempt to simplify 3-way comparisons on this Binary expression
    if let DirExpr::Binary { op, lhs, rhs, ty } = expr {
        if let Some((new_op, v, w)) = try_simplify_three_way_cmp(*op, lhs, rhs) {
            *expr = DirExpr::Binary {
                op: new_op,
                lhs: Box::new(v),
                rhs: Box::new(w),
                ty: ty.clone(),
            };
            changed = true;
        }
    }

    changed
}

fn collect_add_terms(expr: &DirExpr, terms: &mut Vec<DirExpr>) {
    if let DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = expr
    {
        collect_add_terms(lhs, terms);
        collect_add_terms(rhs, terms);
    } else {
        terms.push(expr.clone());
    }
}

fn detect_three_way(expr: &DirExpr) -> Option<(DirExpr, DirExpr, DirBinaryOp)> {
    let mut terms = Vec::new();
    collect_add_terms(expr, &mut terms);
    if terms.len() != 3 {
        return None;
    }
    // Find the constant -1
    let const_idx = terms.iter().position(|t| {
        if let DirExpr::Const(val, ty) = t {
            let mask = match ty {
                NirType::Int { bits, .. } => {
                    (1u64.checked_shl(*bits).unwrap_or(0).wrapping_sub(1)) as i64
                }
                _ => -1,
            };
            *val == -1 || *val == mask
        } else {
            false
        }
    })?;
    let _const_term = terms.remove(const_idx);

    // The other two terms must be Casts of comparisons
    let mut comparisons = Vec::new();
    for term in &terms {
        if let DirExpr::Cast { expr: inner, .. } = term {
            if let DirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref() {
                comparisons.push((*op, lhs.as_ref(), rhs.as_ref()));
            }
        }
    }
    if comparisons.len() != 2 {
        return None;
    }

    let (op1, lhs1, rhs1) = comparisons[0];
    let (op2, lhs2, rhs2) = comparisons[1];

    // Check if lhs1 == lhs2 and rhs1 == rhs2
    if lhs1 != lhs2 || rhs1 != rhs2 {
        return None;
    }

    // One must be less than, the other must be less-than-or-equal
    let is_less = |op: DirBinaryOp| matches!(op, DirBinaryOp::Lt | DirBinaryOp::SLt);
    let is_lesseq = |op: DirBinaryOp| matches!(op, DirBinaryOp::Le | DirBinaryOp::SLe);

    if (is_less(op1) && is_lesseq(op2)) || (is_lesseq(op1) && is_less(op2)) {
        let less_op = if is_less(op1) { op1 } else { op2 };
        return Some((lhs1.clone(), rhs1.clone(), less_op));
    }

    None
}

enum TargetRelation {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

fn get_compare_op(less_op: DirBinaryOp, rel: TargetRelation) -> DirBinaryOp {
    match (less_op, rel) {
        (DirBinaryOp::Lt, TargetRelation::Lt) => DirBinaryOp::Lt,
        (DirBinaryOp::Lt, TargetRelation::Le) => DirBinaryOp::Le,
        (DirBinaryOp::Lt, TargetRelation::Gt) => DirBinaryOp::Gt,
        (DirBinaryOp::Lt, TargetRelation::Ge) => DirBinaryOp::Ge,
        (DirBinaryOp::Lt, TargetRelation::Eq) => DirBinaryOp::Eq,
        (DirBinaryOp::Lt, TargetRelation::Ne) => DirBinaryOp::Ne,

        (DirBinaryOp::SLt, TargetRelation::Lt) => DirBinaryOp::SLt,
        (DirBinaryOp::SLt, TargetRelation::Le) => DirBinaryOp::SLe,
        (DirBinaryOp::SLt, TargetRelation::Gt) => DirBinaryOp::SGt,
        (DirBinaryOp::SLt, TargetRelation::Ge) => DirBinaryOp::SGe,
        (DirBinaryOp::SLt, TargetRelation::Eq) => DirBinaryOp::Eq,
        (DirBinaryOp::SLt, TargetRelation::Ne) => DirBinaryOp::Ne,

        _ => less_op,
    }
}

fn try_simplify_three_way_cmp(
    op: DirBinaryOp,
    lhs: &DirExpr,
    rhs: &DirExpr,
) -> Option<(DirBinaryOp, DirExpr, DirExpr)> {
    // We expect one side to be a 3-way compare, and the other side to be a constant
    // Match (3way, constant) or (constant, 3way)
    let (three_way_expr, const_val, is_three_way_lhs) = match (lhs, rhs) {
        (t, DirExpr::Const(c, _)) => (t, *c, true),
        (DirExpr::Const(c, _), t) => (t, *c, false),
        _ => return None,
    };

    let (v, w, less_op) = detect_three_way(three_way_expr)?;

    // Simplification table based on:
    //   X is the 3-way result (-1, 0, or 1)
    //   OP is the comparison operator
    //   C is the constant
    // We return the simplified (new_op, new_lhs, new_rhs)
    // If is_three_way_lhs is true, the comparison is `X OP C`.
    // If false, the comparison is `C OP X`, which is equivalent to `X OP_swapped C`.
    let normalized_op = if is_three_way_lhs {
        op
    } else {
        match op {
            DirBinaryOp::Lt => DirBinaryOp::Gt,
            DirBinaryOp::SLt => DirBinaryOp::SGt,

            DirBinaryOp::Le => DirBinaryOp::Ge,
            DirBinaryOp::SLe => DirBinaryOp::SGe,

            DirBinaryOp::Gt => DirBinaryOp::Lt,
            DirBinaryOp::SGt => DirBinaryOp::SLt,

            DirBinaryOp::Ge => DirBinaryOp::Le,
            DirBinaryOp::SGe => DirBinaryOp::SLe,

            other => other, // Eq, Ne are symmetric
        }
    };

    // Helper to return comparison inputs in order (v, w) or (w, v)
    let make_ret = |rel: TargetRelation| -> Option<(DirBinaryOp, DirExpr, DirExpr)> {
        match rel {
            TargetRelation::Lt => Some((
                get_compare_op(less_op, TargetRelation::Lt),
                v.clone(),
                w.clone(),
            )),
            TargetRelation::Le => Some((
                get_compare_op(less_op, TargetRelation::Le),
                v.clone(),
                w.clone(),
            )),
            TargetRelation::Gt => Some((
                get_compare_op(less_op, TargetRelation::Lt),
                w.clone(),
                v.clone(),
            )),
            TargetRelation::Ge => Some((
                get_compare_op(less_op, TargetRelation::Le),
                w.clone(),
                v.clone(),
            )),
            TargetRelation::Eq => Some((
                get_compare_op(less_op, TargetRelation::Eq),
                v.clone(),
                w.clone(),
            )),
            TargetRelation::Ne => Some((
                get_compare_op(less_op, TargetRelation::Ne),
                v.clone(),
                w.clone(),
            )),
        }
    };

    match normalized_op {
        // EQUAL
        DirBinaryOp::Eq => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X == -1 => v < w
                0 => make_ret(TargetRelation::Eq),  // X == 0  => v == w
                1 => make_ret(TargetRelation::Gt),  // X == 1  => w < v (v > w)
                _ => None,
            }
        }
        // NOT EQUAL
        DirBinaryOp::Ne => {
            match const_val {
                -1 => make_ret(TargetRelation::Ge), // X != -1 => v >= w
                0 => make_ret(TargetRelation::Ne),  // X != 0  => v != w
                1 => make_ret(TargetRelation::Le),  // X != 1  => v <= w
                _ => None,
            }
        }
        // LESS THAN (signed and unsigned)
        DirBinaryOp::Lt | DirBinaryOp::SLt => {
            match const_val {
                0 => make_ret(TargetRelation::Lt), // X < 0  => v < w
                1 => make_ret(TargetRelation::Le), // X < 1  => v <= w
                -1 => None, // X < -1 => always false (not simplified here, handled by constant folding)
                _ => None,
            }
        }
        // LESS OR EQUAL
        DirBinaryOp::Le | DirBinaryOp::SLe => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X <= -1 => v < w
                0 => make_ret(TargetRelation::Le),  // X <= 0  => v <= w
                1 => None,                          // X <= 1 => always true (not simplified here)
                _ => None,
            }
        }
        // GREATER THAN
        DirBinaryOp::Gt | DirBinaryOp::SGt => {
            match const_val {
                0 => make_ret(TargetRelation::Gt),  // X > 0  => w < v (v > w)
                -1 => make_ret(TargetRelation::Ge), // X > -1 => w <= v (v >= w)
                1 => None,                          // X > 1 => always false
                _ => None,
            }
        }
        // GREATER OR EQUAL
        DirBinaryOp::Ge | DirBinaryOp::SGe => {
            match const_val {
                1 => make_ret(TargetRelation::Gt), // X >= 1  => w < v (v > w)
                0 => make_ret(TargetRelation::Ge), // X >= 0  => w <= v (v >= w)
                -1 => None,                        // X >= -1 => always true
                _ => None,
            }
        }
        _ => None,
    }
}
