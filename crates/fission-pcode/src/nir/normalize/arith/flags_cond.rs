use super::super::*;
use super::util::*;

pub(crate) fn canonicalize_flag_intrinsics(expr: &HirExpr) -> Option<HirExpr> {
    canonicalize_flag_intrinsic_call(expr).or_else(|| canonicalize_sborrow_compare(expr))
}


pub(crate) fn normalize_boolean_logic(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_true_expr())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if lhs == rhs && is_self_comparable_non_float_type(&expr_type(lhs)) => {
            Some(bool_false_expr())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some((**lhs).clone())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some(negate_expr((**lhs).clone()))
        }
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: inner,
                ..
            } => Some((**inner).clone()),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            // Negate comparison operators: !(a == b) → a != b, !(a < b) → b <= a, etc.
            HirExpr::Binary { op, lhs, rhs, ty } => {
                let negated_op = match op {
                    HirBinaryOp::Eq => Some(HirBinaryOp::Ne),
                    HirBinaryOp::Ne => Some(HirBinaryOp::Eq),
                    // !(a < b)  →  b <= a
                    HirBinaryOp::Lt => None, // handled below with swapped operands
                    HirBinaryOp::Le => None,
                    HirBinaryOp::SLt => None,
                    HirBinaryOp::SLe => None,
                    _ => None,
                };
                if let Some(op2) = negated_op {
                    return Some(HirExpr::Binary {
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
                    HirBinaryOp::Lt => Some(HirExpr::Binary {
                        op: HirBinaryOp::Le,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    HirBinaryOp::Le => Some(HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    HirBinaryOp::SLt => Some(HirExpr::Binary {
                        op: HirBinaryOp::SLe,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    HirBinaryOp::SLe => Some(HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: rhs.clone(),
                        rhs: lhs.clone(),
                        ty: ty.clone(),
                    }),
                    _ => None,
                }
            }
            _ => None,
        },
        HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
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
        HirExpr::Binary {
            op: HirBinaryOp::LogicalOr,
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

fn canonicalize_flag_intrinsic_call(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Call { target, args, .. } if target == "__carry" => {
            canonicalize_carry_intrinsic_call(args)
        }
        HirExpr::Call { target, args, .. } if target == "__scarry" || target == "__sborrow" => {
            canonicalize_zero_fold_flag_call(args)
        }
        _ => None,
    }
}

fn canonicalize_carry_intrinsic_call(args: &[HirExpr]) -> Option<HirExpr> {
    let [lhs, rhs] = args else {
        return None;
    };
    let HirExpr::Const(value, _) = rhs else {
        return None;
    };
    if *value == 0 {
        return Some(bool_false_expr());
    }
    let bits = int_type_bits(&expr_type(rhs)).or_else(|| int_type_bits(&expr_type(lhs)))?;
    let threshold = wrap_negated_const(*value, bits)?;
    Some(HirExpr::Binary {
        op: HirBinaryOp::Le,
        lhs: Box::new(HirExpr::Const(
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

fn canonicalize_zero_fold_flag_call(args: &[HirExpr]) -> Option<HirExpr> {
    let [_, rhs] = args else {
        return None;
    };
    is_zero_const(rhs).then_some(bool_false_expr())
}

fn canonicalize_sborrow_compare(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: op @ (HirBinaryOp::Eq | HirBinaryOp::Ne),
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
        (HirBinaryOp::Ne, SignedDiffSignTest::Negative) => (a.clone(), b.clone(), HirBinaryOp::SLt),
        (HirBinaryOp::Ne, SignedDiffSignTest::Positive) => (b.clone(), a.clone(), HirBinaryOp::SLt),
        (HirBinaryOp::Eq, SignedDiffSignTest::Positive) => (a.clone(), b.clone(), HirBinaryOp::SLe),
        (HirBinaryOp::Eq, SignedDiffSignTest::Negative) => (b.clone(), a.clone(), HirBinaryOp::SLe),
        _ => return None,
    };

    Some(HirExpr::Binary {
        op: cmp_op,
        lhs: Box::new(cmp_lhs),
        rhs: Box::new(cmp_rhs),
        ty: NirType::Bool,
    })
}

fn match_sborrow_call(expr: &HirExpr) -> Option<(&HirExpr, &HirExpr)> {
    let HirExpr::Call { target, args, .. } = expr else {
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
    expr: &HirExpr,
    a: &HirExpr,
    b: &HirExpr,
) -> Option<SignedDiffSignTest> {
    let HirExpr::Binary {
        op: HirBinaryOp::SLt,
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

fn matches_signed_difference(expr: &HirExpr, a: &HirExpr, b: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && rhs.as_ref() == b,
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => lhs.as_ref() == a && matches_negated_expr(rhs, b),
        _ => false,
    }
}

fn matches_negated_expr(expr: &HirExpr, inner: &HirExpr) -> bool {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Neg,
            expr,
            ..
        } => expr.as_ref() == inner,
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
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

pub(crate) fn canonicalize_condition_expr(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && is_truthy_condition_type(&expr_type(lhs)) => {
            Some((**lhs).clone())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && is_truthy_condition_type(&expr_type(lhs)) => {
            Some(negate_expr((**lhs).clone()))
        }
        _ => None,
    }
}
