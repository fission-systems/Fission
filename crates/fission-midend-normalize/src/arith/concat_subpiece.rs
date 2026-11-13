//! CONCAT/SUBPIECE/SHIFT algebra inspired by Ghidra RuleHumptyDumpty,
//! RuleDumptyHump, and RuleDumptyHumpLate.

use super::util::*;
use crate::prelude::*;

/// `Or(Shl(Shr(x, n), n), And(x, low_mask))` where `low_mask == (1<<n)-1`
/// reassembles a SUBPIECE/CONCAT split back to `x` (optionally cast to `ty`).
pub fn recognize_humpty_dumpty_or(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    let (high, low) = match (lhs.as_ref(), rhs.as_ref()) {
        (high, low)
            if matches!(
                high,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shl,
                    ..
                }
            ) =>
        {
            (high, low)
        }
        (low, high)
            if matches!(
                high,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shl,
                    ..
                }
            ) =>
        {
            (low, high)
        }
        _ => return None,
    };

    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs: shifted,
        rhs: shift_amt,
        ..
    } = high
    else {
        return None;
    };
    let PreHirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 || *n >= 64 {
        return None;
    }
    let low_mask = full_mask_for_bits(*n as u32)?;
    let (src, mask_val) = match low {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let (x, m) = match (and_lhs.as_ref(), and_rhs.as_ref()) {
                (x, PreHirExpr::Const(m, _)) => (x, *m),
                (PreHirExpr::Const(m, _), x) => (x, *m),
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

    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
        lhs: shr_lhs,
        rhs: shr_amt,
        ..
    } = shifted.as_ref()
    else {
        return None;
    };
    let PreHirExpr::Const(shr_n, _) = shr_amt.as_ref() else {
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
        Some(PreHirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(src.clone()),
        })
    } else {
        None
    }
}

/// `Cast(narrow, Or(humpty_pattern))` → `Cast(narrow, x)` (DumptyHump).
pub fn recognize_dumpty_hump_cast(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    let recombined = recognize_humpty_dumpty_or(inner)?;
    Some(PreHirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(recombined),
    })
}

/// `Shl(And(x, mask), n)` → `Shl(Cast(w, x), n)` when the mask is redundant
/// (DumptyHumpLate).
///
/// At width `w` a left shift by `n` keeps only the low `w - n` bits of its
/// operand, so the mask says nothing exactly when it keeps all of them. The
/// mask is first narrowed to the And's own width, since bits the And cannot
/// hold are cleared whatever the constant says. Any other mask selects a
/// field, as in a byte swap's `(x & 0xff0000) << 24`, and must stay.
pub fn recognize_dumpty_hump_late(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs,
        rhs: shift_amt,
        ty,
    } = expr
    else {
        return None;
    };
    let PreHirExpr::Const(n, _) = shift_amt.as_ref() else {
        return None;
    };
    if *n <= 0 || *n >= 64 {
        return None;
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::And,
        lhs: and_lhs,
        rhs: and_rhs,
        ty: and_ty,
    } = lhs.as_ref()
    else {
        return None;
    };
    let (src, mask) = match (and_lhs.as_ref(), and_rhs.as_ref()) {
        (x, PreHirExpr::Const(m, _)) => (x, *m as u64),
        (PreHirExpr::Const(m, _), x) => (x, *m as u64),
        _ => return None,
    };
    let width = int_type_bits(ty)?;
    let shift = u32::try_from(*n).ok()?;
    if width > 64 || shift >= width {
        return None;
    }
    let surviving = low_bits_u64(width - shift);
    let mask = match int_type_bits(and_ty) {
        Some(and_bits) if and_bits < 64 => mask & low_bits_u64(and_bits),
        Some(_) => mask,
        // An And of unknown width may clear bits the constant keeps.
        None => return None,
    };
    if mask & surviving != surviving {
        return None;
    }
    let cast_ty = ty.clone();
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs: Box::new(PreHirExpr::Cast {
            ty: cast_ty.clone(),
            expr: Box::new(src.clone()),
        }),
        rhs: shift_amt.clone(),
        ty: cast_ty,
    })
}

