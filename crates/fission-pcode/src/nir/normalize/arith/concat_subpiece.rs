//! CONCAT/SUBPIECE/SHIFT algebra inspired by Ghidra RuleHumptyDumpty,
//! RuleDumptyHump, and RuleDumptyHumpLate.

use super::super::*;
use super::util::*;

/// `Or(Shl(Shr(x, n), n), And(x, low_mask))` where `low_mask == (1<<n)-1`
/// reassembles a SUBPIECE/CONCAT split back to `x` (optionally cast to `ty`).
pub(crate) fn recognize_humpty_dumpty_or(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    let (high, low) = match (lhs.as_ref(), rhs.as_ref()) {
        (high, low) if matches!(high, HirExpr::Binary { op: HirBinaryOp::Shl, .. }) => (high, low),
        (low, high) if matches!(high, HirExpr::Binary { op: HirBinaryOp::Shl, .. }) => (low, high),
        _ => return None,
    };

    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shifted,
        rhs: shift_amt,
        ..
    } = high
    else {
        return None;
    };
    let HirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 || *n >= 64 {
        return None;
    }
    let low_mask = full_mask_for_bits(*n as u32)?;
    let (src, mask_val) = match low {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let (x, m) = match (and_lhs.as_ref(), and_rhs.as_ref()) {
                (x, HirExpr::Const(m, _)) => (x, *m),
                (HirExpr::Const(m, _), x) => (x, *m),
                _ => return None,
            };
            if m != low_mask {
                return None;
            }
            (x, m)
        }
        _ => return None,
    };
    let _ = mask_val;

    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs: shr_lhs,
        rhs: shr_amt,
        ..
    } = shifted.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shr_n, _) = shr_amt.as_ref() else {
        return None;
    };
    if shr_n != n {
        return None;
    }
    if shr_lhs.as_ref() != src {
        return None;
    }

    let source_ty = expr_type(src);
    if source_ty == *ty {
        Some(src.clone())
    } else if matches!(source_ty, NirType::Unknown) || is_integer_type(&source_ty) {
        Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(src.clone()),
        })
    } else {
        None
    }
}

/// `Cast(narrow, Or(humpty_pattern))` → `Cast(narrow, x)` (DumptyHump).
pub(crate) fn recognize_dumpty_hump_cast(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    let recombined = recognize_humpty_dumpty_or(inner)?;
    Some(HirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(recombined),
    })
}

/// `Shl(And(x, mask), n)` where `mask << n == 0` → `Shl(Cast(w, x), n)` (DumptyHumpLate).
pub(crate) fn recognize_dumpty_hump_late(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs,
        rhs: shift_amt,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 || *n >= 64 {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::And,
        lhs: and_lhs,
        rhs: and_rhs,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let (src, mask) = match (and_lhs.as_ref(), and_rhs.as_ref()) {
        (x, HirExpr::Const(m, _)) => (x, *m as u64),
        (HirExpr::Const(m, _), x) => (x, *m as u64),
        _ => return None,
    };
    if (mask as u64) >= (1u64 << *n) {
        return None;
    }
    let cast_ty = ty.clone();
    Some(HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: Box::new(HirExpr::Cast {
            ty: cast_ty.clone(),
            expr: Box::new(src.clone()),
        }),
        rhs: shift_amt.clone(),
        ty: cast_ty,
    })
}

/// `Or(Shl(Cast(w, hi), n), Cast(w, lo))` with zero-extending casts → wide recombine (ConcatZext).
pub(crate) fn recognize_concat_zext_or(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    let (hi_side, lo_side) = match (lhs.as_ref(), rhs.as_ref()) {
        (HirExpr::Binary { op: HirBinaryOp::Shl, .. }, lo) => (lhs.as_ref(), lo),
        (lo, HirExpr::Binary { op: HirBinaryOp::Shl, .. }) => (rhs.as_ref(), lhs.as_ref()),
        _ => return None,
    };

    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: hi_cast_expr,
        rhs: shift_amt,
        ..
    } = hi_side
    else {
        return None;
    };
    let HirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 {
        return None;
    }

    let (hi_src, wide_ty) = peel_unsigned_zext_cast(hi_cast_expr)?;
    let (lo_src, lo_wide_ty) = peel_unsigned_zext_cast(lo_side)?;
    if wide_ty != lo_wide_ty {
        return None;
    }
    if hi_src != lo_src {
        return None;
    }
    let Some(total_bits) = int_type_bits(ty) else {
        return None;
    };
    let Some(wide_bits) = int_type_bits(&wide_ty) else {
        return None;
    };
    if wide_bits >= total_bits {
        return Some(hi_src.clone());
    }
    Some(HirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(hi_src.clone()),
    })
}

/// `Cast(W, Cast(N, x))` with `W > N` and matching signedness → single cast (Piece2Zext/Sext).
pub(crate) fn recognize_piece2_zext_sext(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: outer_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    let HirExpr::Cast {
        ty: inner_ty,
        expr: source,
    } = inner.as_ref()
    else {
        return None;
    };
    let (outer_bits, outer_signed) = scalar_cast_signature(outer_ty)?;
    let (inner_bits, inner_signed) = scalar_cast_signature(inner_ty)?;
    // Piece2Zext only — do not collapse signed extension chains (Piece2Sext is separate).
    if outer_signed || inner_signed || outer_bits <= inner_bits {
        return None;
    }
    if !source_is_scalarish(&expr_type(source)) {
        return None;
    }
    Some(HirExpr::Cast {
        ty: outer_ty.clone(),
        expr: source.clone(),
    })
}

fn peel_unsigned_zext_cast(expr: &HirExpr) -> Option<(HirExpr, NirType)> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    let NirType::Int {
        bits: cast_bits,
        signed: false,
    } = ty
    else {
        return None;
    };
    let inner_ty = expr_type(inner);
    let inner_bits = int_type_bits(&inner_ty).unwrap_or(0);
    if *cast_bits <= inner_bits {
        return None;
    }
    Some(((*inner).as_ref().clone(), ty.clone()))
}

fn scalar_cast_signature(ty: &NirType) -> Option<(u32, bool)> {
    match ty {
        NirType::Bool => Some((1, false)),
        NirType::Int { bits, signed } => Some((*bits, *signed)),
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

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    #[test]
    fn humpty_dumpty_reassembles_split_word() {
        let x = var("x");
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(x.clone()),
                    rhs: Box::new(HirExpr::Const(16, u32_ty())),
                    ty: u32_ty(),
                }),
                rhs: Box::new(HirExpr::Const(16, u32_ty())),
                ty: u32_ty(),
            }),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: Box::new(x.clone()),
                rhs: Box::new(HirExpr::Const(0xffff, u32_ty())),
                ty: u32_ty(),
            }),
            ty: u32_ty(),
        };

        let normalized = recognize_humpty_dumpty_or(&expr).expect("humpty");
        match normalized {
            HirExpr::Var(name) => assert_eq!(name, "x"),
            HirExpr::Cast { expr, .. } => {
                assert!(matches!(expr.as_ref(), HirExpr::Var(name) if name == "x"));
            }
            other => panic!("unexpected humpty result: {other:?}"),
        }
    }

    #[test]
    fn piece2_zext_collapses_nested_unsigned_casts() {
        let expr = HirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(HirExpr::Cast {
                ty: u32_ty(),
                expr: Box::new(var("x")),
            }),
        };
        let normalized = recognize_piece2_zext_sext(&expr).expect("piece2zext");
        assert_eq!(
            normalized,
            HirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(var("x")),
            }
        );
    }
}
