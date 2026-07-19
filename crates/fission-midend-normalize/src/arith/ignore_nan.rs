use crate::prelude::*;
use crate::HashSet;

/// Simplifies floating-point comparison expressions by ignoring redundant NaN checks (RuleIgnoreNan).
/// - `!__isnan(x) && (x < y)` -> `x < y`
/// - `__isnan(x) || (x != y)` -> `x != y`
pub fn apply_ignore_nan_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= visit_stmt(stmt);
    }
    changed
}

fn get_var_name(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name),
        HirExpr::Cast { expr: inner, .. } => get_var_name(inner),
        _ => None,
    }
}

fn get_isnan_var(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Call { target, args, .. } if target == "__isnan" => {
            if args.len() == 1 {
                get_var_name(&args[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_negated_isnan_var(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: inner,
            ..
        } => get_isnan_var(inner),
        _ => None,
    }
}

fn contains_comparison_involving(expr: &HirExpr, var_name: &str) -> bool {
    match expr {
        HirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(
                op,
                HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::Gt
                    | HirBinaryOp::Ge
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
                    | HirBinaryOp::SGt
                    | HirBinaryOp::SGe
            ) {
                if get_var_name(lhs) == Some(var_name) || get_var_name(rhs) == Some(var_name) {
                    return true;
                }
            }
            if matches!(op, HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr) {
                return contains_comparison_involving(lhs, var_name)
                    || contains_comparison_involving(rhs, var_name);
            }
        }
        _ => {}
    }
    false
}

fn collect_and_operands(expr: &HirExpr, operands: &mut Vec<HirExpr>) {
    if let HirExpr::Binary {
        op: HirBinaryOp::LogicalAnd,
        lhs,
        rhs,
        ..
    } = expr
    {
        collect_and_operands(lhs, operands);
        collect_and_operands(rhs, operands);
    } else {
        operands.push(expr.clone());
    }
}

fn collect_or_operands(expr: &HirExpr, operands: &mut Vec<HirExpr>) {
    if let HirExpr::Binary {
        op: HirBinaryOp::LogicalOr,
        lhs,
        rhs,
        ..
    } = expr
    {
        collect_or_operands(lhs, operands);
        collect_or_operands(rhs, operands);
    } else {
        operands.push(expr.clone());
    }
}

fn rebuild_and_tree(mut operands: Vec<HirExpr>) -> HirExpr {
    if operands.is_empty() {
        return HirExpr::Const(1, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn rebuild_or_tree(mut operands: Vec<HirExpr>) -> HirExpr {
    if operands.is_empty() {
        return HirExpr::Const(0, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = HirExpr::Binary {
            op: HirBinaryOp::LogicalOr,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn visit_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;

    // Recurse first
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            changed |= visit_expr(inner);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs);
            changed |= visit_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= visit_expr(cond);
            changed |= visit_expr(then_expr);
            changed |= visit_expr(else_expr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base);
            changed |= visit_expr(index);
        }
        _ => {}
    }

    // Now try optimizing the current expression
    if let HirExpr::Binary { op, .. } = expr {
        if *op == HirBinaryOp::LogicalAnd {
            let mut operands = Vec::new();
            collect_and_operands(expr, &mut operands);

            let mut to_remove = HashSet::default();
            for (idx, operand) in operands.iter().enumerate() {
                if let Some(var_name) = get_negated_isnan_var(operand) {
                    // Check if any OTHER operand contains a comparison involving var_name
                    let has_comparison = operands.iter().enumerate().any(|(o_idx, o_operand)| {
                        o_idx != idx && contains_comparison_involving(o_operand, var_name)
                    });
                    if has_comparison {
                        to_remove.insert(idx);
                    }
                }
            }

            if !to_remove.is_empty() {
                let mut new_operands = Vec::new();
                for (idx, operand) in operands.into_iter().enumerate() {
                    if !to_remove.contains(&idx) {
                        new_operands.push(operand);
                    }
                }
                *expr = rebuild_and_tree(new_operands);
                return true;
            }
        } else if *op == HirBinaryOp::LogicalOr {
            let mut operands = Vec::new();
            collect_or_operands(expr, &mut operands);

            let mut to_remove = HashSet::default();
            for (idx, operand) in operands.iter().enumerate() {
                if let Some(var_name) = get_isnan_var(operand) {
                    // Check if any OTHER operand contains a comparison involving var_name
                    let has_comparison = operands.iter().enumerate().any(|(o_idx, o_operand)| {
                        o_idx != idx && contains_comparison_involving(o_operand, var_name)
                    });
                    if has_comparison {
                        to_remove.insert(idx);
                    }
                }
            }

            if !to_remove.is_empty() {
                let mut new_operands = Vec::new();
                for (idx, operand) in operands.into_iter().enumerate() {
                    if !to_remove.contains(&idx) {
                        new_operands.push(operand);
                    }
                }
                *expr = rebuild_or_tree(new_operands);
                return true;
            }
        }
    }

    changed
}

fn visit_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= visit_expr(rhs);
            match lhs {
                HirLValue::Deref { ptr, .. } => {
                    changed |= visit_expr(ptr);
                }
                HirLValue::Index { base, index, .. } => {
                    changed |= visit_expr(base);
                    changed |= visit_expr(index);
                }
                _ => {}
            }
        }
        HirStmt::Expr(expr) => {
            changed |= visit_expr(expr);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= visit_expr(va_list);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for s in body {
                changed |= visit_stmt(s);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= visit_expr(cond);
            for s in then_body {
                changed |= visit_stmt(s);
            }
            for s in else_body {
                changed |= visit_stmt(s);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= visit_expr(expr);
            for case in cases {
                for s in &mut case.body {
                    changed |= visit_stmt(s);
                }
            }
            for s in default {
                changed |= visit_stmt(s);
            }
        }
        HirStmt::Return(Some(expr)) => {
            changed |= visit_expr(expr);
        }
        _ => {}
    }
    changed
}
