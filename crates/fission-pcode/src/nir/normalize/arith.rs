use super::*;

pub(super) fn canonicalize_integer_expr(expr: &HirExpr) -> Option<HirExpr> {
    canonicalize_cast_expr(expr)
}

pub(super) fn canonicalize_flag_intrinsics(expr: &HirExpr) -> Option<HirExpr> {
    canonicalize_flag_intrinsic_call(expr).or_else(|| canonicalize_sborrow_compare(expr))
}

pub(super) fn recognize_mod_div_power_of_two(expr: &HirExpr) -> Option<HirExpr> {
    normalize_signed_power_of_two_mod(expr)
        .or_else(|| normalize_unsigned_power_of_two_mod(expr))
        .or_else(|| normalize_signed_power_of_two_div(expr))
        .or_else(|| normalize_unsigned_power_of_two_div(expr))
}

pub(super) fn recognize_hi_lo_extract(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Cast { ty, expr: inner } if is_integer_type(ty) => match inner.as_ref() {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                let HirExpr::Const(mask, _) = rhs.as_ref() else {
                    return None;
                };
                let mask_limit = full_mask_for_type(ty)?;
                if *mask == mask_limit {
                    return Some(HirExpr::Cast {
                        ty: ty.clone(),
                        expr: lhs.clone(),
                    });
                }
                None
            }
            HirExpr::Binary {
                op: HirBinaryOp::Shr | HirBinaryOp::Sar,
                lhs,
                rhs,
                ..
            } => {
                let HirExpr::Const(shift, _) = rhs.as_ref() else {
                    return None;
                };
                let inner_ty = expr_type(lhs);
                let Some(target_bits) = int_type_bits(ty) else {
                    return None;
                };
                let Some(source_bits) = int_type_bits(&inner_ty) else {
                    return None;
                };
                if *shift == i64::from(source_bits.saturating_sub(target_bits)) {
                    Some(HirExpr::Cast {
                        ty: ty.clone(),
                        expr: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Shr,
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                            ty: inner_ty,
                        }),
                    })
                } else {
                    None
                }
            }
            _ => None,
        },
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ty,
        } if is_integer_type(ty) => {
            let HirExpr::Const(mask, _) = rhs.as_ref() else {
                return None;
            };
            let mask_limit = full_mask_for_type(ty)?;
            if *mask != mask_limit {
                return None;
            }
            Some(HirExpr::Cast {
                ty: ty.clone(),
                expr: lhs.clone(),
            })
        }
        _ => None,
    }
}

pub(super) fn recognize_wide_integer_recombine(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: hi_expr,
        rhs: hi_shift,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = hi_shift.as_ref() else {
        return None;
    };
    let Some(total_bits) = int_type_bits(ty) else {
        return None;
    };
    let high = extract_high_part(hi_expr, *shift_amount, total_bits)?;
    let low = extract_low_part(rhs, *shift_amount)?;
    if high.source != low.source
        || high.width_bits != low.width_bits
        || high.shift_bits != low.shift_bits
    {
        return None;
    }
    let source_ty = expr_type(&high.source);
    if source_ty == *ty {
        Some(high.source)
    } else if matches!(source_ty, NirType::Unknown) {
        Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(high.source),
        })
    } else {
        None
    }
}

#[derive(Clone)]
struct WidePart {
    source: HirExpr,
    width_bits: u32,
    shift_bits: i64,
}

fn extract_high_part(expr: &HirExpr, shift_amount: i64, total_bits: u32) -> Option<WidePart> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = inner.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(inner_shift, _) = rhs.as_ref() else {
        return None;
    };
    if *inner_shift != shift_amount {
        return None;
    }
    let width_bits = int_type_bits(ty)?;
    if shift_amount != i64::from(total_bits.saturating_sub(width_bits)) {
        return None;
    }
    Some(WidePart {
        source: (**lhs).clone(),
        width_bits,
        shift_bits: shift_amount,
    })
}

