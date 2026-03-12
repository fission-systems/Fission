//! Bit Operation Simplification Rules
//!
//! Implements Ghidra's bit manipulation simplification rules:
//! - RuleOrMask: (x | 0xFF) & 0xFF → x & 0xFF
//! - RuleAndMask: (x & 0xFF) | 0xFF → 0xFF
//! - RuleBxor2NotEqual: (x ^ y) == 0 → x == y
//! - RuleShiftBitops: (x << 8) >> 8 → x & 0xFF
//! - RuleHighOrderAnd: High-bit masking optimizations

use super::{BinOpKind, Expr};

/// Simplify bit operations in an expression
pub fn simplify_bitops(expr: Expr) -> Expr {
    match expr {
        Expr::BinOp { op, left, right } => {
            // Recursively simplify children first
            let left = Box::new(simplify_bitops(*left));
            let right = Box::new(simplify_bitops(*right));

            // Try to apply simplification rules
            if let Some(simplified) = try_simplify(&op, &left, &right) {
                simplified
            } else {
                Expr::BinOp { op, left, right }
            }
        }
        Expr::UnaryOp { op, operand } => {
            let operand = Box::new(simplify_bitops(*operand));
            Expr::UnaryOp { op, operand }
        }
        Expr::Call { name, args } => Expr::Call {
            name,
            args: args.into_iter().map(simplify_bitops).collect(),
        },
        Expr::Assign { target, value } => Expr::Assign {
            target,
            value: Box::new(simplify_bitops(*value)),
        },
        other => other,
    }
}

fn try_simplify(op: &BinOpKind, left: &Expr, right: &Expr) -> Option<Expr> {
    match op {
        BinOpKind::And => try_simplify_and(left, right),
        BinOpKind::Or => try_simplify_or(left, right),
        BinOpKind::Xor => try_simplify_xor(left, right),
        BinOpKind::Eq | BinOpKind::Ne => try_simplify_comparison(*op, left, right),
        BinOpKind::Shl | BinOpKind::Shr => try_simplify_shift(*op, left, right),
        _ => None,
    }
}

/// RuleAndMask: Simplify AND operations with masks
fn try_simplify_and(left: &Expr, right: &Expr) -> Option<Expr> {
    // (x | mask1) & mask2 where mask2 is subset of mask1 → x & mask2
    if let Expr::BinOp {
        op: BinOpKind::Or,
        left: inner_left,
        right: inner_right,
    } = left
    {
        if let (Expr::Const(mask1), Expr::Const(mask2)) = (&**inner_right, right) {
            if (mask2 & mask1) == *mask2 {
                return Some(Expr::BinOp {
                    op: BinOpKind::And,
                    left: inner_left.clone(),
                    right: Box::new(Expr::Const(*mask2)),
                });
            }
        }
    }

    // x & 0 → 0
    if *right == Expr::Const(0) {
        return Some(Expr::Const(0));
    }

    // x & -1 (all bits set) → x
    if *right == Expr::Const(-1) {
        return Some((*left).clone());
    }

    // (x & mask1) & mask2 → x & (mask1 & mask2)
    if let Expr::BinOp {
        op: BinOpKind::And,
        left: inner_left,
        right: inner_right,
    } = left
    {
        if let (Expr::Const(mask1), Expr::Const(mask2)) = (&**inner_right, right) {
            return Some(Expr::BinOp {
                op: BinOpKind::And,
                left: inner_left.clone(),
                right: Box::new(Expr::Const(mask1 & mask2)),
            });
        }
    }

    None
}

/// RuleOrMask: Simplify OR operations with masks
fn try_simplify_or(left: &Expr, right: &Expr) -> Option<Expr> {
    // (x & mask1) | mask2 where mask1 is subset of mask2 → mask2
    if let Expr::BinOp {
        op: BinOpKind::And,
        left: _inner_left,
        right: inner_right,
    } = left
    {
        if let (Expr::Const(mask1), Expr::Const(mask2)) = (&**inner_right, right) {
            if (mask1 & mask2) == *mask1 {
                return Some(Expr::Const(*mask2));
            }
        }
    }

    // x | 0 → x
    if *right == Expr::Const(0) {
        return Some((*left).clone());
    }

    // x | -1 → -1
    if *right == Expr::Const(-1) {
        return Some(Expr::Const(-1));
    }

    // (x | mask1) | mask2 → x | (mask1 | mask2)
    if let Expr::BinOp {
        op: BinOpKind::Or,
        left: inner_left,
        right: inner_right,
    } = left
    {
        if let (Expr::Const(mask1), Expr::Const(mask2)) = (&**inner_right, right) {
            return Some(Expr::BinOp {
                op: BinOpKind::Or,
                left: inner_left.clone(),
                right: Box::new(Expr::Const(mask1 | mask2)),
            });
        }
    }

    None
}

