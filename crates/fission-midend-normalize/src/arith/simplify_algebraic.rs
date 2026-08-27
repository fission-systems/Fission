//! Algebraic simplifications for HIR expressions.
//!
//! - `x + (-c)`  -> `x - c`
//! - `x - (-c)`  -> `x + c`
//! - `x + x`     -> `x * 2`

use crate::prelude::*;

/// The constant's value read at the width its type declares, when that
/// reading is negative.
///
/// A lifter stores `add esp, -168` as the four-byte pattern `0xffffff58`, and
/// `lower_varnode_inner` carries that through as the `i64` 4,294,967,128 with
/// a 32-bit unsigned type. `simplify_negated_const` turns `x + (-c)` into
/// `x - c`, but it tested `c < 0` and so never fired for those: the emitted
/// body printed `esp + 4294967128` where `esp - 168` belongs. Measured on
/// `dexter.dll`'s `GetOSVersion` among others.
///
/// Only a value inside the unsigned range of its own width qualifies, so a
/// constant already stored as a negative `i64`, or one wider than its type
/// claims, is left to the existing `c < 0` path.
fn negative_at_declared_width(value: i64, ty: &NirType) -> Option<i64> {
    let NirType::Int { bits, .. } = ty else {
        return None;
    };
    let bits = *bits;
    if !(2..64).contains(&bits) || value <= 0 {
        return None;
    }
    let span = 1_i128 << bits;
    let value = i128::from(value);
    // Strictly above the midpoint. At exactly `span / 2` -- the most negative
    // value of the width -- `span - value` is the same value again, so the
    // `Add`/`Sub` rewrite flips forever inside the normalize fixpoint. The
    // existing `c != i64::MIN` guard on the already-signed path excludes the
    // same number for the same reason; leaving it out here hung
    // `dexter.dll`'s `DetectShutdown@16`.
    if value >= span || value <= span / 2 {
        return None;
    }
    i64::try_from(span - value).ok()
}

/// The magnitude to subtract for `x + c`, whether `c` is stored negative or
/// is negative only at its declared width.
fn subtractable_magnitude(value: i64, ty: &NirType) -> Option<i64> {
    if value < 0 && value != i64::MIN {
        return Some(-value);
    }
    negative_at_declared_width(value, ty)
}

pub fn simplify_negated_const(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ty,
        } => {
            if let PreHirExpr::Const(c, cty) = rhs.as_ref()
                && let Some(magnitude) = subtractable_magnitude(*c, cty)
            {
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: lhs.clone(),
                    rhs: Box::new(PreHirExpr::Const(magnitude, cty.clone())),
                    ty: ty.clone(),
                });
            }
            if let PreHirExpr::Const(c, cty) = lhs.as_ref()
                && let Some(magnitude) = subtractable_magnitude(*c, cty)
            {
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: rhs.clone(),
                    rhs: Box::new(PreHirExpr::Const(magnitude, cty.clone())),
                    ty: ty.clone(),
                });
            }
            None
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ty,
        } => {
            if let PreHirExpr::Const(c, cty) = rhs.as_ref()
                && let Some(magnitude) = subtractable_magnitude(*c, cty)
            {
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: lhs.clone(),
                    rhs: Box::new(PreHirExpr::Const(magnitude, cty.clone())),
                    ty: ty.clone(),
                });
            }
            None
        }
        _ => None,
    }
}

/// Recognize `x + x` → `x << 1`.
///
/// Compilers often emit `ADD reg, reg` instead of `SHL reg, 1`; prefer the
/// bitwise form to match Ghidra's RuleShl canonicalization and to avoid
/// subsequent arithmetic-normalization passes lifting this back to `* 2`.
pub fn simplify_double_add(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ty,
        } if lhs == rhs => Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: lhs.clone(),
            rhs: Box::new(PreHirExpr::Const(2, ty.clone())),
            ty: ty.clone(),
        }),
        _ => None,
    }
}

