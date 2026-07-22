use super::util::*;
use crate::prelude::*;

pub fn canonicalize_flag_intrinsics(expr: &DirExpr) -> Option<DirExpr> {
    canonicalize_flag_intrinsic_call(expr)
        .or_else(|| canonicalize_sborrow_compare(expr))
        .or_else(|| canonicalize_arm_compound_flag_condition(expr))
}

pub fn normalize_boolean_logic(expr: &DirExpr) -> Option<DirExpr> {
    fold_signed_zero_or_negative(expr)
        .or_else(|| fold_signed_zero_or_positive(expr))
        .or_else(|| normalize_boolean_logic_core(expr))
}

/// `(x == 0 || x < 0)` / either order → `x <= 0` (signed compares only).
/// Measured on power-class loops that test `exp > 0` as `!(exp == 0 || exp < 0)`.
fn fold_signed_zero_or_negative(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::LogicalOr | DirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let x = match (
        is_eq_zero_of(lhs.as_ref()),
        is_signed_lt_zero_of(lhs.as_ref()),
        is_eq_zero_of(rhs.as_ref()),
        is_signed_lt_zero_of(rhs.as_ref()),
    ) {
        (Some(x), None, None, Some(y)) if x == y => x,
        (None, Some(x), Some(y), None) if x == y => x,
        _ => return None,
    };
    let ty = expr_type(&x);
    // Original arm used SLt so signed order is intended.
    Some(DirExpr::Binary {
        op: DirBinaryOp::SLe,
        lhs: Box::new(x),
        rhs: Box::new(DirExpr::Const(0, ty)),
        ty: NirType::Bool,
    })
}

/// `(x == 0 || x > 0)` with signed SGt → `x >= 0` (SGe).
fn fold_signed_zero_or_positive(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::LogicalOr | DirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let x = match (
        is_eq_zero_of(lhs.as_ref()),
        is_signed_gt_zero_of(lhs.as_ref()),
        is_eq_zero_of(rhs.as_ref()),
        is_signed_gt_zero_of(rhs.as_ref()),
    ) {
        (Some(x), None, None, Some(y)) if x == y => x,
        (None, Some(x), Some(y), None) if x == y => x,
        _ => return None,
    };
    let ty = expr_type(&x);
    Some(DirExpr::Binary {
        op: DirBinaryOp::SGe,
        lhs: Box::new(x),
        rhs: Box::new(DirExpr::Const(0, ty)),
        ty: NirType::Bool,
    })
}

fn is_eq_zero_of(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if is_zero_const(rhs.as_ref()) {
        Some((**lhs).clone())
    } else if is_zero_const(lhs.as_ref()) {
        Some((**rhs).clone())
    } else {
        None
    }
}

fn is_signed_lt_zero_of(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if is_zero_const(rhs.as_ref()) {
        Some((**lhs).clone())
    } else {
        None
    }
}

fn is_signed_gt_zero_of(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::SGt,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if is_zero_const(rhs.as_ref()) {
        Some((**lhs).clone())
    } else {
        None
    }
}

