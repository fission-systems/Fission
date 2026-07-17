use crate::prelude::*;

/// The trait representing an AST simplification rule.
pub trait Rule {
    fn name(&self) -> &'static str;
    fn apply_stmt(&self, _stmt: &mut HirStmt) -> bool {
        false
    }
    fn apply_expr(&self, _expr: &mut HirExpr) -> bool {
        false
    }
}

/// Simplifies double negations: `!(!x)` -> `x` and `~(~x)` -> `x`.
struct RuleSimplifyDoubleNegation;

impl Rule for RuleSimplifyDoubleNegation {
    fn name(&self) -> &'static str {
        "simplify_double_negation"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::Unary {
            op: op1,
            expr: inner1,
            ty: _,
        } = expr
        {
            if let HirExpr::Unary {
                op: op2,
                expr: inner2,
                ty: _,
            } = inner1.as_mut()
            {
                if (*op1 == HirUnaryOp::Not && *op2 == HirUnaryOp::Not)
                    || (*op1 == HirUnaryOp::BitNot && *op2 == HirUnaryOp::BitNot)
                {
                    *expr = (**inner2).clone();
                    return true;
                }
            }
        }
        false
    }
}

/// Folds constant expressions and simplifies algebraic identities:
/// - `C1 + C2` -> `C`
/// - `x + 0` -> `x`
/// - `x - 0` -> `x`
/// - `x * 0` -> `0`
/// - `x * 1` -> `x`
struct RuleFoldConstants;