/// RuleBxor2NotEqual: (x ^ y) == 0 → x == y
fn try_simplify_xor(left: &Expr, right: &Expr) -> Option<Expr> {
    // x ^ 0 → x
    if *right == Expr::Const(0) {
        return Some((*left).clone());
    }

    // x ^ x → 0
    if left == right {
        return Some(Expr::Const(0));
    }

    None
}

/// Simplify comparisons involving XOR
fn try_simplify_comparison(op: BinOpKind, left: &Expr, right: &Expr) -> Option<Expr> {
    // (x ^ y) == 0 → x == y
    if op == BinOpKind::Eq {
        if let Expr::BinOp {
            op: BinOpKind::Xor,
            left: xor_left,
            right: xor_right,
        } = left
        {
            if *right == Expr::Const(0) {
                return Some(Expr::BinOp {
                    op: BinOpKind::Eq,
                    left: xor_left.clone(),
                    right: xor_right.clone(),
                });
            }
        }
    }

    // (x ^ y) != 0 → x != y
    if op == BinOpKind::Ne {
        if let Expr::BinOp {
            op: BinOpKind::Xor,
            left: xor_left,
            right: xor_right,
        } = left
        {
            if *right == Expr::Const(0) {
                return Some(Expr::BinOp {
                    op: BinOpKind::Ne,
                    left: xor_left.clone(),
                    right: xor_right.clone(),
                });
            }
        }
    }

    None
}

/// RuleShiftBitops: (x << n) >> n → x & mask
fn try_simplify_shift(op: BinOpKind, left: &Expr, right: &Expr) -> Option<Expr> {
    // (x << n) >> n → x & ((1 << (bits - n)) - 1)
    if op == BinOpKind::Shr {
        if let Expr::BinOp {
            op: BinOpKind::Shl,
            left: shl_left,
            right: shl_right,
        } = left
        {
            // Check if shift amounts are the same
            if **shl_right == *right {
                if let Expr::Const(shift) = *right {
                    if shift > 0 && shift < 64 {
                        let mask = (1i64 << (64 - shift)) - 1;
                        return Some(Expr::BinOp {
                            op: BinOpKind::And,
                            left: shl_left.clone(),
                            right: Box::new(Expr::Const(mask)),
                        });
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_and_with_zero() {
        let expr = Expr::BinOp {
            op: BinOpKind::And,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Const(0)),
        };
        assert_eq!(simplify_bitops(expr), Expr::Const(0));
    }

    #[test]
    fn test_or_with_zero() {
        let expr = Expr::BinOp {
            op: BinOpKind::Or,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Const(0)),
        };
        assert_eq!(simplify_bitops(expr), Expr::Var("x".to_string()));
    }

    #[test]
    fn test_xor_with_zero() {
        let expr = Expr::BinOp {
            op: BinOpKind::Xor,
            left: Box::new(Expr::Var("x".to_string())),
            right: Box::new(Expr::Const(0)),
        };
        assert_eq!(simplify_bitops(expr), Expr::Var("x".to_string()));
    }

    #[test]
    fn test_xor_eq_zero() {
        // (x ^ y) == 0 → x == y
        let expr = Expr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(Expr::BinOp {
                op: BinOpKind::Xor,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Var("y".to_string())),
            }),
            right: Box::new(Expr::Const(0)),
        };
        let result = simplify_bitops(expr);
        if let Expr::BinOp { op, left, right } = result {
            assert_eq!(op, BinOpKind::Eq);
            assert_eq!(*left, Expr::Var("x".to_string()));
            assert_eq!(*right, Expr::Var("y".to_string()));
        } else {
            panic!("Expected BinOp");
        }
    }

    #[test]
    fn test_shift_mask() {
        // (x << 8) >> 8 → x & 0x00FFFFFFFFFFFFFF
        let expr = Expr::BinOp {
            op: BinOpKind::Shr,
            left: Box::new(Expr::BinOp {
                op: BinOpKind::Shl,
                left: Box::new(Expr::Var("x".to_string())),
                right: Box::new(Expr::Const(8)),
            }),
            right: Box::new(Expr::Const(8)),
        };
        let result = simplify_bitops(expr);
        if let Expr::BinOp {
            op: BinOpKind::And,
            left,
            right,
        } = result
        {
            assert_eq!(*left, Expr::Var("x".to_string()));
            if let Expr::Const(mask) = *right {
                assert_eq!(mask, (1i64 << 56) - 1);
            } else {
                panic!("Expected Const mask");
            }
        } else {
            panic!("Expected AND operation");
        }
    }
}
