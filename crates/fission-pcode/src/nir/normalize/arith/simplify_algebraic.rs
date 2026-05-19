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
