use super::super::*;

/// Simplifies comparisons involving 3-way comparisons.
/// Detects expressions of the form `zext(a < b) + zext(a <= b) - 1` (or similar)
/// compared to constants (-1, 0, 1) and replaces them with direct comparisons.
pub(crate) fn apply_three_way_compare_pass(func: &mut HirFunction) -> bool {
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
        HirLValue::FieldAccess { base, .. } => {
            changed |= simplify_expr(base);
        }
    }
    changed
}

fn simplify_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;

    // Recurse first
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

    // Attempt to simplify 3-way comparisons on this Binary expression
    if let HirExpr::Binary { op, lhs, rhs, ty } = expr {
        if let Some((new_op, v, w)) = try_simplify_three_way_cmp(*op, lhs, rhs) {
            *expr = HirExpr::Binary {
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

fn collect_add_terms(expr: &HirExpr, terms: &mut Vec<HirExpr>) {
    if let HirExpr::Binary { op: HirBinaryOp::Add, lhs, rhs, .. } = expr {
        collect_add_terms(lhs, terms);
        collect_add_terms(rhs, terms);
    } else {
        terms.push(expr.clone());
    }
}

fn detect_three_way(expr: &HirExpr) -> Option<(HirExpr, HirExpr, HirBinaryOp)> {
    let mut terms = Vec::new();
    collect_add_terms(expr, &mut terms);
    if terms.len() != 3 {
        return None;
    }
    // Find the constant -1
    let const_idx = terms.iter().position(|t| {
        if let HirExpr::Const(val, ty) = t {
            let mask = match ty {
                NirType::Int { bits, .. } => (1u64.checked_shl(*bits).unwrap_or(0).wrapping_sub(1)) as i64,
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
        if let HirExpr::Cast { expr: inner, .. } = term {
            if let HirExpr::Binary { op, lhs, rhs, .. } = inner.as_ref() {
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
    let is_less = |op: HirBinaryOp| matches!(op, HirBinaryOp::Lt | HirBinaryOp::SLt);
    let is_lesseq = |op: HirBinaryOp| matches!(op, HirBinaryOp::Le | HirBinaryOp::SLe);

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

fn get_compare_op(less_op: HirBinaryOp, rel: TargetRelation) -> HirBinaryOp {
    match (less_op, rel) {
        (HirBinaryOp::Lt, TargetRelation::Lt) => HirBinaryOp::Lt,
        (HirBinaryOp::Lt, TargetRelation::Le) => HirBinaryOp::Le,
        (HirBinaryOp::Lt, TargetRelation::Gt) => HirBinaryOp::Gt,
        (HirBinaryOp::Lt, TargetRelation::Ge) => HirBinaryOp::Ge,
        (HirBinaryOp::Lt, TargetRelation::Eq) => HirBinaryOp::Eq,
        (HirBinaryOp::Lt, TargetRelation::Ne) => HirBinaryOp::Ne,

        (HirBinaryOp::SLt, TargetRelation::Lt) => HirBinaryOp::SLt,
        (HirBinaryOp::SLt, TargetRelation::Le) => HirBinaryOp::SLe,
        (HirBinaryOp::SLt, TargetRelation::Gt) => HirBinaryOp::SGt,
        (HirBinaryOp::SLt, TargetRelation::Ge) => HirBinaryOp::SGe,
        (HirBinaryOp::SLt, TargetRelation::Eq) => HirBinaryOp::Eq,
        (HirBinaryOp::SLt, TargetRelation::Ne) => HirBinaryOp::Ne,

        _ => less_op,
    }
}

fn try_simplify_three_way_cmp(
    op: HirBinaryOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
) -> Option<(HirBinaryOp, HirExpr, HirExpr)> {
    // We expect one side to be a 3-way compare, and the other side to be a constant
    // Match (3way, constant) or (constant, 3way)
    let (three_way_expr, const_val, is_three_way_lhs) = match (lhs, rhs) {
        (t, HirExpr::Const(c, _)) => (t, *c, true),
        (HirExpr::Const(c, _), t) => (t, *c, false),
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
            HirBinaryOp::Lt => HirBinaryOp::Gt,
            HirBinaryOp::SLt => HirBinaryOp::SGt,

            HirBinaryOp::Le => HirBinaryOp::Ge,
            HirBinaryOp::SLe => HirBinaryOp::SGe,

            HirBinaryOp::Gt => HirBinaryOp::Lt,
            HirBinaryOp::SGt => HirBinaryOp::SLt,

            HirBinaryOp::Ge => HirBinaryOp::Le,
            HirBinaryOp::SGe => HirBinaryOp::SLe,

            other => other, // Eq, Ne are symmetric
        }
    };

    // Helper to return comparison inputs in order (v, w) or (w, v)
    let make_ret = |rel: TargetRelation| -> Option<(HirBinaryOp, HirExpr, HirExpr)> {
        match rel {
            TargetRelation::Lt => Some((get_compare_op(less_op, TargetRelation::Lt), v.clone(), w.clone())),
            TargetRelation::Le => Some((get_compare_op(less_op, TargetRelation::Le), v.clone(), w.clone())),
            TargetRelation::Gt => Some((get_compare_op(less_op, TargetRelation::Lt), w.clone(), v.clone())),
            TargetRelation::Ge => Some((get_compare_op(less_op, TargetRelation::Le), w.clone(), v.clone())),
            TargetRelation::Eq => Some((get_compare_op(less_op, TargetRelation::Eq), v.clone(), w.clone())),
            TargetRelation::Ne => Some((get_compare_op(less_op, TargetRelation::Ne), v.clone(), w.clone())),
        }
    };

    match normalized_op {
        // EQUAL
        HirBinaryOp::Eq => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X == -1 => v < w
                0 => make_ret(TargetRelation::Eq),  // X == 0  => v == w
                1 => make_ret(TargetRelation::Gt),  // X == 1  => w < v (v > w)
                _ => None,
            }
        }
        // NOT EQUAL
        HirBinaryOp::Ne => {
            match const_val {
                -1 => make_ret(TargetRelation::Ge), // X != -1 => v >= w
                0 => make_ret(TargetRelation::Ne),  // X != 0  => v != w
                1 => make_ret(TargetRelation::Le),  // X != 1  => v <= w
                _ => None,
            }
        }
        // LESS THAN (signed and unsigned)
        HirBinaryOp::Lt | HirBinaryOp::SLt => {
            match const_val {
                0 => make_ret(TargetRelation::Lt),  // X < 0  => v < w
                1 => make_ret(TargetRelation::Le),  // X < 1  => v <= w
                -1 => None, // X < -1 => always false (not simplified here, handled by constant folding)
                _ => None,
            }
        }
        // LESS OR EQUAL
        HirBinaryOp::Le | HirBinaryOp::SLe => {
            match const_val {
                -1 => make_ret(TargetRelation::Lt), // X <= -1 => v < w
                0 => make_ret(TargetRelation::Le),  // X <= 0  => v <= w
                1 => None, // X <= 1 => always true (not simplified here)
                _ => None,
            }
        }
        // GREATER THAN
        HirBinaryOp::Gt | HirBinaryOp::SGt => {
            match const_val {
                0 => make_ret(TargetRelation::Gt),  // X > 0  => w < v (v > w)
                -1 => make_ret(TargetRelation::Ge), // X > -1 => w <= v (v >= w)
                1 => None, // X > 1 => always false
                _ => None,
            }
        }
        // GREATER OR EQUAL
        HirBinaryOp::Ge | HirBinaryOp::SGe => {
            match const_val {
                1 => make_ret(TargetRelation::Gt),  // X >= 1  => w < v (v > w)
                0 => make_ret(TargetRelation::Ge),  // X >= 0  => w <= v (v >= w)
                -1 => None, // X >= -1 => always true
                _ => None,
            }
        }
        _ => None,
    }
}
