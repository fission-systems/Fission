//! Pure logical expression helpers.

use crate::ir::{NirType, PreHirBinaryOp, PreHirExpr, PreHirUnaryOp};

pub fn fold_logical_chain(mut exprs: Vec<PreHirExpr>, op: PreHirBinaryOp) -> PreHirExpr {
    debug_assert!(matches!(
        op,
        PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::LogicalOr
    ));
    if exprs.is_empty() {
        return PreHirExpr::Const(
            if op == PreHirBinaryOp::LogicalAnd {
                1
            } else {
                0
            },
            NirType::Bool,
        );
    }
    let first = exprs.remove(0);
    exprs
        .into_iter()
        .fold(first, |lhs, rhs| PreHirExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        })
}

pub fn negate_expr(expr: PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

pub fn strip_casts(expr: &PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other.clone(),
    }
}

pub fn simplify_logical_expr(expr: PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::LogicalOr,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ty,
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::LogicalAnd,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ty,
            }
        }
        other => other,
    }
}
