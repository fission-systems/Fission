use super::util::*;
use crate::prelude::*;

pub fn canonicalize_flag_intrinsics(expr: &PreHirExpr) -> Option<PreHirExpr> {
    canonicalize_flag_intrinsic_call(expr)
        .or_else(|| canonicalize_sborrow_compare(expr))
        .or_else(|| canonicalize_arm_compound_flag_condition(expr))
}

pub fn normalize_boolean_logic(expr: &PreHirExpr) -> Option<PreHirExpr> {
    fold_signed_zero_or_negative(expr)
        .or_else(|| fold_signed_zero_or_positive(expr))
        .or_else(|| normalize_boolean_logic_core(expr))
}

/// `(x == 0 || x < 0)` / either order → `x <= 0` (signed compares only).
/// Measured on power-class loops that test `exp > 0` as `!(exp == 0 || exp < 0)`.
fn fold_signed_zero_or_negative(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalOr | PreHirBinaryOp::Or,
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
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::SLe,
        lhs: Box::new(x),
        rhs: Box::new(PreHirExpr::Const(0, ty)),
        ty: NirType::Bool,
    })
}

/// `(x == 0 || x > 0)` with signed SGt → `x >= 0` (SGe).
fn fold_signed_zero_or_positive(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalOr | PreHirBinaryOp::Or,
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
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::SGe,
        lhs: Box::new(x),
        rhs: Box::new(PreHirExpr::Const(0, ty)),
        ty: NirType::Bool,
    })
}

