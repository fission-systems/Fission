use crate::prelude::*;

/// The trait representing an AST simplification rule.
pub trait Rule {
    fn name(&self) -> &'static str;
    fn apply_stmt(&self, _stmt: &mut DirStmt) -> bool {
        false
    }
    fn apply_expr(&self, _expr: &mut DirExpr) -> bool {
        false
    }
}

/// Simplifies double negations: `!(!x)` -> `x` and `~(~x)` -> `x`.
struct RuleSimplifyDoubleNegation;

impl Rule for RuleSimplifyDoubleNegation {
    fn name(&self) -> &'static str {
        "simplify_double_negation"
    }

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::Unary {
            op: op1,
            expr: inner1,
            ty: _,
        } = expr
        {
            if let DirExpr::Unary {
                op: op2,
                expr: inner2,
                ty: _,
            } = inner1.as_mut()
            {
                if (*op1 == DirUnaryOp::Not && *op2 == DirUnaryOp::Not)
                    || (*op1 == DirUnaryOp::BitNot && *op2 == DirUnaryOp::BitNot)
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        match expr {
            DirExpr::Binary { op, lhs, rhs, ty } => {
                match (lhs.as_ref(), rhs.as_ref()) {
                    // Fold double constants
                    (DirExpr::Const(c1, _), DirExpr::Const(c2, _)) => {
                        let val = match op {
                            DirBinaryOp::Add => Some(c1.wrapping_add(*c2)),
                            DirBinaryOp::Sub => Some(c1.wrapping_sub(*c2)),
                            DirBinaryOp::Mul => Some(c1.wrapping_mul(*c2)),
                            DirBinaryOp::Div if *c2 != 0 => Some(c1.wrapping_div(*c2)),
                            DirBinaryOp::Mod if *c2 != 0 => Some(c1.wrapping_rem(*c2)),
                            DirBinaryOp::And => Some(c1 & c2),
                            DirBinaryOp::Or => Some(c1 | c2),
                            DirBinaryOp::Xor => Some(c1 ^ c2),
                            DirBinaryOp::Shl => Some(c1.wrapping_shl(*c2 as u32)),
                            DirBinaryOp::Shr => Some((*c1 as u64).wrapping_shr(*c2 as u32) as i64),
                            DirBinaryOp::Sar => Some(c1.wrapping_shr(*c2 as u32)),
                            _ => None,
                        };
                        if let Some(v) = val {
                            *expr = DirExpr::Const(v, ty.clone());
                            return true;
                        }
                    }
                    // x + 0 -> x
                    (other, DirExpr::Const(0, _)) if *op == DirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(0, _), other) if *op == DirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    // x - 0 -> x
                    (other, DirExpr::Const(0, _)) if *op == DirBinaryOp::Sub => {
                        *expr = other.clone();
                        return true;
                    }
                    // x * 0 -> 0, x * 1 -> x
                    (other, DirExpr::Const(c, _)) if *op == DirBinaryOp::Mul => {
                        if *c == 0 {
                            *expr = DirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    (DirExpr::Const(c, _), other) if *op == DirBinaryOp::Mul => {
                        if *c == 0 {
                            *expr = DirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            DirExpr::Unary {
                op: DirUnaryOp::Neg,
                expr: inner,
                ty,
            } => {
                if let DirExpr::Const(c, _) = inner.as_ref() {
                    *expr = DirExpr::Const(c.wrapping_neg(), ty.clone());
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::Binary { op, lhs, rhs, ty } = expr {
            match op {
                DirBinaryOp::And => match (lhs.as_ref(), rhs.as_ref()) {
                    (_, DirExpr::Const(0, _)) => {
                        *expr = DirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (DirExpr::Const(0, _), _) => {
                        *expr = DirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (other, DirExpr::Const(-1, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(-1, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                DirBinaryOp::Or => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, DirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                DirBinaryOp::Xor => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, DirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(0, _), other) => {
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: _,
        } = expr
        {
            match op {
                DirBinaryOp::LogicalAnd => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, DirExpr::Const(1, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(1, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    (_, DirExpr::Const(0, _)) => {
                        *expr = DirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    (DirExpr::Const(0, _), _) => {
                        *expr = DirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    _ => {}
                },
                DirBinaryOp::LogicalOr => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, DirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (DirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    (_, DirExpr::Const(1, _)) => {
                        *expr = DirExpr::Const(1, NirType::Bool);
                        return true;
                    }
                    (DirExpr::Const(1, _), _) => {
                        *expr = DirExpr::Const(1, NirType::Bool);
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::PtrOffset { base, offset: 0 } = expr {
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs,
            rhs,
            ty,
        } = expr
        {
            match (lhs.as_ref(), rhs.as_ref()) {
                (other, DirExpr::Const(2, _)) => {
                    *expr = DirExpr::Binary {
                        op: DirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(DirExpr::Const(1, ty.clone())),
                        ty: ty.clone(),
                    };
                    return true;
                }
                (DirExpr::Const(2, _), other) => {
                    *expr = DirExpr::Binary {
                        op: DirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(DirExpr::Const(1, ty.clone())),
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

    fn apply_expr(&self, expr: &mut DirExpr) -> bool {
        if let DirExpr::Select {
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
            if let DirExpr::Const(val, _) = cond.as_ref() {
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
pub fn apply_rule_normalization(func: &mut DirFunction) -> bool {
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

fn apply_rules_to_stmts(stmts: &mut [DirStmt], rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_rules_to_stmt(stmt, rules);
    }
    changed
}

fn apply_rules_to_stmt(stmt: &mut DirStmt, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    // Apply rules directly to the statement
    for rule in rules {
        changed |= rule.apply_stmt(stmt);
    }

    // Recurse and apply to inner expressions/statements
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            changed |= apply_rules_to_lvalue(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            changed |= apply_rules_to_expr(expr, rules);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            changed |= apply_rules_to_stmts(body, rules);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_stmts(then_body, rules);
            changed |= apply_rules_to_stmts(else_body, rules);
        }
        DirStmt::Switch {
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

fn apply_rules_to_lvalue(lhs: &mut DirLValue, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;
    match lhs {
        DirLValue::Deref { ptr, .. } => {
            changed |= apply_rules_to_expr(ptr, rules);
        }
        DirLValue::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules);
            changed |= apply_rules_to_expr(index, rules);
        }
        _ => {}
    }
    changed
}

fn apply_rules_to_expr(expr: &mut DirExpr, rules: &[Box<dyn Rule>]) -> bool {
    let mut changed = false;

    // First apply rules to sub-expressions recursively
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::Load { ptr: inner, .. } => {
            changed |= apply_rules_to_expr(inner, rules);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= apply_rules_to_expr(lhs, rules);
            changed |= apply_rules_to_expr(rhs, rules);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= apply_rules_to_expr(cond, rules);
            changed |= apply_rules_to_expr(then_expr, rules);
            changed |= apply_rules_to_expr(else_expr, rules);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= apply_rules_to_expr(arg, rules);
            }
        }
        DirExpr::Index { base, index, .. } => {
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
        let cond = DirExpr::Var("cond".to_string());
        let val = DirExpr::Var("val".to_string());
        let mut select_expr = DirExpr::Select {
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