fn normalize_boolean_logic_core(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_true_expr())
        }
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_false_expr())
        }
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some((**lhs).clone())
        }
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) && matches!(expr_type(rhs), NirType::Bool) => {
            // `0 != bool` → bool
            Some((**rhs).clone())
        }
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some(negate_expr((**lhs).clone()))
        }
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) && matches!(expr_type(rhs), NirType::Bool) => {
            // `0 == (a < 0)` → `!(a < 0)` → further folds to `a >= 0`
            Some(negate_expr((**rhs).clone()))
        }
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: inner,
                ..
            } => Some((**inner).clone()),
            DirExpr::Binary {
                op: DirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => Some(DirExpr::Binary {
                op: DirBinaryOp::LogicalOr,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            DirExpr::Binary {
                op: DirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => Some(DirExpr::Binary {
                op: DirBinaryOp::LogicalAnd,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            // Negate comparison operators: !(a == b) → a != b, !(a < b) → b <= a, etc.
            DirExpr::Binary { op, lhs, rhs, ty } => {
                let negated_op = match op {
                    DirBinaryOp::Eq => Some(DirBinaryOp::Ne),
                    DirBinaryOp::Ne => Some(DirBinaryOp::Eq),
                    // !(a < b)  →  b <= a
                    DirBinaryOp::Lt => None, // handled below with swapped operands
                    DirBinaryOp::Le => None,
                    DirBinaryOp::Gt => None,
                    DirBinaryOp::Ge => None,
                    DirBinaryOp::SLt => None,
                    DirBinaryOp::SLe => None,
                    DirBinaryOp::SGt => None,
                    DirBinaryOp::SGe => None,
                    _ => None,
                };
                if let Some(op2) = negated_op {
                    return Some(DirExpr::Binary {
                        op: op2,
                        lhs: lhs.clone(),
                        rhs: rhs.clone(),
                        ty: ty.clone(),
                    });
                }
                // For ordered comparisons: swap operands to canonicalize.
                // !(a < b)  →  b <= a
                // !(a <= b) →  b < a
                // !(a <s b) →  b <=s a
                // !(a <=s b) → b <s a
                match op {
                    DirBinaryOp::Lt => Some(DirExpr::Binary {
                        op: DirBinaryOp::Le,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::Le => Some(DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::Gt => Some(DirExpr::Binary {
                        op: DirBinaryOp::Ge,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::Ge => Some(DirExpr::Binary {
                        op: DirBinaryOp::Gt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::SLt => Some(DirExpr::Binary {
                        op: DirBinaryOp::SLe,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::SLe => Some(DirExpr::Binary {
                        op: DirBinaryOp::SLt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::SGt => Some(DirExpr::Binary {
                        op: DirBinaryOp::SGe,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    DirBinaryOp::SGe => Some(DirExpr::Binary {
                        op: DirBinaryOp::SGt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        },
        DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            if is_bool_true_expr(lhs) {
                Some((**rhs).clone())
            } else if is_bool_true_expr(rhs) {
                Some((**lhs).clone())
            } else if is_bool_false_expr(lhs) || is_bool_false_expr(rhs) {
                Some(bool_false_expr())
            } else if lhs == rhs {
                Some((**lhs).clone())
            } else {
                None
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            if is_bool_false_expr(lhs) {
                Some((**rhs).clone())
            } else if is_bool_false_expr(rhs) {
                Some((**lhs).clone())
            } else if is_bool_true_expr(lhs) || is_bool_true_expr(rhs) {
                Some(bool_true_expr())
            } else if lhs == rhs {
                Some((**lhs).clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignedDiffSignTest {
    Negative,
    Positive,
}

fn canonicalize_flag_intrinsic_call(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Call { target, args, .. } if target == "__carry" => {
            canonicalize_carry_intrinsic_call(args)
        }
        DirExpr::Call { target, args, .. } if target == "__scarry" || target == "__sborrow" => {
            canonicalize_zero_fold_flag_call(args)
        }
        _ => None,
    }
}

fn canonicalize_carry_intrinsic_call(args: &[DirExpr]) -> Option<DirExpr> {
    let [lhs, rhs] = args else {
        return None;
    };
    let DirExpr::Const(value, _) = rhs else {
        return None;
    };
    if *value == 0 {
        return Some(bool_false_expr());
    }
    let bits = int_type_bits(&expr_type(rhs)).or_else(|| int_type_bits(&expr_type(lhs)))?;
    let threshold = wrap_negated_const(*value, bits)?;
    Some(DirExpr::Binary {
        op: DirBinaryOp::Le,
        lhs: Box::new(DirExpr::Const(
            threshold,
            NirType::Int {
                bits,
                signed: false,
            },
        )),
        rhs: Box::new(lhs.clone()),
        ty: NirType::Bool,
    })
}

fn canonicalize_zero_fold_flag_call(args: &[DirExpr]) -> Option<DirExpr> {
    let [_, rhs] = args else {
        return None;
    };
    is_zero_const(rhs).then_some(bool_false_expr())
}

fn canonicalize_sborrow_compare(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: op @ (DirBinaryOp::Eq | DirBinaryOp::Ne),
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };

    let (a, b, sign_test) = if let Some((a, b)) = match_sborrow_call(lhs) {
        (a, b, match_signed_diff_sign_test(rhs, a, b)?)
    } else if let Some((a, b)) = match_sborrow_call(rhs) {
        (a, b, match_signed_diff_sign_test(lhs, a, b)?)
    } else {
        return None;
    };

    let (cmp_lhs, cmp_rhs, cmp_op) = match (op, sign_test) {
        (DirBinaryOp::Ne, SignedDiffSignTest::Negative) => (a.clone(), b.clone(), DirBinaryOp::SLt),
        (DirBinaryOp::Ne, SignedDiffSignTest::Positive) => (b.clone(), a.clone(), DirBinaryOp::SLt),
        (DirBinaryOp::Eq, SignedDiffSignTest::Positive) => (a.clone(), b.clone(), DirBinaryOp::SLe),
        (DirBinaryOp::Eq, SignedDiffSignTest::Negative) => (b.clone(), a.clone(), DirBinaryOp::SLe),
        _ => return None,
    };

    Some(DirExpr::Binary {
        op: cmp_op,
        lhs: Box::new(cmp_lhs),
        rhs: Box::new(cmp_rhs),
        ty: NirType::Bool,
    })
}

fn match_sborrow_call(expr: &DirExpr) -> Option<(&DirExpr, &DirExpr)> {
    let DirExpr::Call { target, args, .. } = expr else {
        return None;
    };
    if target != "__sborrow" {
        return None;
    }
    let [lhs, rhs] = args.as_slice() else {
        return None;
    };
    Some((lhs, rhs))
}

fn match_signed_diff_sign_test(
    expr: &DirExpr,
    a: &DirExpr,
    b: &DirExpr,
) -> Option<SignedDiffSignTest> {
    let DirExpr::Binary {
        op: DirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if is_zero_const(rhs) && matches_signed_difference(lhs, a, b) {
        return Some(SignedDiffSignTest::Negative);
    }
    if is_zero_const(lhs) && matches_signed_difference(rhs, a, b) {
        return Some(SignedDiffSignTest::Positive);
    }
    None
}

fn matches_signed_difference(expr: &DirExpr, a: &DirExpr, b: &DirExpr) -> bool {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && rhs.as_ref() == b,
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && matches_negated_expr(rhs, b),
        _ => false,
    }
}

fn matches_negated_expr(expr: &DirExpr, inner: &DirExpr) -> bool {
    match expr {
        DirExpr::Unary {
            op: DirUnaryOp::Neg,
            expr,
            ..
        } => expr.as_ref() == inner,
        DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            (lhs.as_ref() == inner && is_negative_one_const(rhs))
                || (rhs.as_ref() == inner && is_negative_one_const(lhs))
        }
        _ => false,
    }
}

fn is_truthy_condition_type(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_)
    )
}

pub fn canonicalize_condition_expr(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Ne | DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => {
            let is_eq = matches!(
                expr,
                DirExpr::Binary {
                    op: DirBinaryOp::Eq,
                    ..
                }
            );
            match lhs.as_ref() {
                DirExpr::Binary {
                    op: inner_op @ (DirBinaryOp::Sub | DirBinaryOp::Xor),
                    lhs: inner_lhs,
                    rhs: inner_rhs,
                    ty: inner_ty,
                } => {
                    let new_op = if is_eq {
                        DirBinaryOp::Eq
                    } else {
                        DirBinaryOp::Ne
                    };
                    return Some(DirExpr::Binary {
                        op: new_op,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty: NirType::Bool,
                    });
                }
                _ => {}
            }
            if is_truthy_condition_type(&expr_type(lhs)) {
                if is_eq {
                    Some(negate_expr((**lhs).clone()))
                } else {
                    Some((**lhs).clone())
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn canonicalize_arm_compound_flag_condition(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::LogicalAnd,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };

    // Check if one side is a Ne comparison and the other is a SLe comparison
    if let Some((ne_a, ne_b)) = match_ne_comparison(lhs) {
        if let Some((sle_a, sle_b)) = match_sle_comparison(rhs) {
            if (ne_a == sle_a && ne_b == sle_b) || (ne_a == sle_b && ne_b == sle_a) {
                return Some(DirExpr::Binary {
                    op: DirBinaryOp::SLt,
                    lhs: Box::new(sle_a.clone()),
                    rhs: Box::new(sle_b.clone()),
                    ty: NirType::Bool,
                });
            }
        }
    }
    if let Some((ne_a, ne_b)) = match_ne_comparison(rhs) {
        if let Some((sle_a, sle_b)) = match_sle_comparison(lhs) {
            if (ne_a == sle_a && ne_b == sle_b) || (ne_a == sle_b && ne_b == sle_a) {
                return Some(DirExpr::Binary {
                    op: DirBinaryOp::SLt,
                    lhs: Box::new(sle_a.clone()),
                    rhs: Box::new(sle_b.clone()),
                    ty: NirType::Bool,
                });
            }
        }
    }

    None
}

fn match_ne_comparison<'a>(expr: &'a DirExpr) -> Option<(&'a DirExpr, &'a DirExpr)> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            if is_zero_const(rhs.as_ref()) {
                if let DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    lhs: inner_lhs,
                    rhs: inner_rhs,
                    ..
                } = lhs.as_ref()
                {
                    return Some((inner_lhs.as_ref(), inner_rhs.as_ref()));
                }
            }
            Some((lhs.as_ref(), rhs.as_ref()))
        }
        _ => None,
    }
}

fn match_sle_comparison<'a>(expr: &'a DirExpr) -> Option<(&'a DirExpr, &'a DirExpr)> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => Some((lhs.as_ref(), rhs.as_ref())),
        DirExpr::Binary {
            op: op @ (DirBinaryOp::Eq | DirBinaryOp::Ne),
            lhs,
            rhs,
            ..
        } => {
            let (a, b, sign_test) = if let Some((a, b)) = match_sborrow_call(lhs) {
                (a, b, match_signed_diff_sign_test(rhs, a, b)?)
            } else if let Some((a, b)) = match_sborrow_call(rhs) {
                (a, b, match_signed_diff_sign_test(lhs, a, b)?)
            } else {
                return None;
            };
            if *op == DirBinaryOp::Eq && sign_test == SignedDiffSignTest::Positive {
                Some((a, b))
            } else if *op == DirBinaryOp::Eq && sign_test == SignedDiffSignTest::Negative {
                Some((b, a))
            } else {
                None
            }
        }
        _ => None,
    }
}