fn extract_low_part(expr: &HirExpr, shift_amount: i64) -> Option<WidePart> {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
            let width_bits = int_type_bits(ty)?;
            Some(WidePart {
                source: (**inner).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(mask, _) = rhs.as_ref() else {
                return None;
            };
            let width_bits = shift_amount as u32;
            let expected_mask = full_mask_for_bits(width_bits)?;
            if *mask != expected_mask {
                return None;
            }
            Some(WidePart {
                source: (**lhs).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(modulus, _) = rhs.as_ref() else {
                return None;
            };
            let width_bits = shift_amount as u32;
            let expected_modulus = 1i64.checked_shl(width_bits)?;
            if *modulus != expected_modulus {
                return None;
            }
            Some(WidePart {
                source: (**lhs).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        _ => None,
    }
}

fn canonicalize_cast_expr(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };

    if should_preserve_non_scalar_cast(ty) {
        if let HirExpr::Cast {
            ty: inner_ty,
            expr: inner_inner,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                return Some(HirExpr::Cast {
                    ty: ty.clone(),
                    expr: inner_inner.clone(),
                });
            }
        }
        return None;
    }

    let inner_ty = expr_type(inner);
    if inner_ty == *ty {
        return Some((**inner).clone());
    }

    let HirExpr::Cast {
        ty: inner_cast_ty,
        expr: inner_inner,
    } = inner.as_ref()
    else {
        return None;
    };

    if inner_cast_ty == ty {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    if should_drop_inner_scalar_cast(ty, inner_cast_ty, &expr_type(inner_inner)) {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    None
}

fn should_preserve_non_scalar_cast(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Ptr(_) | NirType::Aggregate { .. } | NirType::Float { .. }
    )
}

fn scalar_cast_signature(ty: &NirType) -> Option<(u32, bool)> {
    match ty {
        NirType::Bool => Some((1, false)),
        NirType::Int { bits, signed } => Some((*bits, *signed)),
        _ => None,
    }
}

fn source_is_scalarish(ty: &NirType) -> bool {
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. })
}

fn should_drop_inner_scalar_cast(
    outer_ty: &NirType,
    inner_ty: &NirType,
    source_ty: &NirType,
) -> bool {
    if should_preserve_non_scalar_cast(outer_ty) || should_preserve_non_scalar_cast(inner_ty) {
        return false;
    }
    let Some((outer_bits, outer_signed)) = scalar_cast_signature(outer_ty) else {
        return false;
    };
    let Some((inner_bits, inner_signed)) = scalar_cast_signature(inner_ty) else {
        return false;
    };
    if !source_is_scalarish(source_ty) {
        return false;
    }

    if outer_bits < inner_bits {
        return true;
    }

    outer_bits == inner_bits && outer_signed == inner_signed
}

pub(super) fn normalize_boolean_logic(expr: &HirExpr) -> Option<HirExpr> {
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

pub(super) fn canonicalize_condition_expr(expr: &HirExpr) -> Option<HirExpr> {
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

fn is_truthy_condition_type(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_)
    )
}

fn normalize_unsigned_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(
        mask,
        NirType::Int {
            bits,
            signed: false,
        },
    ) = rhs.as_ref()
    else {
        return None;
    };
    if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) {
        return None;
    }
    let divisor = (*mask as i128) + 1;
    if divisor <= 1 || (divisor & (divisor - 1)) != 0 {
        return None;
    }
    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor as i64,
            NirType::Int {
                bits: *bits,
                signed: false,
            },
        )),
        ty: NirType::Int {
            bits: *bits,
            signed: false,
        },
    })
}

fn normalize_unsigned_power_of_two_div(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = rhs.as_ref() else {
        return None;
    };
    let width = match ty {
        NirType::Int {
            bits,
            signed: false,
        } => *bits,
        _ => return None,
    };
    match expr_type(lhs) {
        NirType::Int {
            bits,
            signed: false,
        } if bits == width => {}
        NirType::Unknown => {}
        _ => return None,
    }
    if *shift_amount < 0 || *shift_amount >= i64::from(width) {
        return None;
    }
    if *shift_amount == i64::from(width.saturating_sub(1)) {
        return None;
    }
    if (*shift_amount as u32) * 2 >= width && *shift_amount % 8 == 0 {
        return None;
    }
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
    Some(HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: false,
            },
        )),
        ty: NirType::Int {
            bits: width,
            signed: false,
        },
    })
}

