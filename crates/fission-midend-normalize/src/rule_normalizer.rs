use crate::HashMap;
use crate::cleanup::expr_has_side_effects;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

/// The trait representing an AST simplification rule.
pub trait Rule {
    fn name(&self) -> &'static str;
    fn apply_stmt(&self, _stmt: &mut PreHirStmt) -> bool {
        false
    }
    fn apply_expr(&self, _expr: &mut PreHirExpr) -> bool {
        false
    }
    fn apply_expr_with_types(
        &self,
        expr: &mut PreHirExpr,
        _type_map: &HashMap<String, NirType>,
    ) -> bool {
        self.apply_expr(expr)
    }
}

fn is_bool_expr(expr: &PreHirExpr, type_map: &HashMap<String, NirType>) -> bool {
    match expr {
        PreHirExpr::Var(name) => matches!(type_map.get(name), Some(NirType::Bool)),
        PreHirExpr::Const(v, NirType::Bool) => *v == 0 || *v == 1,
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            ..
        } => true,
        PreHirExpr::Binary {
            op:
                PreHirBinaryOp::Eq
                | PreHirBinaryOp::Ne
                | PreHirBinaryOp::Lt
                | PreHirBinaryOp::Le
                | PreHirBinaryOp::Gt
                | PreHirBinaryOp::Ge
                | PreHirBinaryOp::LogicalAnd
                | PreHirBinaryOp::LogicalOr,
            ..
        } => true,
        _ => expr_type(expr) == NirType::Bool,
    }
}

/// Simplifies double negations: `!(!x)` -> `x` and `~(~x)` -> `x`.
struct RuleSimplifyDoubleNegation;

