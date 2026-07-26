use crate::prelude::*;

/// Simplifies comparisons involving 3-way comparisons.
/// Detects expressions of the form `zext(a < b) + zext(a <= b) - 1` (or similar)
/// compared to constants (-1, 0, 1) and replaces them with direct comparisons.
pub fn apply_three_way_compare_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    changed |= simplify_stmts(&mut func.body);
    changed
}

fn simplify_stmts(stmts: &mut [PreHirStmt]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= simplify_stmt(stmt);
    }
    changed
}

fn simplify_stmt(stmt: &mut PreHirStmt) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= simplify_expr(rhs);
            changed |= simplify_lvalue(lhs);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            changed |= simplify_expr(expr);
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            changed |= simplify_stmts(body);
        }
        PreHirStmt::For {
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
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond);
            changed |= simplify_stmts(then_body);
            changed |= simplify_stmts(else_body);
        }
        PreHirStmt::Switch {
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
        PreHirStmt::VaStart { va_list, .. } => {
            changed |= simplify_expr(va_list);
        }
        _ => {}
    }
    changed
}

fn simplify_lvalue(lval: &mut PreHirLValue) -> bool {
    let mut changed = false;
    match lval {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => {
            changed |= simplify_expr(ptr);
        }
        PreHirLValue::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            changed |= simplify_expr(base);
        }
    }
    changed
}

