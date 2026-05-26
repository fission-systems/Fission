use super::super::wave_stats::add_call_artifact_removals;
use super::super::*;
use std::collections::HashSet;

fn is_weak_call_target(target: &str) -> bool {
    target.starts_with("sub_") || target.starts_with("FUN_")
}

fn count_mentions_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => count_mentions_in_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_mentions_in_expr(lhs, name) + count_mentions_in_expr(rhs, name)
        }
        HirExpr::Call { args, .. } => args
            .iter()
            .map(|arg| count_mentions_in_expr(arg, name))
            .sum(),
        HirExpr::PtrOffset { base, .. } => count_mentions_in_expr(base, name),
        HirExpr::Index { base, index, .. } => {
            count_mentions_in_expr(base, name) + count_mentions_in_expr(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_mentions_in_expr(cond, name)
                + count_mentions_in_expr(then_expr, name)
                + count_mentions_in_expr(else_expr, name)
        }
        HirExpr::Const(_, _) => 0,
    }
}

fn count_mentions_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let lhs_mentions = match lhs {
                HirLValue::Var(var) => usize::from(var == name),
                HirLValue::Deref { ptr, .. } => count_mentions_in_expr(ptr, name),
                HirLValue::Index { base, index, .. } => {
                    count_mentions_in_expr(base, name) + count_mentions_in_expr(index, name)
                }
                HirLValue::FieldAccess { base, .. } => count_mentions_in_expr(base, name),
            };
            lhs_mentions + count_mentions_in_expr(rhs, name)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => count_mentions_in_expr(expr, name),
        HirStmt::VaStart { va_list, .. } => count_mentions_in_expr(va_list, name),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => body
            .iter()
            .map(|stmt| count_mentions_in_stmt(stmt, name))
            .sum(),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_mentions_in_expr(cond, name)
                + then_body
                    .iter()
                    .map(|stmt| count_mentions_in_stmt(stmt, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_mentions_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_mentions_in_expr(expr, name)
                + cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_mentions_in_stmt(stmt, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_mentions_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => 0,
    }
}

fn substitute_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(var) if var == name => {
            *expr = replacement.clone();
            true
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => {
            substitute_var_in_expr(expr, name, replacement)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_var_in_expr(lhs, name, replacement)
                | substitute_var_in_expr(rhs, name, replacement)
        }
        HirExpr::Call { args, .. } => args
            .iter_mut()
            .any(|arg| substitute_var_in_expr(arg, name, replacement)),
        HirExpr::PtrOffset { base, .. } => substitute_var_in_expr(base, name, replacement),
        HirExpr::Index { base, index, .. } => {
            substitute_var_in_expr(base, name, replacement)
                | substitute_var_in_expr(index, name, replacement)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_var_in_expr(cond, name, replacement)
                | substitute_var_in_expr(then_expr, name, replacement)
                | substitute_var_in_expr(else_expr, name, replacement)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn substitute_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) -> bool {
    match stmt {
        HirStmt::Assign { rhs, .. } => substitute_var_in_expr(rhs, name, replacement),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            substitute_var_in_expr(expr, name, replacement)
        }
        HirStmt::VaStart { va_list, .. } => substitute_var_in_expr(va_list, name, replacement),
        _ => false,
    }
}

fn remove_inlineable_call_artifacts(
    stmts: &mut Vec<HirStmt>,
    temp_names: &HashSet<String>,
) -> usize {
    let mut removed = 0usize;
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let Some((temp_name, call_expr)) = (match &stmts[idx] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Call { target, .. },
            } if is_weak_call_target(target) && temp_names.contains(name) => Some((
                name.clone(),
                match &stmts[idx] {
                    HirStmt::Assign { rhs, .. } => rhs.clone(),
                    _ => unreachable!(),
                },
            )),
            _ => None,
        }) else {
            idx += 1;
            continue;
        };
        if count_mentions_in_stmt(&stmts[idx + 1], &temp_name) != 1 {
            idx += 1;
            continue;
        }
        if !substitute_var_in_stmt(&mut stmts[idx + 1], &temp_name, &call_expr) {
            idx += 1;
            continue;
        }
        stmts.remove(idx);
        removed += 1;
    }
    let mut idx = 0usize;
    while idx < stmts.len() {
        let Some((temp_name, call_expr)) = (match &stmts[idx] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Call { target, .. },
            } if is_weak_call_target(target) && temp_names.contains(name) => Some((
                name.clone(),
                match &stmts[idx] {
                    HirStmt::Assign { rhs, .. } => rhs.clone(),
                    _ => unreachable!(),
                },
            )),
            _ => None,
        }) else {
            idx += 1;
            continue;
        };
        let mentions_after = stmts[idx + 1..]
            .iter()
            .map(|stmt| count_mentions_in_stmt(stmt, &temp_name))
            .sum::<usize>();
        if mentions_after == 0 {
            stmts[idx] = HirStmt::Expr(call_expr);
            removed += 1;
        }
        idx += 1;
    }
    removed
}

pub(crate) fn apply_call_artifact_cleanup_pass(func: &mut HirFunction) -> bool {
    let temp_names = func
        .locals
        .iter()
        .filter_map(|binding| binding.is_temp_like().then(|| binding.name.clone()))
        .collect::<HashSet<_>>();
    let removed = remove_inlineable_call_artifacts(&mut func.body, &temp_names);
    add_call_artifact_removals(removed);
    removed > 0
}