fn is_eq_zero_of(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Eq,
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

fn is_signed_lt_zero_of(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::SLt,
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

fn is_signed_gt_zero_of(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::SGt,
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

fn normalize_boolean_logic_core(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_true_expr())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_false_expr())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some((**lhs).clone())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) && matches!(expr_type(rhs), NirType::Bool) => {
            // `0 != bool` → bool
            Some((**rhs).clone())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some(negate_expr((**lhs).clone()))
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) && matches!(expr_type(rhs), NirType::Bool) => {
            // `0 == (a < 0)` → `!(a < 0)` → further folds to `a >= 0`
            Some(negate_expr((**rhs).clone()))
        }
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                expr: inner,
                ..
            } => Some((**inner).clone()),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            // Negate comparison operators: !(a == b) → a != b, !(a < b) → b <= a, etc.
            PreHirExpr::Binary { op, lhs, rhs, ty } => {
                let negated_op = match op {
                    PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Ne),
                    PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Eq),
                    // !(a < b)  →  b <= a
                    PreHirBinaryOp::Lt => None, // handled below with swapped operands
                    PreHirBinaryOp::Le => None,
                    PreHirBinaryOp::Gt => None,
                    PreHirBinaryOp::Ge => None,
                    PreHirBinaryOp::SLt => None,
                    PreHirBinaryOp::SLe => None,
                    PreHirBinaryOp::SGt => None,
                    PreHirBinaryOp::SGe => None,
                    _ => None,
                };
                if let Some(op2) = negated_op {
                    return Some(PreHirExpr::Binary {
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
                    PreHirBinaryOp::Lt => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Le,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::Le => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Lt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::Gt => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Ge,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::Ge => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Gt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::SLt => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::SLe,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::SLe => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::SLt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::SGt => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::SGe,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    PreHirBinaryOp::SGe => Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::SGt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        },
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
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

fn canonicalize_flag_intrinsic_call(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Call { target, args, .. } if target == "__carry" => {
            canonicalize_carry_intrinsic_call(args)
        }
        PreHirExpr::Call { target, args, .. } if target == "__scarry" || target == "__sborrow" => {
            canonicalize_zero_fold_flag_call(args)
        }
        _ => None,
    }
}

fn canonicalize_carry_intrinsic_call(args: &[PreHirExpr]) -> Option<PreHirExpr> {
    let [lhs, rhs] = args else {
        return None;
    };
    let PreHirExpr::Const(value, _) = rhs else {
        return None;
    };
    if *value == 0 {
        return Some(bool_false_expr());
    }
    let bits = int_type_bits(&expr_type(rhs)).or_else(|| int_type_bits(&expr_type(lhs)))?;
    let threshold = wrap_negated_const(*value, bits)?;
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Le,
        lhs: Box::new(PreHirExpr::Const(
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

fn canonicalize_zero_fold_flag_call(args: &[PreHirExpr]) -> Option<PreHirExpr> {
    let [_, rhs] = args else {
        return None;
    };
    is_zero_const(rhs).then_some(bool_false_expr())
}

fn canonicalize_sborrow_compare(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: op @ (PreHirBinaryOp::Eq | PreHirBinaryOp::Ne),
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
        (PreHirBinaryOp::Ne, SignedDiffSignTest::Negative) => (a.clone(), b.clone(), PreHirBinaryOp::SLt),
        (PreHirBinaryOp::Ne, SignedDiffSignTest::Positive) => (b.clone(), a.clone(), PreHirBinaryOp::SLt),
        (PreHirBinaryOp::Eq, SignedDiffSignTest::Positive) => (a.clone(), b.clone(), PreHirBinaryOp::SLe),
        (PreHirBinaryOp::Eq, SignedDiffSignTest::Negative) => (b.clone(), a.clone(), PreHirBinaryOp::SLe),
        _ => return None,
    };

    Some(PreHirExpr::Binary {
        op: cmp_op,
        lhs: Box::new(cmp_lhs),
        rhs: Box::new(cmp_rhs),
        ty: NirType::Bool,
    })
}

fn match_sborrow_call(expr: &PreHirExpr) -> Option<(&PreHirExpr, &PreHirExpr)> {
    let PreHirExpr::Call { target, args, .. } = expr else {
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
    expr: &PreHirExpr,
    a: &PreHirExpr,
    b: &PreHirExpr,
) -> Option<SignedDiffSignTest> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::SLt,
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

fn matches_signed_difference(expr: &PreHirExpr, a: &PreHirExpr, b: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && rhs.as_ref() == b,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && matches_negated_expr(rhs, b),
        _ => false,
    }
}

fn matches_negated_expr(expr: &PreHirExpr, inner: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Neg,
            expr,
            ..
        } => expr.as_ref() == inner,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
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

pub fn canonicalize_condition_expr(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne | PreHirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => {
            let is_eq = matches!(
                expr,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Eq,
                    ..
                }
            );
            match lhs.as_ref() {
                PreHirExpr::Binary {
                    op: inner_op @ (PreHirBinaryOp::Sub | PreHirBinaryOp::Xor),
                    lhs: inner_lhs,
                    rhs: inner_rhs,
                    ty: inner_ty,
                } => {
                    let new_op = if is_eq {
                        PreHirBinaryOp::Eq
                    } else {
                        PreHirBinaryOp::Ne
                    };
                    return Some(PreHirExpr::Binary {
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

pub fn canonicalize_arm_compound_flag_condition(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalAnd,
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
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
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
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
                    lhs: Box::new(sle_a.clone()),
                    rhs: Box::new(sle_b.clone()),
                    ty: NirType::Bool,
                });
            }
        }
    }

    None
}

fn match_ne_comparison<'a>(expr: &'a PreHirExpr) -> Option<(&'a PreHirExpr, &'a PreHirExpr)> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            if is_zero_const(rhs.as_ref()) {
                if let PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
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

fn match_sle_comparison<'a>(expr: &'a PreHirExpr) -> Option<(&'a PreHirExpr, &'a PreHirExpr)> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } => Some((lhs.as_ref(), rhs.as_ref())),
        PreHirExpr::Binary {
            op: op @ (PreHirBinaryOp::Eq | PreHirBinaryOp::Ne),
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
            if *op == PreHirBinaryOp::Eq && sign_test == SignedDiffSignTest::Positive {
                Some((a, b))
            } else if *op == PreHirBinaryOp::Eq && sign_test == SignedDiffSignTest::Negative {
                Some((b, a))
            } else {
                None
            }
        }
        _ => None,
    }
}