/// `Or(Shl(Cast(w, hi), n), Cast(w, lo))` with zero-extending casts → wide recombine (ConcatZext).
pub fn recognize_concat_zext_or(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    let (hi_side, lo_side) = match (lhs.as_ref(), rhs.as_ref()) {
        (
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                ..
            },
            lo,
        ) => (lhs.as_ref(), lo),
        (
            lo,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                ..
            },
        ) => (rhs.as_ref(), lhs.as_ref()),
        _ => return None,
    };

    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs: hi_cast_expr,
        rhs: shift_amt,
        ..
    } = hi_side
    else {
        return None;
    };
    let PreHirExpr::Const(n, _) = shift_amt.as_ref() else {
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
    let (hi_source, hi_shift_amt) = match &hi_src {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs,
            rhs,
            ..
        } => {
            let PreHirExpr::Const(s, _) = rhs.as_ref() else {
                return None;
            };
            (lhs.as_ref(), *s)
        }
        _ => return None,
    };
    if hi_shift_amt != *n || hi_source != &lo_src {
        return None;
    }
    let Some(total_bits) = int_type_bits(ty) else {
        return None;
    };
    let Some(wide_bits) = int_type_bits(&wide_ty) else {
        return None;
    };
    if wide_bits >= total_bits {
        return Some(lo_src);
    }
    Some(PreHirExpr::Cast {
        ty: ty.clone(),
        expr: Box::new(lo_src),
    })
}