impl Rule for RuleFoldConstants {
    fn name(&self) -> &'static str {
        "fold_constants"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        match expr {
            HirExpr::Binary { op, lhs, rhs, ty } => {
                match (lhs.as_ref(), rhs.as_ref()) {
                    // Fold double constants
                    (HirExpr::Const(c1, _), HirExpr::Const(c2, _)) => {
                        let val = match op {
                            HirBinaryOp::Add => Some(c1.wrapping_add(*c2)),
                            HirBinaryOp::Sub => Some(c1.wrapping_sub(*c2)),
                            HirBinaryOp::Mul => Some(c1.wrapping_mul(*c2)),
                            HirBinaryOp::Div if *c2 != 0 => Some(c1.wrapping_div(*c2)),
                            HirBinaryOp::Mod if *c2 != 0 => Some(c1.wrapping_rem(*c2)),
                            HirBinaryOp::And => Some(c1 & c2),
                            HirBinaryOp::Or => Some(c1 | c2),
                            HirBinaryOp::Xor => Some(c1 ^ c2),
                            HirBinaryOp::Shl => Some(c1.wrapping_shl(*c2 as u32)),
                            HirBinaryOp::Shr => Some((*c1 as u64).wrapping_shr(*c2 as u32) as i64),
                            HirBinaryOp::Sar => Some(c1.wrapping_shr(*c2 as u32)),
                            _ => None,
                        };
                        if let Some(v) = val {
                            *expr = HirExpr::Const(v, ty.clone());
                            return true;
                        }
                    }
                    // x + 0 -> x
                    (other, HirExpr::Const(0, _)) if *op == HirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(0, _), other) if *op == HirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    // x - 0 -> x
                    (other, HirExpr::Const(0, _)) if *op == HirBinaryOp::Sub => {
                        *expr = other.clone();
                        return true;
                    }
                    // x * 0 -> 0, x * 1 -> x
                    (other, HirExpr::Const(c, _)) if *op == HirBinaryOp::Mul => {
                        if *c == 0 {
                            *expr = HirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    (HirExpr::Const(c, _), other) if *op == HirBinaryOp::Mul => {
                        if *c == 0 {
                            *expr = HirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            HirExpr::Unary {
                op: HirUnaryOp::Neg,
                expr: inner,
                ty,
            } => {
                if let HirExpr::Const(c, _) = inner.as_ref() {
                    *expr = HirExpr::Const(c.wrapping_neg(), ty.clone());
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

/// Simplifies bitwise identities:
/// - `x & 0` -> `0`
/// - `x & -1` -> `x`
/// - `x | 0` -> `x`
/// - `x ^ 0` -> `x`
struct RuleBitwiseIdentities;

impl Rule for RuleBitwiseIdentities {
    fn name(&self) -> &'static str {
        "bitwise_identities"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::Binary { op, lhs, rhs, ty } = expr {
            match op {
                HirBinaryOp::And => match (lhs.as_ref(), rhs.as_ref()) {
                    (_, HirExpr::Const(0, _)) => {
                        *expr = HirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (HirExpr::Const(0, _), _) => {
                        *expr = HirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (other, HirExpr::Const(-1, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(-1, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                HirBinaryOp::Or => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, HirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                HirBinaryOp::Xor => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, HirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        false
    }
}

/// Simplifies logical identities:
/// - `x && true` -> `x`
/// - `x || false` -> `x`
struct RuleLogicalIdentities;

impl Rule for RuleLogicalIdentities {
    fn name(&self) -> &'static str {
        "logical_identities"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: _,
        } = expr
        {
            match op {
                HirBinaryOp::LogicalAnd => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, HirExpr::Const(1, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(1, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    (_, HirExpr::Const(0, _)) => {
                        *expr = HirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    (HirExpr::Const(0, _), _) => {
                        *expr = HirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    _ => {}
                },
                HirBinaryOp::LogicalOr => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, HirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (HirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    (_, HirExpr::Const(1, _)) => {
                        *expr = HirExpr::Const(1, NirType::Bool);
                        return true;
                    }
                    (HirExpr::Const(1, _), _) => {
                        *expr = HirExpr::Const(1, NirType::Bool);
                        return true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        false
    }
}

/// Collapses zero offset pointer calculations or redundant casts:
/// - `PtrOffset(base, 0)` -> `base`
struct RuleCollapseZeroOffset;

impl Rule for RuleCollapseZeroOffset {
    fn name(&self) -> &'static str {
        "collapse_zero_offset"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::PtrOffset { base, offset: 0 } = expr {
            *expr = (**base).clone();
            return true;
        }
        false
    }
}

struct RuleSimplifyMulToShl;

impl Rule for RuleSimplifyMulToShl {
    fn name(&self) -> &'static str {
        "simplify_mul_to_shl"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ty,
        } = expr
        {
            match (lhs.as_ref(), rhs.as_ref()) {
                (other, HirExpr::Const(2, _)) => {
                    *expr = HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(HirExpr::Const(1, ty.clone())),
                        ty: ty.clone(),
                    };
                    return true;
                }
                (HirExpr::Const(2, _), other) => {
                    *expr = HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(HirExpr::Const(1, ty.clone())),
                        ty: ty.clone(),
                    };
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

struct RuleSimplifySelect;

impl Rule for RuleSimplifySelect {
    fn name(&self) -> &'static str {
        "simplify_select"
    }

    fn apply_expr(&self, expr: &mut HirExpr) -> bool {
        if let HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } = expr
        {
            // Rule 1: cond ? A : A -> A
            if then_expr == else_expr {
                *expr = (**then_expr).clone();
                return true;
            }
            // Rule 2: true ? A : B -> A, false ? A : B -> B
            if let HirExpr::Const(val, _) = cond.as_ref() {
                if *val != 0 {
                    *expr = (**then_expr).clone();
                } else {
                    *expr = (**else_expr).clone();
                }
                return true;
            }
        }
        false
    }
}

/// Applies a list of rules to the function AST iteratively until convergence.
pub fn apply_rule_normalization(func: &mut HirFunction) -> bool {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(RuleSimplifyDoubleNegation),
        Box::new(RuleFoldConstants),
        Box::new(RuleBitwiseIdentities),
        Box::new(RuleLogicalIdentities),
        Box::new(RuleCollapseZeroOffset),
        Box::new(RuleSimplifyMulToShl),
        Box::new(RuleSimplifySelect),
    ];

    let mut changed = false;
    let mut loop_changed = true;
    let mut round = 0;

    while loop_changed && round < 15 {
        loop_changed = false;
        round += 1;

        loop_changed |= apply_rules_to_stmts(&mut func.body, &rules);
        changed |= loop_changed;
    }

    changed
}

fn apply_rules_to_stmts(stmts: &mut [HirStmt], rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_rules_to_stmt(stmt, rules);
    }
    changed
}

fn apply_rules_to_stmt(stmt: &mut HirStmt, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    // Apply rules directly to the statement
    for rule in rules {
        changed |= rule.apply_stmt(stmt);
    }

    // Recurse and apply to inner expressions/statements
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= apply_rules_to_lvalue(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= apply_rules_to_expr(expr, rules);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            changed |= apply_rules_to_stmts(body, rules);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_stmts(then_body, rules);
            changed |= apply_rules_to_stmts(else_body, rules);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= apply_rules_to_expr(expr, rules);
            for case in cases {
                changed |= apply_rules_to_stmts(&mut case.body, rules);
            }
            changed |= apply_rules_to_stmts(default, rules);
        }
        _ => {}
    }

    changed
}

fn apply_rules_to_lvalue(lhs: &mut HirLValue, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    match lhs {
        HirLValue::Deref { ptr, .. } => {
            changed |= apply_rules_to_expr(ptr, rules);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }
    changed
}

fn apply_rules_to_expr(expr: &mut HirExpr, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    // First apply rules to sub-expressions recursively
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::Load { ptr: inner, .. } => {
            changed |= apply_rules_to_expr(inner, rules);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= apply_rules_to_expr(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_expr(then_expr, rules);
            changed |= apply_rules_to_expr(else_expr, rules);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= apply_rules_to_expr(arg, rules);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }

    // Apply rules directly to this expression
    for rule in rules {
        changed |= rule.apply_expr(expr);
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    #[test]
    fn test_rule_simplify_select() {
        let cond = HirExpr::Var("cond".to_string());
        let val = HirExpr::Var("val".to_string());
        let mut select_expr = HirExpr::Select {
            cond: Box::new(cond.clone()),
            then_expr: Box::new(val.clone()),
            else_expr: Box::new(val.clone()),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        };
        let rule = RuleSimplifySelect;
        let changed = rule.apply_expr(&mut select_expr);
        assert!(changed);
        assert_eq!(select_expr, val);
    }
}