fn normalize_signed_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    if let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    {
        let HirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
            return None;
        };
        let HirExpr::Binary {
            op: HirBinaryOp::Div,
            lhs: div_lhs,
            rhs: div_rhs,
            ..
        } = shl_inner.as_ref()
        else {
            return None;
        };
        let HirExpr::Const(divisor, _) = div_rhs.as_ref() else {
            return None;
        };
        if div_lhs.as_ref() == lhs.as_ref()
            && *divisor > 1
            && (*divisor & (*divisor - 1)) == 0
            && *divisor == (1_i64.checked_shl(*shift_amount as u32)?)
        {
            let width = match ty {
                NirType::Int { bits, signed: true } => *bits,
                _ => 64,
            };
            return Some(HirExpr::Binary {
                op: HirBinaryOp::Mod,
                lhs: lhs.clone(),
                rhs: Box::new(HirExpr::Const(
                    *divisor,
                    NirType::Int {
                        bits: width,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: width,
                    signed: true,
                },
            });
        }
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Sar,
        lhs: sar_inner,
        rhs: sar_rhs,
        ..
    } = shl_inner.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(sar_shift, _) = sar_rhs.as_ref() else {
        return None;
    };
    if sar_shift != shift_amount {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = sar_inner.as_ref()
    else {
        return None;
    };
    if add_lhs.as_ref() != lhs.as_ref() {
        return None;
    }
    let (sign_source, sign_shift, mask) = match add_rhs.as_ref() {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *divisor - 1)
        }
        _ => return None,
    };
    if sign_source != lhs.as_ref() {
        return None;
    }

    let width = match ty {
        NirType::Int { bits, signed: true } => *bits,
        _ => 64,
    };
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
    if sign_shift != i64::from(width.saturating_sub(1)) || mask != divisor - 1 {
        return None;
    }

    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: true,
            },
        )),
        ty: NirType::Int {
            bits: width,
            signed: true,
        },
    })
}

fn normalize_signed_power_of_two_div(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sar,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = rhs.as_ref() else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let (sign_source, sign_shift, mask) = match add_rhs.as_ref() {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *divisor - 1)
        }
        _ => return None,
    };
    if sign_source != add_lhs.as_ref() {
        return None;
    }

    let width = match ty {
        NirType::Int { bits, signed: true } => *bits,
        _ => return None,
    };
    if *shift_amount < 0 || *shift_amount >= i64::from(width) {
        return None;
    }
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
    if sign_shift != i64::from(width.saturating_sub(1)) || mask != divisor - 1 {
        return None;
    }

    Some(HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: add_lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: true,
            },
        )),
        ty: NirType::Int {
            bits: width,
            signed: true,
        },
    })
}

pub(super) fn collapse_zero_offset_cast(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Load { ptr, ty } => {
            let HirExpr::PtrOffset { base, offset } = ptr.as_ref() else {
                return None;
            };
            if *offset != 0 {
                return None;
            }
            Some(HirExpr::Load {
                ptr: base.clone(),
                ty: ty.clone(),
            })
        }
        HirExpr::PtrOffset { base, offset } if *offset == 0 => Some((**base).clone()),
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } if matches!(index.as_ref(), HirExpr::Const(0, _))
            && !matches!(base.as_ref(), HirExpr::Var(_)) =>
        {
            Some(HirExpr::Load {
                ptr: base.clone(),
                ty: elem_ty.clone(),
            })
        }
        _ => None,
    }
}

pub(super) fn cleanup_arithmetic_wrappers(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) => {
            Some(HirExpr::Const(0, expr_type(lhs)))
        }
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(lhs.as_ref(), &expr_type(rhs)) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => match lhs.as_ref() {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: and_lhs,
                rhs: and_rhs,
                ty: _,
            } if is_one_const(and_rhs.as_ref()) && matches!(expr_type(and_lhs), NirType::Bool) => {
                Some((**and_lhs).clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_zero_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(0, _))
}

fn is_one_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(1, _))
}

fn is_negative_one_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(-1, _))
}

fn bool_false_expr() -> HirExpr {
    HirExpr::Const(0, NirType::Bool)
}

fn bool_true_expr() -> HirExpr {
    HirExpr::Const(1, NirType::Bool)
}

fn is_bool_false_expr(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(0, NirType::Bool))
}

fn is_bool_true_expr(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(1, NirType::Bool))
}

fn is_integer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

fn is_self_comparable_non_float_type(ty: &NirType) -> bool {
    matches!(
        ty,
        NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_)
    )
}

pub(super) fn int_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn full_mask_for_bits(bits: u32) -> Option<i64> {
    match bits {
        0 => None,
        1..=62 => Some((1_i64 << bits) - 1),
        63 => Some(i64::MAX),
        _ => None,
    }
}

