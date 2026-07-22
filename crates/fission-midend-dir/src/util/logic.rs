//! Pure logical expression helpers.

use crate::ir::{DirBinaryOp, DirExpr, DirUnaryOp, NirType};

pub fn fold_logical_chain(mut exprs: Vec<DirExpr>, op: DirBinaryOp) -> DirExpr {
    debug_assert!(matches!(
        op,
        DirBinaryOp::LogicalAnd | DirBinaryOp::LogicalOr
    ));
    if exprs.is_empty() {
        return DirExpr::Const(
            if op == DirBinaryOp::LogicalAnd { 1 } else { 0 },
            NirType::Bool,
        );
    }
    let first = exprs.remove(0);
    exprs.into_iter().fold(first, |lhs, rhs| DirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    })
}

pub fn negate_expr(expr: DirExpr) -> DirExpr {
    match expr {
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

pub fn strip_casts(expr: &DirExpr) -> DirExpr {
    match expr {
        DirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other.clone(),
    }
}

pub fn simplify_logical_expr(expr: DirExpr) -> DirExpr {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::LogicalOr,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            DirExpr::Binary {
                op: DirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ty,
            }
        }
        DirExpr::Binary {
            op: DirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::LogicalAnd,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            DirExpr::Binary {
                op: DirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ty,
            }
        }
        other => other,
    }
}
