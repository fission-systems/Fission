//! Algebraic simplifications for HIR expressions.
//!
//! - `x + (-c)`  -> `x - c`
//! - `x - (-c)`  -> `x + c`
//! - `x + x`     -> `x * 2`

use super::super::*;

pub(crate) fn simplify_negated_const(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ty,
        } => {
            if let HirExpr::Const(c, cty) = rhs.as_ref() {
                if *c < 0 && *c != i64::MIN {
                    return Some(HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: lhs.clone(),
                        rhs: Box::new(HirExpr::Const(-*c, cty.clone())),
                        ty: ty.clone(),
                    });
                }
            }
            if let HirExpr::Const(c, cty) = lhs.as_ref() {
                if *c < 0 && *c != i64::MIN {
                    return Some(HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: rhs.clone(),
                        rhs: Box::new(HirExpr::Const(-*c, cty.clone())),
                        ty: ty.clone(),
                    });
                }
            }
            None
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ty,
        } => {
            if let HirExpr::Const(c, cty) = rhs.as_ref() {
                if *c < 0 && *c != i64::MIN {
                    return Some(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: lhs.clone(),
                        rhs: Box::new(HirExpr::Const(-*c, cty.clone())),
                        ty: ty.clone(),
                    });
                }
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
pub(crate) fn simplify_double_add(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ty,
        } if lhs == rhs => Some(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: lhs.clone(),
            rhs: Box::new(HirExpr::Const(2, ty.clone())),
            ty: ty.clone(),
        }),
        _ => None,
    }
}

/// Factor common multiplicand: `x + x*c` -> `x*(c+1)` and `x*c + x` -> `x*(c+1)`.
pub(crate) fn simplify_factor_common_mul(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };

    // Helper: if `term` matches `common * const`, return (common, const)
    let match_mul_const =
        |term: &HirExpr, common: &HirExpr| -> Option<(Box<HirExpr>, i64, NirType)> {
            if let HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: mul_lhs,
                rhs: mul_rhs,
                ..
            } = term
            {
                if mul_lhs.as_ref() == common {
                    if let HirExpr::Const(c, cty) = mul_rhs.as_ref() {
                        return Some((mul_lhs.clone(), *c, cty.clone()));
                    }
                }
                if mul_rhs.as_ref() == common {
                    if let HirExpr::Const(c, cty) = mul_lhs.as_ref() {
                        return Some((mul_rhs.clone(), *c, cty.clone()));
                    }
                }
            }
            None
        };

    // rhs is lhs * c
    if let Some((common, c, cty)) = match_mul_const(rhs, lhs) {
        if c != i64::MAX {
            return Some(HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: common,
                rhs: Box::new(HirExpr::Const(c + 1, cty)),
                ty: ty.clone(),
            });
        }
    }

    // lhs is rhs * c
    if let Some((common, c, cty)) = match_mul_const(lhs, rhs) {
        if c != i64::MAX {
            return Some(HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: common,
                rhs: Box::new(HirExpr::Const(c + 1, cty)),
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
pub(crate) fn simplify_nested_adds_subs(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        // (a + C1) + C2 or (a - C1) + C2
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs: const2_expr,
            ty,
        } => {
            let HirExpr::Const(c2, cty2) = const2_expr.as_ref() else {
                return None;
            };
            match lhs.as_ref() {
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let HirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_add(*c2)?;
                        return Some(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(HirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let HirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c2.checked_sub(*c1)?;
                        return Some(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(HirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        // (a + C1) - C2 or (a - C1) - C2
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs: const2_expr,
            ty,
        } => {
            let HirExpr::Const(c2, cty2) = const2_expr.as_ref() else {
                return None;
            };
            match lhs.as_ref() {
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let HirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_sub(*c2)?;
                        return Some(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: a.clone(),
                            rhs: Box::new(HirExpr::Const(new_c, cty2.clone())),
                            ty: ty.clone(),
                        });
                    }
                }
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: a,
                    rhs: const1_expr,
                    ..
                } => {
                    if let HirExpr::Const(c1, _) = const1_expr.as_ref() {
                        let new_c = c1.checked_add(*c2)?;
                        return Some(HirExpr::Binary {
                            op: HirBinaryOp::Sub,
                            lhs: a.clone(),
                            rhs: Box::new(HirExpr::Const(new_c, cty2.clone())),
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
pub(crate) fn simplify_collect_mul_terms(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: op @ (HirBinaryOp::Add | HirBinaryOp::Sub),
            lhs,
            rhs,
            ty,
        } => {
            let extract_factor = |term: &HirExpr| -> Option<(HirExpr, i64, NirType)> {
                match term {
                    HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: mul_lhs,
                        rhs: mul_rhs,
                        ..
                    } => {
                        if let HirExpr::Const(c, cty) = mul_rhs.as_ref() {
                            return Some((mul_lhs.as_ref().clone(), *c, cty.clone()));
                        }
                        if let HirExpr::Const(c, cty) = mul_lhs.as_ref() {
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
                if let HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    ..
                } = lhs.as_ref()
                {
                } else if let HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    ..
                } = rhs.as_ref()
                {
                } else {
                    return None;
                }

                let new_c = match op {
                    HirBinaryOp::Add => c_lhs.checked_add(c_rhs)?,
                    HirBinaryOp::Sub => c_lhs.checked_sub(c_rhs)?,
                    _ => unreachable!(),
                };

                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(factor_lhs),
                    rhs: Box::new(HirExpr::Const(new_c, cty_lhs)),
                    ty: ty.clone(),
                });
            }
        }
        _ => {}
    }
    None
}

/// Distribute a shared multiplicand: `a*b + a*c` → `a*(b+c)`.
pub(crate) fn simplify_distribute_common_factor(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
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
    Some(HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs: Box::new(factor_lhs),
        rhs: Box::new(HirExpr::Const(new_c, cty)),
        ty: ty.clone(),
    })
}

fn extract_mul_factor(term: &HirExpr) -> Option<(HirExpr, i64, NirType)> {
    match term {
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(c, cty) = rhs.as_ref() {
                Some((lhs.as_ref().clone(), *c, cty.clone()))
            } else if let HirExpr::Const(c, cty) = lhs.as_ref() {
                Some((rhs.as_ref().clone(), *c, cty.clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Canonicalize commutative add operand order for stable output (RuleTermOrder).
pub(crate) fn simplify_term_order_add(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
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
    Some(HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: rhs.clone(),
        rhs: lhs.clone(),
        ty: ty.clone(),
    })
}

fn term_order_key(expr: &HirExpr) -> (u8, String) {
    match expr {
        HirExpr::Const(c, _) => (0, format!("const:{c}")),
        HirExpr::Var(name) => (1, format!("var:{name}")),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
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

    #[test]
    fn distributes_shared_multiplicand() {
        let a = HirExpr::Var("a".to_string());
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(a.clone()),
                rhs: Box::new(HirExpr::Const(2, ty.clone())),
                ty: ty.clone(),
            }),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(a.clone()),
                rhs: Box::new(HirExpr::Const(3, ty.clone())),
                ty: ty.clone(),
            }),
            ty: ty.clone(),
        };
        let normalized = simplify_distribute_common_factor(&expr).expect("distribute");
        assert_eq!(
            normalized,
            HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(a),
                rhs: Box::new(HirExpr::Const(5, ty.clone())),
                ty,
            }
        );
    }
}
