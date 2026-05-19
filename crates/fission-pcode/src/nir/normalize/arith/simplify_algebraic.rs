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
    } = expr else {
        return None;
    };

    // Helper: if `term` matches `common * const`, return (common, const)
    let match_mul_const = |term: &HirExpr, common: &HirExpr| -> Option<(Box<HirExpr>, i64, NirType)> {
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
