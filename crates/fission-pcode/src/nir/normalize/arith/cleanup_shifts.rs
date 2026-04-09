use super::super::*;
use super::util::*;


pub(crate) fn collapse_zero_offset_cast(expr: &HirExpr) -> Option<HirExpr> {
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

pub(crate) fn cleanup_arithmetic_wrappers(expr: &HirExpr) -> Option<HirExpr> {
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
        // Sar(Cast(signed_T, x), k) → the cast already makes the value signed;
        // if the output type of the Sar equals the cast type, the cast is
        // redundant — drop it.  This prevents the printer from emitting a
        // double-signed-cast.
        HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs,
            rhs,
            ty,
        } => match lhs.as_ref() {
            HirExpr::Cast {
                ty: cast_ty,
                expr: inner,
            } if matches!(cast_ty, NirType::Int { signed: true, .. }) && cast_ty == ty => {
                Some(HirExpr::Binary {
                    op: HirBinaryOp::Sar,
                    lhs: inner.clone(),
                    rhs: rhs.clone(),
                    ty: ty.clone(),
                })
            }
            _ => None,
        },
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


// ── SubPiece / Cast chain simplifications ─────────────────────────────────────

/// Simplify `Cast(IntN, Shr(Cast(IntM, x), K))` where the inner cast is a
/// widening zero-extension of `x` (M >= bit_width(x)), so the Shr operates on
/// the same bits regardless of whether the cast is present.
///
/// Two sub-rules:
/// 1. **Cast removal**: when both outer/inner casts are unsigned and the inner
///    one is a pure widening, collapse it:
///    `Cast(IntN, Shr(Cast(IntM, x), K))` → `Cast(IntN, Shr(x, K))`
///    (only when `x_bits <= M` and `M >= x_bits`, i.e. inner cast is non-narrowing)
///
/// 2. **Zero extraction**: when the shift amount K is ≥ bit_width(x), the shift
///    completely expels all data bits through zero-extension, yielding zero:
///    `Cast(IntN, Shr(Cast(IntM, x), K))` where K >= x_bits → `Const(0, IntN)`
pub(crate) fn simplify_subpiece_chain(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: outer_ty,
        expr: shr_expr,
    } = expr
    else {
        return None;
    };
    if !is_integer_type(outer_ty) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs: inner_cast_expr,
        rhs: shift_const,
        ..
    } = shr_expr.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = shift_const.as_ref() else {
        return None;
    };
    let HirExpr::Cast {
        ty: mid_ty,
        expr: source_expr,
    } = inner_cast_expr.as_ref()
    else {
        return None;
    };
    let source_ty = expr_type(source_expr);
    let Some(source_bits) = int_type_bits(&source_ty) else {
        return None;
    };
    let Some(mid_bits) = int_type_bits(mid_ty) else {
        return None;
    };

    // Rule 2: shift expels all real data → result is 0.
    if *shift_amount >= i64::from(source_bits) && mid_bits >= source_bits {
        return Some(HirExpr::Const(0, outer_ty.clone()));
    }

    // Rule 1: inner cast is a non-narrowing zero-extension — it doesn't
    // change any bits that will end up in the final result.  Remove it.
    if mid_bits >= source_bits {
        let outer_bits = int_type_bits(outer_ty).unwrap_or(0);
        // Preserve the shift result type as the wider of the two integer sizes.
        let shr_result_ty = if mid_bits >= outer_bits { mid_ty.clone() } else { outer_ty.clone() };
        return Some(HirExpr::Cast {
            ty: outer_ty.clone(),
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: source_expr.clone(),
                rhs: shift_const.clone(),
                ty: shr_result_ty,
            }),
        });
    }

    None
}

/// Merge two consecutive right-shifts into one:
/// `Shr(Shr(x, K1), K2)` → `Shr(x, K1+K2)` (unsigned)
///
/// This is valid for UNSIGNED (`Shr`) shifts with non-negative amounts.  We
/// conservatively reject `Sar` (arithmetic shift) to avoid changing sign-extension
/// semantics for the outer shift.
pub(crate) fn merge_consecutive_shifts(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs,
        rhs: rhs2,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs: x,
        rhs: rhs1,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(k1, _) = rhs1.as_ref() else {
        return None;
    };
    let HirExpr::Const(k2, _) = rhs2.as_ref() else {
        return None;
    };
    if *k1 < 0 || *k2 < 0 {
        return None;
    }
    let total = k1.checked_add(*k2)?;
    // Guard against degenerate total shifts ≥ 64 bits.
    if total >= 64 {
        return Some(HirExpr::Const(0, ty.clone()));
    }
    Some(HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs: x.clone(),
        rhs: Box::new(HirExpr::Const(total, rhs1.as_ref().clone().into_const_type())),
        ty: ty.clone(),
    })
}

pub(crate) trait IntoConstType {
    fn into_const_type(self) -> NirType;
}
impl IntoConstType for HirExpr {
    fn into_const_type(self) -> NirType {
        match self {
            HirExpr::Const(_, ty) => ty,
            _ => NirType::Int { bits: 64, signed: false },
        }
    }
}
