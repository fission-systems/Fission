use crate::prelude::*;
use crate::HashSet;

/// Simplifies floating-point comparison expressions by ignoring redundant NaN checks (RuleIgnoreNan).
/// - `!__isnan(x) && (x < y)` -> `x < y`
/// - `__isnan(x) || (x != y)` -> `x != y`
pub fn apply_ignore_nan_pass(func: &mut DirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= visit_stmt(stmt);
    }
    changed
}

fn get_var_name(expr: &DirExpr) -> Option<&str> {
    match expr {
        DirExpr::Var(name) => Some(name),
        DirExpr::Cast { expr: inner, .. } => get_var_name(inner),
        _ => None,
    }
}

fn get_isnan_var(expr: &DirExpr) -> Option<&str> {
    match expr {
        DirExpr::Call { target, args, .. } if target == "__isnan" => {
            if args.len() == 1 {
                get_var_name(&args[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_negated_isnan_var(expr: &DirExpr) -> Option<&str> {
    match expr {
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: inner,
            ..
        } => get_isnan_var(inner),
        _ => None,
    }
}

fn contains_comparison_involving(expr: &DirExpr, var_name: &str) -> bool {
    match expr {
        DirExpr::Binary { op, lhs, rhs, .. } => {
            if matches!(
                op,
                DirBinaryOp::Eq
                    | DirBinaryOp::Ne
                    | DirBinaryOp::Lt
                    | DirBinaryOp::Le
                    | DirBinaryOp::Gt
                    | DirBinaryOp::Ge
                    | DirBinaryOp::SLt
                    | DirBinaryOp::SLe
                    | DirBinaryOp::SGt
                    | DirBinaryOp::SGe
            ) {
                if get_var_name(lhs) == Some(var_name) || get_var_name(rhs) == Some(var_name) {
                    return true;
                }
            }
            if matches!(op, DirBinaryOp::LogicalAnd | DirBinaryOp::LogicalOr) {
                return contains_comparison_involving(lhs, var_name)
                    || contains_comparison_involving(rhs, var_name);
            }
        }
        _ => {}
    }
    false
}

fn collect_and_operands(expr: &DirExpr, operands: &mut Vec<DirExpr>) {
    if let DirExpr::Binary {
        op: DirBinaryOp::LogicalAnd,
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

fn collect_or_operands(expr: &DirExpr, operands: &mut Vec<DirExpr>) {
    if let DirExpr::Binary {
        op: DirBinaryOp::LogicalOr,
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

fn rebuild_and_tree(mut operands: Vec<DirExpr>) -> DirExpr {
    if operands.is_empty() {
        return DirExpr::Const(1, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn rebuild_or_tree(mut operands: Vec<DirExpr>) -> DirExpr {
    if operands.is_empty() {
        return DirExpr::Const(0, NirType::Bool);
    }
    let mut expr = operands.remove(0);
    for op in operands {
        expr = DirExpr::Binary {
            op: DirBinaryOp::LogicalOr,
            lhs: Box::new(expr),
            rhs: Box::new(op),
            ty: NirType::Bool,
        };
    }
    expr
}

fn visit_expr(expr: &mut DirExpr) -> bool {
    let mut changed = false;

    // Recurse first
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. } => {
            changed |= visit_expr(inner);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= visit_expr(lhs);
            changed |= visit_expr(rhs);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= visit_expr(arg);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= visit_expr(cond);
            changed |= visit_expr(then_expr);
            changed |= visit_expr(else_expr);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= visit_expr(base);
            changed |= visit_expr(index);
        }
        _ => {}
    }

    // Now try optimizing the current expression
    if let DirExpr::Binary { op, .. } = expr {
        if *op == DirBinaryOp::LogicalAnd {
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
        } else if *op == DirBinaryOp::LogicalOr {
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

fn visit_stmt(stmt: &mut DirStmt) -> bool {
    let mut changed = false;
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            changed |= visit_expr(rhs);
            match lhs {
                DirLValue::Deref { ptr, .. } => {
                    changed |= visit_expr(ptr);
                }
                DirLValue::Index { base, index, .. } => {
                    changed |= visit_expr(base);
                    changed |= visit_expr(index);
                }
                _ => {}
            }
        }
        DirStmt::Expr(expr) => {
            changed |= visit_expr(expr);
        }
        DirStmt::VaStart { va_list, .. } => {
            changed |= visit_expr(va_list);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for s in body {
                changed |= visit_stmt(s);
            }
        }
        DirStmt::If {
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
        DirStmt::Switch {
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
        DirStmt::Return(Some(expr)) => {
            changed |= visit_expr(expr);
        }
        _ => {}
    }
    changed
}