/// `Cast(W, Cast(N, x))` with `W > N` and matching signedness → single cast (Piece2Zext/Sext).
pub fn recognize_piece2_zext_sext(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Cast {
        ty: outer_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    let PreHirExpr::Cast {
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
    if let Some((source_bits, source_signed)) = scalar_cast_signature(&expr_type(source)) {
        if source_signed || inner_bits < source_bits {
            return None;
        }
    }
    if !source_is_scalarish(&expr_type(source)) {
        return None;
    }
    Some(PreHirExpr::Cast {
        ty: outer_ty.clone(),
        expr: source.clone(),
    })
}

fn peel_unsigned_zext_cast(expr: &PreHirExpr) -> Option<(PreHirExpr, NirType)> {
    let PreHirExpr::Cast { ty, expr: inner } = expr else {
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

/// The low `bits` bits set, for `1..=64`.
fn low_bits_u64(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
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
    // prelude via parent

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

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    #[test]
    fn humpty_dumpty_reassembles_split_word() {
        let x = var("x");
        let expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                lhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Shr,
                    lhs: Box::new(x.clone()),
                    rhs: Box::new(PreHirExpr::Const(16, u32_ty())),
                    ty: u32_ty(),
                }),
                rhs: Box::new(PreHirExpr::Const(16, u32_ty())),
                ty: u32_ty(),
            }),
            rhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(x.clone()),
                rhs: Box::new(PreHirExpr::Const(0xffff, u32_ty())),
                ty: u32_ty(),
            }),
            ty: u32_ty(),
        };

        let normalized = recognize_humpty_dumpty_or(&expr).expect("humpty");
        match normalized {
            PreHirExpr::Var(name) => assert_eq!(name, "x"),
            PreHirExpr::Cast { expr, .. } => {
                assert!(matches!(expr.as_ref(), PreHirExpr::Var(name) if name == "x"));
            }
            other => panic!("unexpected humpty result: {other:?}"),
        }
    }

    fn u8_ty() -> NirType {
        NirType::Int {
            bits: 8,
            signed: false,
        }
    }

    /// `(x & mask) << n`, the And typed `and_ty` and the shift typed `ty`.
    fn masked_shl(mask: i64, n: i64, and_ty: NirType, ty: NirType) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::And,
                lhs: Box::new(var("x")),
                rhs: Box::new(PreHirExpr::Const(mask, and_ty.clone())),
                ty: and_ty,
            }),
            rhs: Box::new(PreHirExpr::Const(n, ty.clone())),
            ty,
        }
    }

    #[test]
    fn dumpty_hump_late_drops_a_mask_the_shift_makes_redundant() {
        // (x & 0xff) << 56 at 64 bits: only x's low 8 bits survive the shift,
        // and the mask keeps exactly those, so it says nothing.
        let normalized = recognize_dumpty_hump_late(&masked_shl(0xff, 56, u64_ty(), u64_ty()))
            .expect("mask covers every surviving bit");
        assert_eq!(
            normalized,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                lhs: Box::new(PreHirExpr::Cast {
                    ty: u64_ty(),
                    expr: Box::new(var("x")),
                }),
                rhs: Box::new(PreHirExpr::Const(56, u64_ty())),
                ty: u64_ty(),
            }
        );
        // A wider mask than needed is just as redundant.
        assert!(recognize_dumpty_hump_late(&masked_shl(0xffff, 16, u32_ty(), u32_ty())).is_some());
    }

    #[test]
    fn dumpty_hump_late_keeps_a_mask_that_clears_surviving_bits() {
        // One half of a 64-bit byte swap: (x & 0xff0000) << 24 moves byte 2
        // to byte 5 and nothing else. x << 24 would carry bytes 0..4 along.
        // The old guard (`mask < 1 << n`) accepted this.
        assert!(
            recognize_dumpty_hump_late(&masked_shl(0xff_0000, 24, u64_ty(), u64_ty())).is_none()
        );
        assert!(recognize_dumpty_hump_late(&masked_shl(0xff00, 40, u64_ty(), u64_ty())).is_none());
    }

    #[test]
    fn dumpty_hump_late_counts_only_the_mask_bits_the_and_keeps() {
        // 0xffffffff covers the 24 bits a shift by 8 keeps at 32 bits, but the
        // And is 8 bits wide, so it keeps only x's low byte.
        assert!(
            recognize_dumpty_hump_late(&masked_shl(0xffff_ffff, 8, u8_ty(), u32_ty())).is_none()
        );
        // Positive control: the same 8-bit And is redundant once the shift
        // keeps no more than 8 bits.
        assert!(
            recognize_dumpty_hump_late(&masked_shl(0xffff_ffff, 24, u8_ty(), u32_ty())).is_some()
        );
    }

    #[test]
    fn piece2_zext_collapses_nested_unsigned_casts() {
        let expr = PreHirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(PreHirExpr::Cast {
                ty: u32_ty(),
                expr: Box::new(var("x")),
            }),
        };
        let normalized = recognize_piece2_zext_sext(&expr).expect("piece2zext");
        assert_eq!(
            normalized,
            PreHirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(var("x")),
            }
        );
    }

    #[test]
    fn piece2_zext_does_not_drop_narrowing_truncation() {
        let x_u64 = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(var("x")),
            rhs: Box::new(PreHirExpr::Const(1, u64_ty())),
            ty: u64_ty(),
        };
        // Cast(u64, Cast(u32, x_u64)) should NOT collapse to Cast(u64, x_u64)
        let expr = PreHirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(PreHirExpr::Cast {
                ty: u32_ty(),
                expr: Box::new(x_u64),
            }),
        };
        assert!(recognize_piece2_zext_sext(&expr).is_none());
    }

    #[test]
    fn concat_zext_or_rejects_duplicate_lower_half() {
        // (Cast(u64, x) << 32) | Cast(u64, x)
        let x = var("x");
        let cast_x = PreHirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(x),
        };
        let expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Or,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Shl,
                lhs: Box::new(cast_x.clone()),
                rhs: Box::new(PreHirExpr::Const(32, u32_ty())),
                ty: u64_ty(),
            }),
            rhs: Box::new(cast_x),
            ty: u64_ty(),
        };
        assert!(recognize_concat_zext_or(&expr).is_none());
    }
}
