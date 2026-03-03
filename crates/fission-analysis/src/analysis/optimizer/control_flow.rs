//! Control Flow Normalization
//!
//! Implements Ghidra's ActionNormalizeBranches:
//! - Converts double negatives to positive conditions
//! - Normalizes condition order (constant on right)
//! - Simplifies redundant boolean operations

use super::{BinOpKind, Expr, Stmt, UnaryOpKind};

/// Normalize control flow statements
pub fn normalize(stmts: Vec<Stmt>) -> Vec<Stmt> {
    stmts.into_iter().map(normalize_stmt).collect()
}

fn normalize_stmt(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            let condition = normalize_condition(condition);
            let then_block = normalize(then_block);
            let else_block = else_block.map(normalize);

            // Try to simplify: if (!!x) → if (x)
            let condition = remove_double_negation(condition);

            Stmt::If {
                condition,
                then_block,
                else_block,
            }
        }
        Stmt::While { condition, body } => {
            let condition = normalize_condition(condition);
            let condition = remove_double_negation(condition);
            let body = normalize(body);

            Stmt::While { condition, body }
        }
        Stmt::Return(Some(expr)) => Stmt::Return(Some(normalize_expr(expr))),
        other => other,
    }
}

/// Normalize a condition expression
fn normalize_condition(expr: Expr) -> Expr {
    let expr = normalize_expr(expr);

    // if (!(x < y)) → if (x >= y)
    if let Expr::UnaryOp {
        op: UnaryOpKind::Not,
        operand,
    } = &expr
    {
        if let Some(inverted) = try_invert_comparison(operand) {
            return inverted;
        }
    }

    expr
}

/// Remove double negation: !!x → x
fn remove_double_negation(expr: Expr) -> Expr {
    match expr {
        Expr::UnaryOp {
            op: UnaryOpKind::Not,
            operand,
        } => {
            if let Expr::UnaryOp {
                op: UnaryOpKind::Not,
                operand: inner,
            } = *operand
            {
                *inner
            } else {
                Expr::UnaryOp {
                    op: UnaryOpKind::Not,
                    operand,
                }
            }
        }
        other => other,
    }
}

/// Try to invert a comparison: !(x < y) → x >= y
fn try_invert_comparison(expr: &Expr) -> Option<Expr> {
    if let Expr::BinOp { op, left, right } = expr {
        let inverted_op = match op {
            BinOpKind::Lt => BinOpKind::Ge,
            BinOpKind::Le => BinOpKind::Gt,
            BinOpKind::Gt => BinOpKind::Le,
            BinOpKind::Ge => BinOpKind::Lt,
            BinOpKind::Eq => BinOpKind::Ne,
            BinOpKind::Ne => BinOpKind::Eq,
            _ => return None,
        };

        return Some(Expr::BinOp {
            op: inverted_op,
            left: left.clone(),
            right: right.clone(),
        });
    }
    None
}

/// Normalize an expression
fn normalize_expr(expr: Expr) -> Expr {
    match expr {
        Expr::BinOp { op, left, right } => {
            let left = Box::new(normalize_expr(*left));
            let right = Box::new(normalize_expr(*right));

            // Normalize constant position: (5 == x) → (x == 5)
            if is_const(&left) && !is_const(&right) {
                if is_commutative(&op) {
                    return Expr::BinOp {
                        op,
                        left: right,
                        right: left,
                    };
                }
                // For non-commutative: (5 < x) → (x > 5)
                if let Some(flipped_op) = flip_comparison(&op) {
                    return Expr::BinOp {
                        op: flipped_op,
                        left: right,
                        right: left,
                    };
                }
            }

            Expr::BinOp { op, left, right }
        }
        Expr::UnaryOp { op, operand } => Expr::UnaryOp {
            op,
            operand: Box::new(normalize_expr(*operand)),
        },
        Expr::Call { name, args } => Expr::Call {
            name,
            args: args.into_iter().map(normalize_expr).collect(),
        },
        Expr::Assign { target, value } => Expr::Assign {
            target,
            value: Box::new(normalize_expr(*value)),
        },
        other => other,
    }
}

fn is_const(expr: &Expr) -> bool {
    matches!(expr, Expr::Const(_))
}

fn is_commutative(op: &BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Add
            | BinOpKind::Mul
            | BinOpKind::And
            | BinOpKind::Or
            | BinOpKind::Xor
            | BinOpKind::Eq
            | BinOpKind::Ne
    )
}

fn flip_comparison(op: &BinOpKind) -> Option<BinOpKind> {
    match op {
        BinOpKind::Lt => Some(BinOpKind::Gt),
        BinOpKind::Le => Some(BinOpKind::Ge),
        BinOpKind::Gt => Some(BinOpKind::Lt),
        BinOpKind::Ge => Some(BinOpKind::Le),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_negation() {
        let expr = Expr::UnaryOp {
            op: UnaryOpKind::Not,
            operand: Box::new(Expr::UnaryOp {
                op: UnaryOpKind::Not,
                operand: Box::new(Expr::Var("x".to_string())),
            }),
        };
        assert_eq!(remove_double_negation(expr), Expr::Var("x".to_string()));
    }

    #[test]
    fn test_invert_lt() {
        let expr = Expr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Var("y".to_string())),
        };
        let Some(inverted) = try_invert_comparison(&expr) else {
            panic!("comparison inversion should succeed")
        };
        if let Expr::BinOp { op, .. } = inverted {
            assert_eq!(op, BinOpKind::Ge);
        } else {
            panic!("Expected BinOp");
        }
    }

    #[test]
    fn test_normalize_const_order() {
        // (5 == x) → (x == 5)
        let expr = Expr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(Expr::Const(5)),
            right: Box::new(Expr::Var("x".to_string())),
        };
        let normalized = normalize_expr(expr);
        if let Expr::BinOp { left, right, .. } = normalized {
            assert!(matches!(*left, Expr::Var(_)));
            assert!(matches!(*right, Expr::Const(_)));
        } else {
            panic!("Expected BinOp");
        }
    }

    #[test]
    fn test_normalize_if_not_lt() {
        // if (!(x < y)) then ... → if (x >= y) then ...
        let condition = Expr::UnaryOp {
            op: UnaryOpKind::Not,
            operand: Box::new(Expr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("y".to_string())),
            }),
        };

        let stmt = Stmt::If {
            condition,
            then_block: vec![],
            else_block: None,
        };

        let normalized = normalize_stmt(stmt);
        if let Stmt::If { condition, .. } = normalized {
            if let Expr::BinOp { op, .. } = condition {
                assert_eq!(op, BinOpKind::Ge);
            } else {
                panic!("Expected BinOp");
            }
        } else {
            panic!("Expected If");
        }
    }

    #[test]
    fn test_flip_comparison() {
        // (5 < x) → (x > 5)
        let expr = Expr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(Expr::Const(5)),
            right: Box::new(Expr::Var("x".to_string())),
        };
        let normalized = normalize_expr(expr);
        if let Expr::BinOp { op, left, right } = normalized {
            assert_eq!(op, BinOpKind::Gt);
            assert!(matches!(*left, Expr::Var(_)));
            assert!(matches!(*right, Expr::Const(5)));
        } else {
            panic!("Expected BinOp");
        }
    }
}