/// Factor common multiplicand: `x + x*c` -> `x*(c+1)` and `x*c + x` -> `x*(c+1)`.
pub fn simplify_factor_common_mul(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    // Helper: if `term` matches `common * const`, return (common, const)
    let match_mul_const =
        |term: &PreHirExpr, common: &PreHirExpr| -> Option<(Box<PreHirExpr>, i64, NirType)> {
            if let PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: mul_lhs,
                rhs: mul_rhs,
                ..
            } = term
            {
                if mul_lhs.as_ref() == common {
                    if let PreHirExpr::Const(c, cty) = mul_rhs.as_ref() {
                        return Some((mul_lhs.clone(), *c, cty.clone()));
                    }
                }
                if mul_rhs.as_ref() == common {
                    if let PreHirExpr::Const(c, cty) = mul_lhs.as_ref() {
                        return Some((mul_rhs.clone(), *c, cty.clone()));
                    }
                }
            }
            None
        };

    // rhs is lhs * c
    if let Some((common, c, cty)) = match_mul_const(rhs, lhs) {
        if c != i64::MAX {
            return Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: common,
                rhs: Box::new(PreHirExpr::Const(c + 1, cty)),
                ty: ty.clone(),
            });
        }
    }

    // lhs is rhs * c
    if let Some((common, c, cty)) = match_mul_const(lhs, rhs) {
        if c != i64::MAX {
            return Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: common,
                rhs: Box::new(PreHirExpr::Const(c + 1, cty)),
                ty: ty.clone(),
            });
        }
    }

    None
}

/// Simplify nested additions and subtractions with constants:
/// - (a + C1) + C2 -> a + (C1 + C2)
/// - (a - C1) + C2 -> a + (C2 - C1)
/// - (a + C1) - C2 -> a + (C1 - C2)
/// - (a - C1) - C2 -> a - (C1 + C2)
pub fn simplify_nested_adds_subs(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        // (a + C1) + C2 or (a - C1) + C2
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs: const2_expr,
            ty,
        } => {
            let PreHirExpr::Const(c2, cty2) = const2_expr.as_ref() else {
                return None;
            };
            match lhs.as_ref() {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let PreHirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_add(*c2)?;
                        return Some(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(PreHirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let PreHirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c2.checked_sub(*c1)?;
                        return Some(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(PreHirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        // (a + C1) - C2 or (a - C1) - C2
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs: const2_expr,
            ty,
        } => {
            let PreHirExpr::Const(c2, cty2) = const2_expr.as_ref() else {
                return None;
            };
            match lhs.as_ref() {
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let PreHirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_sub(*c2)?;
                        return Some(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(PreHirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let PreHirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_add(*c2)?;
                        return Some(PreHirExpr::Binary {
                            op: PreHirBinaryOp::Sub,
                            lhs: a.clone(),
                            rhs: Box::new(PreHirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    None
}

/// Simplify collections of multiplication terms:
/// - (a * C1) + (a * C2) -> a * (C1 + C2)
/// - (a * C1) - (a * C2) -> a * (C1 - C2)
/// - (a * C1) + a -> a * (C1 + 1)
/// - (a * C1) - a -> a * (C1 - 1)
pub fn simplify_collect_mul_terms(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: op @ (PreHirBinaryOp::Add | PreHirBinaryOp::Sub),
            lhs,
            rhs,
            ty,
        } => {
            let extract_factor = |term: &PreHirExpr| -> Option<(PreHirExpr, i64, NirType)> {
                match term {
                    PreHirExpr::Binary {
                        op: PreHirBinaryOp::Mul,
                        lhs: mul_lhs,
                        rhs: mul_rhs,
                        ..
                    } => {
                        if let PreHirExpr::Const(c, cty) = mul_rhs.as_ref() {
                            return Some((mul_lhs.as_ref().clone(), *c, cty.clone()));
                        }
                        if let PreHirExpr::Const(c, cty) = mul_lhs.as_ref() {
                            return Some((mul_rhs.as_ref().clone(), *c, cty.clone()));
                        }
                    }
                    _ => {
                        return Some((term.clone(), 1, ty.clone()));
                    }
                }
                None
            };

            let (factor_lhs, c_lhs, cty_lhs) = extract_factor(lhs)?;
            let (factor_rhs, c_rhs, _) = extract_factor(rhs)?;

            if factor_lhs == factor_rhs {
                if let PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    ..
                } = lhs.as_ref()
                {
                } else if let PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    ..
                } = rhs.as_ref()
                {
                } else {
                    return None;
                }

                let new_c = match op {
                    PreHirBinaryOp::Add => c_lhs.checked_add(c_rhs)?,
                    PreHirBinaryOp::Sub => c_lhs.checked_sub(c_rhs)?,
                    _ => unreachable!(),
                };

                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(factor_lhs),
                    rhs: Box::new(PreHirExpr::Const(new_c, cty_lhs)),
                    ty: ty.clone(),
                });
            }
        }
        _ => {}
    }
    None
}

/// Distribute a shared multiplicand: `a*b + a*c` → `a*(b+c)`.
pub fn simplify_distribute_common_factor(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    let (factor_lhs, c_lhs, cty) = extract_mul_factor(lhs)?;
    let (factor_rhs, c_rhs, _) = extract_mul_factor(rhs)?;
    if factor_lhs != factor_rhs {
        return None;
    }
    let new_c = c_lhs.checked_add(c_rhs)?;
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Mul,
        lhs: Box::new(factor_lhs),
        rhs: Box::new(PreHirExpr::Const(new_c, cty)),
        ty: ty.clone(),
    })
}

fn extract_mul_factor(term: &PreHirExpr) -> Option<(PreHirExpr, i64, NirType)> {
    match term {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(c, cty) = rhs.as_ref() {
                Some((lhs.as_ref().clone(), *c, cty.clone()))
            } else if let PreHirExpr::Const(c, cty) = lhs.as_ref() {
                Some((rhs.as_ref().clone(), *c, cty.clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Canonicalize commutative add operand order for stable output (RuleTermOrder).
pub fn simplify_term_order_add(expr: &PreHirExpr) -> Option<PreHirExpr> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    if term_order_key(lhs) <= term_order_key(rhs) {
        return None;
    }
    Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: rhs.clone(),
        rhs: lhs.clone(),
        ty: ty.clone(),
    })
}

fn term_order_key(expr: &PreHirExpr) -> (u8, String) {
    match expr {
        PreHirExpr::Const(c, _) => (0, format!("const:{c}")),
        PreHirExpr::Var(name) => (1, format!("var:{name}")),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let lhs_key = term_order_key(lhs);
            let rhs_key = term_order_key(rhs);
            if lhs_key <= rhs_key {
                (2, format!("mul:{lhs_key:?}"))
            } else {
                (2, format!("mul:{rhs_key:?}"))
            }
        }
        other => (3, format!("{other:?}")),
    }
}

#[cfg(test)]
mod term_order_tests {
    use super::*;
    // prelude via parent

    #[test]
    fn distributes_shared_multiplicand() {
        let a = PreHirExpr::Var("a".to_string());
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(a.clone()),
                rhs: Box::new(PreHirExpr::Const(2, ty.clone())),
                ty: ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(a.clone()),
                rhs: Box::new(PreHirExpr::Const(3, ty.clone())),
                ty: ty.clone(),
            }),
            ty: ty.clone(),
        };
        let normalized = simplify_distribute_common_factor(&expr).expect("distribute");
        assert_eq!(
            normalized,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(a),
                rhs: Box::new(PreHirExpr::Const(5, ty.clone())),
                ty,
            }
        );
    }
}

