use crate::prelude::*;
use super::util::*;

pub fn canonicalize_integer_expr(expr: &DirExpr) -> Option<DirExpr> {
    canonicalize_cast_expr(expr)
}

pub fn recognize_hi_lo_extract(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Cast { ty, expr: inner } if is_integer_type(ty) => match inner.as_ref() {
            DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                let DirExpr::Const(mask, _) = rhs.as_ref() else {
                    return None;
                };
                let mask_limit = full_mask_for_type(ty)?;
                if *mask == mask_limit {
                    return Some(DirExpr::Cast {
                        ty: ty.clone(),
                        expr: lhs.clone(),
                    });
                }
                None
            }
            DirExpr::Binary {
                op: DirBinaryOp::Shr | DirBinaryOp::Sar,
                lhs,
                rhs,
                ..
            } => {
                let DirExpr::Const(shift, _) = rhs.as_ref() else {
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
                    Some(DirExpr::Cast {
                        ty: ty.clone(),
                        expr: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Shr,
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
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs,
            rhs,
            ty,
        } if is_integer_type(ty) => {
            let DirExpr::Const(mask, _) = rhs.as_ref() else {
                return None;
            };
            let mask_limit = full_mask_for_type(ty)?;
            if *mask != mask_limit {
                return None;
            }
            Some(DirExpr::Cast {
                ty: ty.clone(),
                expr: lhs.clone(),
            })
        }
        _ => None,
    }
}

pub fn recognize_wide_integer_recombine(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs: hi_expr,
        rhs: hi_shift,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let DirExpr::Const(shift_amount, _) = hi_shift.as_ref() else {
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
        Some(DirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(high.source),
        })
    } else {
        None
    }
}

#[derive(Clone)]
struct WidePart {
    source: DirExpr,
    width_bits: u32,
    shift_bits: i64,
}

fn extract_high_part(expr: &DirExpr, shift_amount: i64, total_bits: u32) -> Option<WidePart> {
    let DirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    // Peel an optional intermediate (widening) cast to reach the Shr directly.
    // Pattern: Cast(IntM, Cast(IntN, Shr(x, K))) where M >= N.
    // When peeling, use IntN's bits as the effective data width, not IntM's.
    let (shr_candidate, effective_ty): (&DirExpr, &NirType) = match inner.as_ref() {
        DirExpr::Cast {
            ty: mid_ty,
            expr: mid_inner,
        } => {
            let outer_bits = int_type_bits(ty).unwrap_or(0);
            let mid_bits = int_type_bits(mid_ty).unwrap_or(0);
            if outer_bits >= mid_bits && mid_bits > 0 {
                (mid_inner.as_ref(), mid_ty)
            } else {
                (inner.as_ref(), ty)
            }
        }
        _ => (inner.as_ref(), ty),
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Shr | DirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = shr_candidate
    else {
        return None;
    };
    let DirExpr::Const(inner_shift, _) = rhs.as_ref() else {
        return None;
    };
    if *inner_shift != shift_amount {
        return None;
    }
    let width_bits = int_type_bits(effective_ty)?;
    if shift_amount != i64::from(total_bits.saturating_sub(width_bits)) {
        return None;
    }
    Some(WidePart {
        source: (**lhs).clone(),
        width_bits,
        shift_bits: shift_amount,
    })
}

fn extract_low_part(expr: &DirExpr, shift_amount: i64) -> Option<WidePart> {
    match expr {
        DirExpr::Cast { ty, expr: inner } => {
            // For a double cast Cast(IntM, Cast(IntN, x)) use the INNER (narrower)
            // width as the true data width, since the outer cast is just for the
            // Piece output type.
            let (width_bits, real_inner) = match inner.as_ref() {
                DirExpr::Cast {
                    ty: inner_ty,
                    expr: inner_inner,
                } => {
                    let outer_bits = int_type_bits(ty).unwrap_or(0);
                    let mid_bits = int_type_bits(inner_ty).unwrap_or(0);
                    if outer_bits >= mid_bits && mid_bits > 0 {
                        (mid_bits, inner_inner.as_ref())
                    } else {
                        (int_type_bits(ty)?, inner.as_ref())
                    }
                }
                _ => (int_type_bits(ty)?, inner.as_ref()),
            };
            Some(WidePart {
                source: real_inner.clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            let DirExpr::Const(mask, _) = rhs.as_ref() else {
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
        DirExpr::Binary {
            op: DirBinaryOp::Mod,
            lhs,
            rhs,
            ..
        } => {
            let DirExpr::Const(modulus, _) = rhs.as_ref() else {
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

fn canonicalize_cast_expr(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };

    if should_preserve_non_scalar_cast(ty) {
        if let DirExpr::Cast {
            ty: inner_ty,
            expr: inner_inner,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                return Some(DirExpr::Cast {
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

    let DirExpr::Cast {
        ty: inner_cast_ty,
        expr: inner_inner,
    } = inner.as_ref()
    else {
        return None;
    };

    if inner_cast_ty == ty {
        return Some(DirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    if should_drop_inner_scalar_cast(ty, inner_cast_ty, &expr_type(inner_inner)) {
        return Some(DirExpr::Cast {
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