fn simplify_expr(expr: &mut PreHirExpr) -> bool {
    let mut changed = false;

    // Recurse first
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            changed |= simplify_expr(inner);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= simplify_expr(lhs);
            changed |= simplify_expr(rhs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= simplify_expr(arg);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= simplify_expr(cond);
            changed |= simplify_expr(then_expr);
            changed |= simplify_expr(else_expr);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= simplify_expr(base);
            changed |= simplify_expr(index);
        }
        _ => {}
    }

    // Attempt to simplify 3-way comparisons on this Binary expression
    if let PreHirExpr::Binary { op, lhs, rhs, ty } = expr {
        if let Some((new_op, v, w)) = try_simplify_three_way_cmp(*op, lhs, rhs) {
            *expr = PreHirExpr::Binary {
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

fn collect_add_terms(expr: &PreHirExpr, terms: &mut Vec<PreHirExpr>) {
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
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

fn detect_three_way(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr, PreHirBinaryOp)> {
    let mut terms = Vec::new();
    collect_add_terms(expr, &mut terms);
    if terms.len() != 3 {
        return None;
    }
    // Find the constant -1
    let const_idx = terms.iter().position(|t| {
        if let PreHirExpr::Const(val, ty) = t {
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
        if let PreHirExpr::Cast { expr: inner, .. } = term {
            if let PreHirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref() {
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
    let is_less = |op: PreHirBinaryOp| matches!(op, PreHirBinaryOp::Lt | PreHirBinaryOp::SLt);
    let is_lesseq = |op: PreHirBinaryOp| matches!(op, PreHirBinaryOp::Le | PreHirBinaryOp::SLe);

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

fn get_compare_op(less_op: PreHirBinaryOp, rel: TargetRelation) -> PreHirBinaryOp {
    match (less_op, rel) {
        (PreHirBinaryOp::Lt, TargetRelation::Lt) => PreHirBinaryOp::Lt,
        (PreHirBinaryOp::Lt, TargetRelation::Le) => PreHirBinaryOp::Le,
        (PreHirBinaryOp::Lt, TargetRelation::Gt) => PreHirBinaryOp::Gt,
        (PreHirBinaryOp::Lt, TargetRelation::Ge) => PreHirBinaryOp::Ge,
        (PreHirBinaryOp::Lt, TargetRelation::Eq) => PreHirBinaryOp::Eq,
        (PreHirBinaryOp::Lt, TargetRelation::Ne) => PreHirBinaryOp::Ne,

        (PreHirBinaryOp::SLt, TargetRelation::Lt) => PreHirBinaryOp::SLt,
        (PreHirBinaryOp::SLt, TargetRelation::Le) => PreHirBinaryOp::SLe,
        (PreHirBinaryOp::SLt, TargetRelation::Gt) => PreHirBinaryOp::SGt,
        (PreHirBinaryOp::SLt, TargetRelation::Ge) => PreHirBinaryOp::SGe,
        (PreHirBinaryOp::SLt, TargetRelation::Eq) => PreHirBinaryOp::Eq,
        (PreHirBinaryOp::SLt, TargetRelation::Ne) => PreHirBinaryOp::Ne,

        _ => less_op,
    }
}

fn try_simplify_three_way_cmp(
    op: PreHirBinaryOp,
    lhs: &PreHirExpr,
    rhs: &PreHirExpr,
) -> Option<(PreHirBinaryOp, PreHirExpr, PreHirExpr)> {
    // We expect one side to be a 3-way compare, and the other side to be a constant
    // Match (3way, constant) or (constant, 3way)
    let (three_way_expr, const_val, is_three_way_lhs) = match (lhs, rhs) {
        (t, PreHirExpr::Const(c, _)) => (t, *c, true),
        (PreHirExpr::Const(c, _), t) => (t, *c, false),
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
            PreHirBinaryOp::Lt => PreHirBinaryOp::Gt,
            PreHirBinaryOp::SLt => PreHirBinaryOp::SGt,

            PreHirBinaryOp::Le => PreHirBinaryOp::Ge,
            PreHirBinaryOp::SLe => PreHirBinaryOp::SGe,

            PreHirBinaryOp::Gt => PreHirBinaryOp::Lt,
            PreHirBinaryOp::SGt => PreHirBinaryOp::SLt,

            PreHirBinaryOp::Ge => PreHirBinaryOp::Le,
            PreHirBinaryOp::SGe => PreHirBinaryOp::SLe,

            other => other, // Eq, Ne are symmetric
        }
    };

    // Helper to return comparison inputs in order (v, w) or (w, v)
    let make_ret = |rel: TargetRelation| -> Option<(PreHirBinaryOp, PreHirExpr, PreHirExpr)> {
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
        PreHirBinaryOp::Eq => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X == -1 => v < w
                0 => make_ret(TargetRelation::Eq),  // X == 0  => v == w
                1 => make_ret(TargetRelation::Gt),  // X == 1  => w < v (v > w)
                _ => None,
            }
        }
        // NOT EQUAL
        PreHirBinaryOp::Ne => {
            match const_val {
                -1 => make_ret(TargetRelation::Ge), // X != -1 => v >= w
                0 => make_ret(TargetRelation::Ne),  // X != 0  => v != w
                1 => make_ret(TargetRelation::Le),  // X != 1  => v <= w
                _ => None,
            }
        }
        // LESS THAN (signed and unsigned)
        PreHirBinaryOp::Lt | PreHirBinaryOp::SLt => {
            match const_val {
                0 => make_ret(TargetRelation::Lt), // X < 0  => v < w
                1 => make_ret(TargetRelation::Le), // X < 1  => v <= w
                -1 => None, // X < -1 => always false (not simplified here, handled by constant folding)
                _ => None,
            }
        }
        // LESS OR EQUAL
        PreHirBinaryOp::Le | PreHirBinaryOp::SLe => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X <= -1 => v < w
                0 => make_ret(TargetRelation::Le),  // X <= 0  => v <= w
                1 => None,                          // X <= 1 => always true (not simplified here)
                _ => None,
            }
        }
        // GREATER THAN
        PreHirBinaryOp::Gt | PreHirBinaryOp::SGt => {
            match const_val {
                0 => make_ret(TargetRelation::Gt),  // X > 0  => w < v (v > w)
                -1 => make_ret(TargetRelation::Ge), // X > -1 => w <= v (v >= w)
                1 => None,                          // X > 1 => always false
                _ => None,
            }
        }
        // GREATER OR EQUAL
        PreHirBinaryOp::Ge | PreHirBinaryOp::SGe => {
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