fn wrap_negated_const(value: i64, bits: u32) -> Option<i64> {
    if bits == 0 || bits > 64 {
        return None;
    }
    let mask = if bits == 64 {
        u128::from(u64::MAX)
    } else {
        (1_u128 << bits) - 1
    };
    let unsigned = (value as i128 as u128) & mask;
    let negated = unsigned.wrapping_neg() & mask;
    Some(negated as i64)
}

pub(super) fn recognize_magic_number_division(expr: &HirExpr) -> Option<HirExpr> {
    let mut current = expr;
    let mut n = 0u32;
    let mut x_size_bits = None;
    let mut ext_ty_bits = None;

    if let HirExpr::Cast { ty, expr: inner } = current {
        if let Some(bits) = int_type_bits(ty) {
            ext_ty_bits = Some(bits);
            current = inner.as_ref();
        }
    }

    let mut is_signed_shift = false;
    if let HirExpr::Binary { op, lhs, rhs, .. } = current {
        if matches!(op, HirBinaryOp::Shr | HirBinaryOp::Sar) {
            if let HirExpr::Const(shift_amt, _) = rhs.as_ref() {
                n += *shift_amt as u32;
                is_signed_shift = matches!(op, HirBinaryOp::Sar);
                current = lhs.as_ref();
            }
        }
    }

    if let HirExpr::Binary { op: HirBinaryOp::Mul, lhs, rhs, ty: mul_ty } = current {
        let (x_ext, y_expr) = if let HirExpr::Const(_, _) = rhs.as_ref() {
            (lhs.as_ref(), rhs.as_ref())
        } else if let HirExpr::Const(_, _) = lhs.as_ref() {
            (rhs.as_ref(), lhs.as_ref())
        } else {
            return None;
        };

        let HirExpr::Const(y_val, _) = y_expr else {
            return None;
        };

        if let Some(bits) = int_type_bits(mul_ty) {
            let mask = if bits == 64 { u64::MAX } else { (1u64 << bits) - 1 };
            let y_128 = ((*y_val as u64) & mask) as u128;
            
            let mut x_val = x_ext;
            let mut is_sext = false;

            if let HirExpr::Cast { ty: cast_ty, expr: original_x } = x_ext {
                if let NirType::Int { bits: orig_bits, signed } = expr_type(original_x.as_ref()) {
                    x_size_bits = Some(orig_bits);
                    is_sext = signed;
                }
                x_val = original_x.as_ref();
            } else if let NirType::Int { bits: orig_bits, signed } = expr_type(x_ext) {
                x_size_bits = Some(orig_bits);
                is_sext = signed;
            }

            if let Some(x_bits) = x_size_bits {
                if n <= 127 && x_bits <= 64 && y_128 > 1 {
                    let y_minus_1 = y_128 - 1;
                    let power = 1u128 << n;
                    let mut q = power / y_minus_1;
                    let mut r = power % y_minus_1;
                    
                    if q <= u64::MAX as u128 && y_minus_1 >= q {
                        let mut diff = 0;
                        if r >= q {
                            q += 1;
                            r = r.wrapping_sub(y_minus_1).wrapping_add(q);
                            if r >= q {
                                // invalid magic
                            } else {
                                diff = q;
                            }
                        } else {
                            diff = 0;
                        }
                        
                        let mut maxx = if x_bits == 64 { 0 } else { 1u128 << x_bits };
                        maxx = maxx.wrapping_sub(1);
                        diff += q.saturating_sub(r);
                        
                        if diff != 0 {
                            let tmp = power / diff;
                            if tmp > maxx {
                                let divisor = q as u64;
                                let _out_bits = ext_ty_bits.unwrap_or(x_bits);
                                // Return the recovered division
                                let div = HirExpr::Binary {
                                    op: HirBinaryOp::Div,
                                    lhs: Box::new(x_val.clone()),
                                    rhs: Box::new(HirExpr::Const(divisor as i64, expr_type(x_val))),
                                    ty: expr_type(x_val),
                                };
                                return Some(if expr_type(expr) == expr_type(x_val) {
                                    div
                                } else {
                                    HirExpr::Cast {
                                        ty: expr_type(expr),
                                        expr: Box::new(div),
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn full_mask_for_type(ty: &NirType) -> Option<i64> {
    int_type_bits(ty).and_then(full_mask_for_bits)
}

fn is_full_mask_const(expr: &HirExpr, ty: &NirType) -> bool {
    let HirExpr::Const(value, _) = expr else {
        return false;
    };
    full_mask_for_type(ty).is_some_and(|mask| mask == *value)
}