#[cfg(test)]
mod width_signed_const_tests {
    use super::*;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn add_const(c: i64) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("esp".to_string())),
            rhs: Box::new(PreHirExpr::Const(c, u32_ty())),
            ty: u32_ty(),
        }
    }

    /// A lifter stores `add esp, -168` as the four-byte `0xffffff58`, which
    /// arrives here as a positive `i64`. Read at its declared width it is
    /// negative, and the emitted body should say so.
    #[test]
    fn rewrites_a_constant_that_is_negative_at_its_declared_width() {
        let out = simplify_negated_const(&add_const(0xffff_ff58)).expect("rewritten");
        let PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            rhs,
            ..
        } = &out
        else {
            panic!("expected a subtraction, got {out:?}");
        };
        assert_eq!(**rhs, PreHirExpr::Const(168, u32_ty()));
    }

    /// The most negative value of the width negates to itself, so rewriting it
    /// would flip `Add` and `Sub` forever inside the normalize fixpoint.
    /// `dexter.dll`'s `DetectShutdown@16` hung on exactly this.
    #[test]
    fn leaves_the_most_negative_value_of_the_width_alone() {
        assert!(simplify_negated_const(&add_const(0x8000_0000)).is_none());
    }

    /// A value below the midpoint is a genuine positive constant.
    #[test]
    fn leaves_an_ordinary_positive_constant_alone() {
        assert!(simplify_negated_const(&add_const(168)).is_none());
    }

    /// A constant wider than the type it carries is not a width-signed value;
    /// the already-signed `c < 0` path owns those.
    #[test]
    fn leaves_a_constant_outside_its_declared_width_alone() {
        assert!(simplify_negated_const(&add_const(0x1_0000_0000)).is_none());
    }
}