impl Rule for RuleSimplifyDoubleNegation {
    fn name(&self) -> &'static str {
        "simplify_double_negation"
    }

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        if let PreHirExpr::Unary {
            op: op1,
            expr: inner1,
            ty: _,
        } = expr
        {
            if let PreHirExpr::Unary {
                op: op2,
                expr: inner2,
                ty: _,
            } = inner1.as_mut()
            {
                if (*op1 == PreHirUnaryOp::Not && *op2 == PreHirUnaryOp::Not)
                    || (*op1 == PreHirUnaryOp::BitNot && *op2 == PreHirUnaryOp::BitNot)
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

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Binary { op, lhs, rhs, ty } => {
                match (lhs.as_ref(), rhs.as_ref()) {
                    // Fold double constants
                    (PreHirExpr::Const(c1, _), PreHirExpr::Const(c2, _)) => {
                        let val = match op {
                            PreHirBinaryOp::Add => Some(c1.wrapping_add(*c2)),
                            PreHirBinaryOp::Sub => Some(c1.wrapping_sub(*c2)),
                            PreHirBinaryOp::Mul => Some(c1.wrapping_mul(*c2)),
                            PreHirBinaryOp::Div if *c2 != 0 => Some(c1.wrapping_div(*c2)),
                            PreHirBinaryOp::Mod if *c2 != 0 => Some(c1.wrapping_rem(*c2)),
                            PreHirBinaryOp::And => Some(c1 & c2),
                            PreHirBinaryOp::Or => Some(c1 | c2),
                            PreHirBinaryOp::Xor => Some(c1 ^ c2),
                            PreHirBinaryOp::Shl => Some(c1.wrapping_shl(*c2 as u32)),
                            PreHirBinaryOp::Shr => {
                                Some((*c1 as u64).wrapping_shr(*c2 as u32) as i64)
                            }
                            PreHirBinaryOp::Sar => Some(c1.wrapping_shr(*c2 as u32)),
                            _ => None,
                        };
                        if let Some(v) = val {
                            *expr = PreHirExpr::Const(v, ty.clone());
                            return true;
                        }
                    }
                    // x + 0 -> x
                    (other, PreHirExpr::Const(0, _)) if *op == PreHirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(0, _), other) if *op == PreHirBinaryOp::Add => {
                        *expr = other.clone();
                        return true;
                    }
                    // x - 0 -> x
                    (other, PreHirExpr::Const(0, _)) if *op == PreHirBinaryOp::Sub => {
                        *expr = other.clone();
                        return true;
                    }
                    // x * 0 -> 0 (only if x is pure), x * 1 -> x
                    (other, PreHirExpr::Const(c, _)) if *op == PreHirBinaryOp::Mul => {
                        if *c == 0 && !expr_has_side_effects(other) {
                            *expr = PreHirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    (PreHirExpr::Const(c, _), other) if *op == PreHirBinaryOp::Mul => {
                        if *c == 0 && !expr_has_side_effects(other) {
                            *expr = PreHirExpr::Const(0, ty.clone());
                            return true;
                        } else if *c == 1 {
                            *expr = other.clone();
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Neg,
                expr: inner,
                ty,
            } => {
                if let PreHirExpr::Const(c, _) = inner.as_ref() {
                    *expr = PreHirExpr::Const(c.wrapping_neg(), ty.clone());
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

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        if let PreHirExpr::Binary { op, lhs, rhs, ty } = expr {
            match op {
                PreHirBinaryOp::And => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, PreHirExpr::Const(0, _)) if !expr_has_side_effects(other) => {
                        *expr = PreHirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (PreHirExpr::Const(0, _), other) if !expr_has_side_effects(other) => {
                        *expr = PreHirExpr::Const(0, ty.clone());
                        return true;
                    }
                    (other, PreHirExpr::Const(-1, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(-1, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                PreHirBinaryOp::Or => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, PreHirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(0, _), other) => {
                        *expr = other.clone();
                        return true;
                    }
                    _ => {}
                },
                PreHirBinaryOp::Xor => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, PreHirExpr::Const(0, _)) => {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(0, _), other) => {
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
/// - `b && true` -> `b` (for boolean `b`)
/// - `b || false` -> `b` (for boolean `b`)
struct RuleLogicalIdentities;

impl Rule for RuleLogicalIdentities {
    fn name(&self) -> &'static str {
        "logical_identities"
    }

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        self.apply_expr_with_types(expr, &HashMap::default())
    }

    fn apply_expr_with_types(
        &self,
        expr: &mut PreHirExpr,
        type_map: &HashMap<String, NirType>,
    ) -> bool {
        if let PreHirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: _,
        } = expr
        {
            match op {
                PreHirBinaryOp::LogicalAnd => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, PreHirExpr::Const(c, _))
                        if *c != 0 && is_bool_expr(other, type_map) =>
                    {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(c, _), other)
                        if *c != 0 && is_bool_expr(other, type_map) =>
                    {
                        *expr = other.clone();
                        return true;
                    }
                    (lhs_expr, PreHirExpr::Const(0, _)) if !expr_has_side_effects(lhs_expr) => {
                        *expr = PreHirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    (PreHirExpr::Const(0, _), _) => {
                        *expr = PreHirExpr::Const(0, NirType::Bool);
                        return true;
                    }
                    _ => {}
                },
                PreHirBinaryOp::LogicalOr => match (lhs.as_ref(), rhs.as_ref()) {
                    (other, PreHirExpr::Const(0, _)) if is_bool_expr(other, type_map) => {
                        *expr = other.clone();
                        return true;
                    }
                    (PreHirExpr::Const(0, _), other) if is_bool_expr(other, type_map) => {
                        *expr = other.clone();
                        return true;
                    }
                    (lhs_expr, PreHirExpr::Const(c, _))
                        if *c != 0 && !expr_has_side_effects(lhs_expr) =>
                    {
                        *expr = PreHirExpr::Const(1, NirType::Bool);
                        return true;
                    }
                    (PreHirExpr::Const(c, _), _) if *c != 0 => {
                        *expr = PreHirExpr::Const(1, NirType::Bool);
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

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        if let PreHirExpr::PtrOffset { base, offset: 0 } = expr {
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

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        if let PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ty,
        } = expr
        {
            match (lhs.as_ref(), rhs.as_ref()) {
                (other, PreHirExpr::Const(2, _)) => {
                    *expr = PreHirExpr::Binary {
                        op: PreHirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(PreHirExpr::Const(1, ty.clone())),
                        ty: ty.clone(),
                    };
                    return true;
                }
                (PreHirExpr::Const(2, _), other) => {
                    *expr = PreHirExpr::Binary {
                        op: PreHirBinaryOp::Shl,
                        lhs: Box::new(other.clone()),
                        rhs: Box::new(PreHirExpr::Const(1, ty.clone())),
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

    fn apply_expr(&self, expr: &mut PreHirExpr) -> bool {
        if let PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } = expr
        {
            // Rule 1: cond ? A : A -> A (only if cond is pure)
            if then_expr == else_expr && !expr_has_side_effects(cond) {
                *expr = (**then_expr).clone();
                return true;
            }
            // Rule 2: true ? A : B -> A, false ? A : B -> B
            if let PreHirExpr::Const(val, _) = cond.as_ref() {
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
pub fn apply_rule_normalization(func: &mut PreHirFunction) -> bool {
    let mut type_map = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

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

        loop_changed |= apply_rules_to_stmts(&mut func.body, &rules, &type_map);
        changed |= loop_changed;
    }

    changed
}

fn apply_rules_to_stmts(
    stmts: &mut [PreHirStmt],
    rules: &[Box<dyn Rule>],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= apply_rules_to_stmt(stmt, rules, type_map);
    }
    changed
}

fn apply_rules_to_stmt(
    stmt: &mut PreHirStmt,
    rules: &[Box<dyn Rule>],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    // Apply rules directly to the statement
    for rule in rules {
        changed |= rule.apply_stmt(stmt);
    }

    // Recurse and apply to inner expressions/statements
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= apply_rules_to_lvalue(lhs, rules, type_map);
            changed |= apply_rules_to_expr(rhs, rules, type_map);
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::Return(Some(expr))
        | PreHirStmt::VaStart { va_list: expr, .. } => {
            changed |= apply_rules_to_expr(expr, rules, type_map);
        }
        PreHirStmt::Block(body) => {
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rules,
                type_map,
            );
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            changed |= apply_rules_to_expr(cond, rules, type_map);
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rules,
                type_map,
            );
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= apply_rules_to_stmt(init, rules, type_map);
            }
            if let Some(cond) = cond {
                changed |= apply_rules_to_expr(cond, rules, type_map);
            }
            if let Some(update) = update {
                changed |= apply_rules_to_stmt(update, rules, type_map);
            }
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rules,
                type_map,
            );
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= apply_rules_to_expr(cond, rules, type_map);
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                rules,
                type_map,
            );
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                rules,
                type_map,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= apply_rules_to_expr(expr, rules, type_map);
            for case in cases {
                changed |= apply_rules_to_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    rules,
                    type_map,
                );
            }
            changed |= apply_rules_to_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                rules,
                type_map,
            );
        }
        _ => {}
    }

    changed
}

fn apply_rules_to_lvalue(
    lhs: &mut PreHirLValue,
    rules: &[Box<dyn Rule>],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match lhs {
        PreHirLValue::Deref { ptr, .. } | PreHirLValue::FieldAccess { base: ptr, .. } => {
            changed |= apply_rules_to_expr(ptr, rules, type_map);
        }
        PreHirLValue::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules, type_map);
            changed |= apply_rules_to_expr(index, rules, type_map);
        }
        PreHirLValue::Var(_) => {}
    }
    changed
}

fn apply_rules_to_expr(
    expr: &mut PreHirExpr,
    rules: &[Box<dyn Rule>],
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    // First apply rules to sub-expressions recursively
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. } => {
            changed |= apply_rules_to_expr(inner, rules, type_map);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= apply_rules_to_expr(lhs, rules, type_map);
            changed |= apply_rules_to_expr(rhs, rules, type_map);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= apply_rules_to_expr(cond, rules, type_map);
            changed |= apply_rules_to_expr(then_expr, rules, type_map);
            changed |= apply_rules_to_expr(else_expr, rules, type_map);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= apply_rules_to_expr(arg, rules, type_map);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= apply_rules_to_expr(base, rules, type_map);
            changed |= apply_rules_to_expr(index, rules, type_map);
        }
        _ => {}
    }

    // Apply rules directly to this expression
    for rule in rules {
        changed |= rule.apply_expr_with_types(expr, type_map);
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    #[test]
    fn test_rule_simplify_select() {
        let cond = PreHirExpr::Var("cond".to_string());
        let val = PreHirExpr::Var("val".to_string());
        let mut select_expr = PreHirExpr::Select {
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

    #[test]
    fn does_not_drop_side_effects_in_zero_absorbing_or_select_rules() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let side_effect = PreHirExpr::Call {
            origin: None,
            target: "tick".to_string(),
            args: vec![],
            ty: u32_ty.clone(),
        };
        let mut func = PreHirFunction {
            name: "side_effects".to_string(),
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Mul,
                    lhs: Box::new(side_effect.clone()),
                    rhs: Box::new(PreHirExpr::Const(0, u32_ty.clone())),
                    ty: u32_ty.clone(),
                }),
                PreHirStmt::Expr(PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(side_effect.clone()),
                    rhs: Box::new(PreHirExpr::Const(0, u32_ty.clone())),
                    ty: u32_ty.clone(),
                }),
                PreHirStmt::Expr(PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalAnd,
                    lhs: Box::new(side_effect.clone()),
                    rhs: Box::new(PreHirExpr::Const(0, NirType::Bool)),
                    ty: NirType::Bool,
                }),
                PreHirStmt::Expr(PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalOr,
                    lhs: Box::new(side_effect.clone()),
                    rhs: Box::new(PreHirExpr::Const(1, NirType::Bool)),
                    ty: NirType::Bool,
                }),
                PreHirStmt::Expr(PreHirExpr::Select {
                    cond: Box::new(side_effect),
                    then_expr: Box::new(PreHirExpr::Const(5, u32_ty.clone())),
                    else_expr: Box::new(PreHirExpr::Const(5, u32_ty.clone())),
                    ty: u32_ty,
                }),
            ],
            ..Default::default()
        };

        assert!(!apply_rule_normalization(&mut func));
        assert!(
            func.body
                .iter()
                .all(|s| !matches!(s, PreHirStmt::Expr(PreHirExpr::Const(_, _))))
        );
    }

    #[test]
    fn does_not_reduce_non_bool_logical_identities() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let binding = |name: &str, ty: NirType| PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        let mut func = PreHirFunction {
            name: "logical_bool_only".to_string(),
            locals: vec![binding("x", u32_ty.clone()), binding("b", NirType::Bool)],
            body: vec![
                PreHirStmt::Return(Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalAnd,
                    lhs: Box::new(PreHirExpr::Var("x".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, NirType::Bool)),
                    ty: NirType::Bool,
                })),
                PreHirStmt::Return(Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalOr,
                    lhs: Box::new(PreHirExpr::Var("x".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0, NirType::Bool)),
                    ty: NirType::Bool,
                })),
                PreHirStmt::Return(Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalAnd,
                    lhs: Box::new(PreHirExpr::Var("b".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, NirType::Bool)),
                    ty: NirType::Bool,
                })),
            ],
            ..Default::default()
        };

        assert!(apply_rule_normalization(&mut func));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                ..
            }))
        ));
        assert!(matches!(
            &func.body[1],
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                ..
            }))
        ));
        assert_eq!(
            func.body[2],
            PreHirStmt::Return(Some(PreHirExpr::Var("b".to_string())))
        );
    }

    #[test]
    fn traverses_loop_conditions_for_headers_vastart_and_field_access() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let add_zero = |name: &str| PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var(name.to_string())),
            rhs: Box::new(PreHirExpr::Const(0, u32_ty.clone())),
            ty: u32_ty.clone(),
        };
        let mut func = PreHirFunction {
            name: "traversal".to_string(),
            body: vec![
                PreHirStmt::While {
                    cond: add_zero("w"),
                    body: vec![].into(),
                },
                PreHirStmt::DoWhile {
                    body: vec![].into(),
                    cond: add_zero("dw"),
                },
                PreHirStmt::For {
                    init: Some(Box::new(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: add_zero("init_v"),
                    })),
                    cond: Some(add_zero("fc")),
                    update: Some(Box::new(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: add_zero("upd_v"),
                    })),
                    body: vec![].into(),
                },
                PreHirStmt::VaStart {
                    va_list: PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("ap".to_string())),
                        offset: 0,
                    },
                    last_named_param: "n".to_string(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::FieldAccess {
                        base: Box::new(PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::Var("obj".to_string())),
                            offset: 0,
                        }),
                        field_name: "f".to_string(),
                        offset: 0,
                        ty: u32_ty.clone(),
                    },
                    rhs: PreHirExpr::FieldAccess {
                        base: Box::new(PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::Var("src".to_string())),
                            offset: 0,
                        }),
                        field_name: "g".to_string(),
                        offset: 0,
                        ty: u32_ty,
                    },
                },
            ],
            ..Default::default()
        };

        assert!(apply_rule_normalization(&mut func));
        assert!(matches!(
            &func.body[0],
            PreHirStmt::While {
                cond: PreHirExpr::Var(n),
                ..
            } if n == "w"
        ));
        assert!(matches!(
            &func.body[1],
            PreHirStmt::DoWhile {
                cond: PreHirExpr::Var(n),
                ..
            } if n == "dw"
        ));
        assert!(matches!(
            &func.body[2],
            PreHirStmt::For {
                init: Some(init),
                cond: Some(PreHirExpr::Var(c)),
                update: Some(upd),
                ..
            } if c == "fc"
                && matches!(init.as_ref(), PreHirStmt::Assign { rhs: PreHirExpr::Var(iv), .. } if iv == "init_v")
                && matches!(upd.as_ref(), PreHirStmt::Assign { rhs: PreHirExpr::Var(uv), .. } if uv == "upd_v")
        ));
        assert!(matches!(
            &func.body[3],
            PreHirStmt::VaStart {
                va_list: PreHirExpr::Var(n),
                ..
            } if n == "ap"
        ));
        assert!(matches!(
            &func.body[4],
            PreHirStmt::Assign {
                lhs: PreHirLValue::FieldAccess { base: lhs_base, .. },
                rhs: PreHirExpr::FieldAccess { base: rhs_base, .. },
            } if matches!(lhs_base.as_ref(), PreHirExpr::Var(l) if l == "obj")
                && matches!(rhs_base.as_ref(), PreHirExpr::Var(r) if r == "src")
        ));
    }
}
