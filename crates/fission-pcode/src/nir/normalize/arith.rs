use super::*;

pub(super) fn canonicalize_integer_expr(expr: &HirExpr) -> Option<HirExpr> {
    canonicalize_cast_expr(expr)
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
    if high.source != low.source || high.width_bits != low.width_bits || high.shift_bits != low.shift_bits {
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
    matches!(ty, NirType::Ptr(_) | NirType::Aggregate { .. } | NirType::Float { .. })
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
        _ => None,
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
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_))
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
            && !matches!(base.as_ref(), HirExpr::Var(_)) => Some(HirExpr::Load {
            ptr: base.clone(),
            ty: elem_ty.clone(),
        }),
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

fn is_integer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
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

fn full_mask_for_type(ty: &NirType) -> Option<i64> {
    int_type_bits(ty).and_then(full_mask_for_bits)
}

fn is_full_mask_const(expr: &HirExpr, ty: &NirType) -> bool {
    let HirExpr::Const(value, _) = expr else {
        return false;
    };
    full_mask_for_type(ty).is_some_and(|mask| mask == *value)
}
