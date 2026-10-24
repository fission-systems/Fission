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
        PreHirStmt::Block(body) => {
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
            changed |= simplify_expr(cond);
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
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
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= simplify_expr(cond);
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= simplify_expr(expr);
            for case in cases {
                changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            changed |= simplify_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
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
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
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
    let (three_way_expr, const_val, const_ty, is_three_way_lhs) = match (lhs, rhs) {
        (t, PreHirExpr::Const(c, cty)) => (t, *c, cty, true),
        (PreHirExpr::Const(c, cty), t) => (t, *c, cty, false),
        _ => return None,
    };

    let is_minus_one = const_val == -1
        || match const_ty {
            NirType::Int { bits, .. } if *bits < 64 => {
                const_val as u64 == (1u64.checked_shl(*bits).unwrap_or(0).wrapping_sub(1))
            }
            _ => false,
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
            if is_minus_one {
                make_ret(TargetRelation::Lt) // X == -1 (or uMAX) => v < w
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Eq), // X == 0  => v == w
                    1 => make_ret(TargetRelation::Gt), // X == 1  => w < v (v > w)
                    _ => None,
                }
            }
        }
        // NOT EQUAL
        PreHirBinaryOp::Ne => {
            if is_minus_one {
                make_ret(TargetRelation::Ge) // X != -1 (or uMAX) => v >= w
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Ne), // X != 0  => v != w
                    1 => make_ret(TargetRelation::Le), // X != 1  => v <= w
                    _ => None,
                }
            }
        }
        // SIGNED LESS THAN
        PreHirBinaryOp::SLt => {
            if is_minus_one {
                None // X < -1 => always false
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Lt), // X < 0  => v < w
                    1 => make_ret(TargetRelation::Le), // X < 1  => v <= w
                    _ => None,
                }
            }
        }
        // SIGNED LESS OR EQUAL
        PreHirBinaryOp::SLe => {
            if is_minus_one {
                make_ret(TargetRelation::Lt) // X <= -1 => v < w
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Le), // X <= 0 => v <= w
                    1 => None,                         // X <= 1 => always true
                    _ => None,
                }
            }
        }
        // SIGNED GREATER THAN
        PreHirBinaryOp::SGt => {
            if is_minus_one {
                make_ret(TargetRelation::Ge) // X > -1 => v >= w
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Gt), // X > 0  => v > w
                    1 => None,                         // X > 1 => always false
                    _ => None,
                }
            }
        }
        // SIGNED GREATER OR EQUAL
        PreHirBinaryOp::SGe => {
            if is_minus_one {
                None // X >= -1 => always true
            } else {
                match const_val {
                    1 => make_ret(TargetRelation::Gt), // X >= 1  => v > w
                    0 => make_ret(TargetRelation::Ge), // X >= 0  => v >= w
                    _ => None,
                }
            }
        }
        // UNSIGNED LESS THAN: X ∈ {0, 1, uMAX}
        PreHirBinaryOp::Lt => {
            if is_minus_one {
                make_ret(TargetRelation::Ge) // X <u uMAX => X ∈ {0, 1} => v >= w
            } else {
                match const_val {
                    0 => None,                         // X <u 0 => always false
                    1 => make_ret(TargetRelation::Eq), // X <u 1 => X == 0 => v == w
                    _ => None,
                }
            }
        }
        // UNSIGNED LESS OR EQUAL
        PreHirBinaryOp::Le => {
            if is_minus_one {
                None // X <=u uMAX => always true
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Eq), // X <=u 0 => X == 0 => v == w
                    1 => make_ret(TargetRelation::Ge), // X <=u 1 => X ∈ {0, 1} => v >= w
                    _ => None,
                }
            }
        }
        // UNSIGNED GREATER THAN
        PreHirBinaryOp::Gt => {
            if is_minus_one {
                None // X >u uMAX => always false
            } else {
                match const_val {
                    0 => make_ret(TargetRelation::Ne), // X >u 0 => X ∈ {1, uMAX} => v != w
                    1 => make_ret(TargetRelation::Lt), // X >u 1 => X == uMAX => v < w
                    _ => None,
                }
            }
        }
        // UNSIGNED GREATER OR EQUAL
        PreHirBinaryOp::Ge => {
            if is_minus_one {
                make_ret(TargetRelation::Lt) // X >=u uMAX => X == uMAX => v < w
            } else {
                match const_val {
                    0 => None,                         // X >=u 0 => always true
                    1 => make_ret(TargetRelation::Ne), // X >=u 1 => X ∈ {1, uMAX} => v != w
                    _ => None,
                }
            }
        }
        _ => None,
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

    fn make_3way_expr() -> PreHirExpr {
        // (zext(a < b) + zext(a <= b)) + -1
        let a = PreHirExpr::Var("a".to_string());
        let b = PreHirExpr::Var("b".to_string());
        let term1 = PreHirExpr::Cast {
            ty: u32_ty(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Lt,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Bool,
            }),
        };
        let term2 = PreHirExpr::Cast {
            ty: u32_ty(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Le,
                lhs: Box::new(a),
                rhs: Box::new(b),
                ty: NirType::Bool,
            }),
        };
        let term3 = PreHirExpr::Const(-1, u32_ty());
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(term1),
                rhs: Box::new(term2),
                ty: u32_ty(),
            }),
            rhs: Box::new(term3),
            ty: u32_ty(),
        }
    }

    #[test]
    fn three_way_unsigned_comparisons_map_accurately() {
        let three_way = make_3way_expr();

        // X <u 1 => a == b
        let (op, lhs, rhs) = try_simplify_three_way_cmp(
            PreHirBinaryOp::Lt,
            &three_way,
            &PreHirExpr::Const(1, u32_ty()),
        )
        .expect("simplify <u 1");
        assert_eq!(op, PreHirBinaryOp::Eq);
        assert_eq!(lhs, PreHirExpr::Var("a".to_string()));
        assert_eq!(rhs, PreHirExpr::Var("b".to_string()));

        // X >u 1 => a < b
        let (op, lhs, rhs) = try_simplify_three_way_cmp(
            PreHirBinaryOp::Gt,
            &three_way,
            &PreHirExpr::Const(1, u32_ty()),
        )
        .expect("simplify >u 1");
        assert_eq!(op, PreHirBinaryOp::Lt);
        assert_eq!(lhs, PreHirExpr::Var("a".to_string()));
        assert_eq!(rhs, PreHirExpr::Var("b".to_string()));

        // X >u 0 => a != b
        let (op, lhs, rhs) = try_simplify_three_way_cmp(
            PreHirBinaryOp::Gt,
            &three_way,
            &PreHirExpr::Const(0, u32_ty()),
        )
        .expect("simplify >u 0");
        assert_eq!(op, PreHirBinaryOp::Ne);
        assert_eq!(lhs, PreHirExpr::Var("a".to_string()));
        assert_eq!(rhs, PreHirExpr::Var("b".to_string()));

        // X <=u 1 => a >= b (which is b <= a)
        let (op, lhs, rhs) = try_simplify_three_way_cmp(
            PreHirBinaryOp::Le,
            &three_way,
            &PreHirExpr::Const(1, u32_ty()),
        )
        .expect("simplify <=u 1");
        assert_eq!(op, PreHirBinaryOp::Le);
        assert_eq!(lhs, PreHirExpr::Var("b".to_string()));
        assert_eq!(rhs, PreHirExpr::Var("a".to_string()));
    }
}
