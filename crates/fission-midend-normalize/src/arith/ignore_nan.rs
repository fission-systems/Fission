use crate::HashSet;
use crate::prelude::*;

/// Simplifies floating-point comparison expressions by ignoring redundant NaN checks (RuleIgnoreNan).
/// - `!__isnan(x) && (x < y)` -> `x < y`
/// - `__isnan(x) || (x != y)` -> `x != y`
pub fn apply_ignore_nan_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= visit_stmt(stmt);
    }
    changed
}

fn get_var_name(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name),
        PreHirExpr::Cast { expr: inner, .. } => get_var_name(inner),
        _ => None,
    }
}

fn get_isnan_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Call { target, args, .. } if target == "__isnan" => {
            if args.len() == 1 {
                get_var_name(&args[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_negated_isnan_var(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: inner,
            ..
        } => get_isnan_var(inner),
        _ => None,
    }
}

fn contains_comparison_involving(expr: &PreHirExpr, var_name: &str) -> bool {
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(
                op,
                PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::Gt
                    | PreHirBinaryOp::Ge
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::SGt
                    | PreHirBinaryOp::SGe
            ) {
                if get_var_name(lhs) == Some(var_name) || get_var_name(rhs) == Some(var_name) {
                    return true;
                }
            }
            if matches!(op, PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::LogicalOr) {
                return contains_comparison_involving(lhs, var_name)
                    || contains_comparison_involving(rhs, var_name);
            }
        }
        _ => {}
    }
    false
}

fn collect_and_operands(expr: &PreHirExpr, operands: &mut Vec<PreHirExpr>) {
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalAnd,
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

fn collect_or_operands(expr: &PreHirExpr, operands: &mut Vec<PreHirExpr>) {
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalOr,
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

fn rebuild_and_tree(mut operands: Vec<PreHirExpr>) -> PreHirExpr {
    if operands.is_empty() {
        return PreHirExpr::Const(1, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn rebuild_or_tree(mut operands: Vec<PreHirExpr>) -> PreHirExpr {
    if operands.is_empty() {
        return PreHirExpr::Const(0, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn visit_expr(expr: &mut PreHirExpr) -> bool {
    let mut changed = false;

    // Recurse first
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            changed |= visit_expr(inner);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs);
            changed |= visit_expr(rhs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= visit_expr(cond);
            changed |= visit_expr(then_expr);
            changed |= visit_expr(else_expr);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base);
            changed |= visit_expr(index);
        }
        _ => {}
    }

    // Now try optimizing the current expression
    if let PreHirExpr::Binary { op, .. } = expr {
        if *op == PreHirBinaryOp::LogicalAnd {
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
        } else if *op == PreHirBinaryOp::LogicalOr {
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

fn visit_stmt(stmt: &mut PreHirStmt) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= visit_expr(rhs);
            match lhs {
                PreHirLValue::Deref { ptr, .. } => {
                    changed |= visit_expr(ptr);
                }
                PreHirLValue::Index { base, index, .. } => {
                    changed |= visit_expr(base);
                    changed |= visit_expr(index);
                }
                _ => {}
            }
        }
        PreHirStmt::Expr(expr) => {
            changed |= visit_expr(expr);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            changed |= visit_expr(va_list);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                changed |= visit_stmt(s);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= visit_expr(cond);
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body) {
                changed |= visit_stmt(s);
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body) {
                changed |= visit_stmt(s);
            }
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= visit_expr(expr);
            for case in cases {
                for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body) {
                    changed |= visit_stmt(s);
                }
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default) {
                changed |= visit_stmt(s);
            }
        }
        PreHirStmt::Return(Some(expr)) => {
            changed |= visit_expr(expr);
        }
        _ => {}
    }
    changed
}
