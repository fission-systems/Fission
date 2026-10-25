use super::util::*;
use crate::cleanup::expr_has_side_effects;
use crate::prelude::*;

pub fn collapse_zero_offset_cast(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            let PreHirExpr::PtrOffset { base, offset } = ptr.as_ref() else {
                return None;
            };
            if *offset != 0 {
                return None;
            }
            Some(PreHirExpr::Load {
                ptr: base.clone(),
                ty: ty.clone(),
            })
        }
        PreHirExpr::PtrOffset { base, offset } if *offset == 0 => Some((**base).clone()),
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } if matches!(index.as_ref(), PreHirExpr::Const(0, _))
            && !matches!(base.as_ref(), PreHirExpr::Var(_)) =>
        {
            Some(PreHirExpr::Load {
                ptr: base.clone(),
                ty: elem_ty.clone(),
            })
        }
        _ => None,
    }
}

pub fn cleanup_arithmetic_wrappers(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(lhs.as_ref()) => Some((**rhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Div,
            lhs,
            rhs,
            ..
        } if is_one_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mod,
            lhs,
            rhs,
            ..
        } if is_one_const(rhs.as_ref()) && !expr_has_side_effects(lhs) => {
            Some(PreHirExpr::Const(0, expr_type(lhs)))
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) && !expr_has_side_effects(lhs) => {
            Some((**lhs).clone())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) && !expr_has_side_effects(lhs) => {
            Some((**lhs).clone())
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if lhs == rhs && source_is_scalarish(&expr_type(lhs)) && !expr_has_side_effects(lhs) => {
            Some(PreHirExpr::Const(0, expr_type(lhs)))
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) => Some((**lhs).clone()),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(lhs.as_ref(), &expr_type(rhs)) => Some((**rhs).clone()),
        // Sar(Cast(signed_T, x), k) → the cast already makes the value signed;
        // if the output type of the Sar equals the cast type, the cast is
        // redundant — drop it.  This prevents the printer from emitting a
        // double-signed-cast.
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ty,
        } => match lhs.as_ref() {
            PreHirExpr::Cast {
                ty: cast_ty,
                expr: inner,
            } if matches!(cast_ty, NirType::Int { signed: true, .. }) && cast_ty == ty => {
                Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sar,
                    lhs: inner.clone(),
                    rhs: rhs.clone(),
                    ty: ty.clone(),
                })
            }
            _ => None,
        },
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => match lhs.as_ref() {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
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
pub fn simplify_subpiece_chain(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Cast {
        ty: outer_ty,
        expr: shr_expr,
    } = expr
    else {
        return None;
    };
    if !is_integer_type(outer_ty) {
        return None;
    }
    let PreHirExpr::Binary {
        op: shift_op @ (PreHirBinaryOp::Shr | PreHirBinaryOp::Sar),
        lhs: inner_cast_expr,
        rhs: shift_const,
        ..
    } = shr_expr.as_ref()
    else {
        return None;
    };
    let PreHirExpr::Const(shift_amount, _) = shift_const.as_ref() else {
        return None;
    };
    let PreHirExpr::Cast {
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
    let is_signed = |ty: &NirType| matches!(ty, NirType::Int { signed: true, .. });

    // Rule 2: shift expels all real data → result is 0.
    // Only valid for logical shift on unsigned types; Sar or signed values preserve sign bits.
    if *shift_op == PreHirBinaryOp::Shr
        && !is_signed(mid_ty)
        && !is_signed(&source_ty)
        && *shift_amount >= i64::from(source_bits)
        && mid_bits >= source_bits
    {
        return Some(PreHirExpr::Const(0, outer_ty.clone()));
    }

    // Rule 1: inner cast is a non-narrowing zero-extension — it doesn't
    // change any bits that will end up in the final result.  Remove it.
    if !is_signed(mid_ty) && !is_signed(&source_ty) && mid_bits >= source_bits {
        let outer_bits = int_type_bits(outer_ty).unwrap_or(0);
        // Preserve the shift result type as the wider of the two integer sizes.
        let shr_result_ty = if mid_bits >= outer_bits {
            mid_ty.clone()
        } else {
            outer_ty.clone()
        };
        return Some(PreHirExpr::Cast {
            ty: outer_ty.clone(),
            expr: Box::new(PreHirExpr::Binary {
                op: *shift_op,
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
pub fn merge_consecutive_shifts(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr,
        lhs,
        rhs: rhs2,
        ty,
    } = expr
    else {
        return None;
    };
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr,
        lhs: x,
        rhs: rhs1,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let PreHirExpr::Const(k1, _) = rhs1.as_ref() else {
        return None;
    };
    let PreHirExpr::Const(k2, _) = rhs2.as_ref() else {
        return None;
    };
    if *k1 < 0 || *k2 < 0 {
        return None;
    }
    let total = k1.checked_add(*k2)?;
    // Guard against degenerate total shifts ≥ 64 bits.
    if total >= 64 {
        return Some(PreHirExpr::Const(0, ty.clone()));
    }
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr,
        lhs: x.clone(),
        rhs: Box::new(PreHirExpr::Const(
            total,
            rhs1.as_ref().clone().into_const_type(),
        )),
        ty: ty.clone(),
    })
}

pub trait IntoConstType {
    fn into_const_type(self) -> NirType;
}
impl IntoConstType for PreHirExpr {
    fn into_const_type(self) -> NirType {
        match self {
            PreHirExpr::Const(_, ty) => ty,
            _ => NirType::Int {
                bits: 64,
                signed: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    fn i64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: true,
        }
    }

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    #[test]
    fn simplify_subpiece_chain_does_not_fold_sar_to_zero() {
        // Cast(i32, Sar(Cast(i64, x_i32), 32))
        let x_i32 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("x".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, i32_ty())),
            ty: i32_ty(),
        };
        let expr = PreHirExpr::Cast {
            ty: i32_ty(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Sar,
                lhs: Box::new(PreHirExpr::Cast {
                    ty: i64_ty(),
                    expr: Box::new(x_i32),
                }),
                rhs: Box::new(PreHirExpr::Const(32, u32_ty())),
                ty: i64_ty(),
            }),
        };
        // Sar on sign-extended i32 by 32 bits should NOT fold to Const(0, i32)
        assert_ne!(
            simplify_subpiece_chain(&expr),
            Some(PreHirExpr::Const(0, i32_ty()))
        );
    }

    #[test]
    fn simplify_subpiece_chain_folds_unsigned_shr_to_zero() {
        // Cast(u32, Shr(Cast(u64, x_u32), 32))
        let x_u32 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("x".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, u32_ty())),
            ty: u32_ty(),
        };
        let expr = PreHirExpr::Cast {
            ty: u32_ty(),
            expr: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Shr,
                lhs: Box::new(PreHirExpr::Cast {
                    ty: u64_ty(),
                    expr: Box::new(x_u32),
                }),
                rhs: Box::new(PreHirExpr::Const(32, u32_ty())),
                ty: u64_ty(),
            }),
        };
        assert_eq!(
            simplify_subpiece_chain(&expr),
            Some(PreHirExpr::Const(0, u32_ty()))
        );
    }
}
