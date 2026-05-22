use crate::nir::{HirBinaryOp, HirExpr, HirLValue, HirStmt, NirBinding, NirType};
use std::collections::{HashMap, HashSet};

pub(super) fn is_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

pub(crate) fn expr_has_side_effects(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_has_side_effects(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        HirExpr::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effects(cond)
                || expr_has_side_effects(then_expr)
                || expr_has_side_effects(else_expr)
        }
        HirExpr::Call { target, args, .. } => {
            if is_pure_intrinsic_call(target) {
                args.iter().any(expr_has_side_effects)
            } else {
                true
            }
        }
    }
}

pub(super) fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

pub(super) fn is_low_cost_flag_intrinsic(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow")
}

pub(super) fn is_dead_local_clobber_name(name: &str) -> bool {
    if name.starts_with("param_ffff")
        || name.starts_with("param_fff")
        || name.starts_with("param_ff")
    {
        return true;
    }
    let Some(hex) = name.strip_prefix("local_") else {
        return false;
    };
    u64::from_str_radix(hex, 16)
        .map(|offset| offset <= 0x0c)
        .unwrap_or(false)
}

pub(super) fn retain_unmarked_stmts(stmts: &mut Vec<HirStmt>, to_remove: &[bool]) {
    let mut idx = 0usize;
    stmts.retain(|_| {
        let keep = !to_remove.get(idx).copied().unwrap_or(false);
        idx += 1;
        keep
    });
}

pub(super) fn count_uses_in_stmt_list(stmts: &[HirStmt], name: &str) -> usize {
    stmts
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, name))
        .sum()
}

pub(super) fn count_uses_in_bindings(bindings: &[NirBinding], name: &str) -> usize {
    bindings
        .iter()
        .filter(|binding| binding.name != name)
        .filter_map(|binding| binding.initializer.as_ref())
        .map(|expr| count_var_uses(expr, name))
        .sum()
}

pub(super) fn count_var_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        HirStmt::Expr(expr) => count_var_uses(expr, name),
        HirStmt::Block(stmts) => stmts
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses(expr, name)
                + cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_var_uses_in_stmt(stmt, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses(cond, name)
                + then_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::While { cond, body } => {
            count_var_uses(cond, name)
                + body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut total = 0;
            if let Some(i) = init {
                total += count_var_uses_in_stmt(i, name);
            }
            if let Some(c) = cond {
                total += count_var_uses(c, name);
            }
            if let Some(u) = update {
                total += count_var_uses_in_stmt(u, name);
            }
            total += body
                .iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>();
            total
        }
        HirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        HirStmt::VaStart { va_list, .. } => count_var_uses(va_list, name),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => 0,
    }
}

pub(super) fn count_var_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(_) => 0,
        HirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
    }
}

pub(super) fn count_var_uses(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. } => count_var_uses(expr, name),
        HirExpr::Unary { expr, .. } => count_var_uses(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        HirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        HirExpr::Load { ptr, .. } => count_var_uses(ptr, name),
        HirExpr::PtrOffset { base, .. } => count_var_uses(base, name),
        HirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        HirExpr::AggregateCopy { src, .. } => count_var_uses(src, name),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_uses(cond, name)
                + count_var_uses(then_expr, name)
                + count_var_uses(else_expr, name)
        }
    }
}

pub(super) fn stmt_mentions_var(stmt: &HirStmt, name: &str) -> bool {
    count_var_uses_in_stmt(stmt, name) > 0
}

pub(super) fn stmt_assigns_var(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_var(s, name)),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_var(s, name))
                || else_body.iter().any(|s| stmt_assigns_var(s, name))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| c.body.iter().any(|s| stmt_assigns_var(s, name)))
                || default.iter().any(|s| stmt_assigns_var(s, name))
        }
        _ => false,
    }
}

pub(super) fn stmt_may_bypass_following_stmts(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => true,
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => body.iter().any(stmt_may_bypass_following_stmts),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_may_bypass_following_stmts)
                || else_body.iter().any(stmt_may_bypass_following_stmts)
        }
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| c.body.iter().any(stmt_may_bypass_following_stmts))
                || default.iter().any(stmt_may_bypass_following_stmts)
        }
        _ => false,
    }
}

pub(super) fn stmt_assigns_any_expr_var(stmt: &HirStmt, expr: &HirExpr) -> bool {
    match stmt {
        HirStmt::Assign { lhs, .. } => lvalue_assigns_any_expr_var(lhs, expr),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
                || else_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| c.body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)))
                || default.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        _ => false,
    }
}

pub(super) fn lvalue_assigns_any_expr_var(lhs: &HirLValue, expr: &HirExpr) -> bool {
    match lhs {
        HirLValue::Var(name) => expr_contains_var(expr, name),
        HirLValue::Deref { .. } | HirLValue::Index { .. } => false,
    }
}

pub(super) fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => var == name,
        HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, name)
                || expr_contains_var(then_expr, name)
                || expr_contains_var(else_expr, name)
        }
    }
}

pub(super) fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        HirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for stmt in &mut case.body {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            for stmt in default {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in then_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            for stmt in else_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init_stmt) = init {
                replace_var_in_stmt(init_stmt, name, replacement);
            }
            if let Some(c) = cond {
                replace_var_in_expr(c, name, replacement);
            }
            if let Some(upd_stmt) = update {
                replace_var_in_stmt(upd_stmt, name, replacement);
            }
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

pub(super) fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
    }
}

pub(super) fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
    match expr {
        HirExpr::Var(var) if var == name => *expr = replacement.clone(),
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Unary { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        HirExpr::Load { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirExpr::PtrOffset { base, .. } => replace_var_in_expr(base, name, replacement),
        HirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        HirExpr::AggregateCopy { src, .. } => replace_var_in_expr(src, name, replacement),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            replace_var_in_expr(cond, name, replacement);
            replace_var_in_expr(then_expr, name, replacement);
            replace_var_in_expr(else_expr, name, replacement);
        }
    }
}

pub(super) fn expr_mentions_var(expr: &HirExpr, name: &str) -> bool {
    count_var_uses(expr, name) > 0
}

pub(super) fn var_is_assigned_in_stmts(stmts: &[HirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| var_is_assigned_in_stmt(stmt, name))
}

pub(super) fn var_is_assigned_in_stmt(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => var_is_assigned_in_stmts(body, name),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => var_is_assigned_in_stmts(then_body, name) || var_is_assigned_in_stmts(else_body, name),
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| var_is_assigned_in_stmts(&case.body, name))
                || var_is_assigned_in_stmts(default, name)
        }
    }
}

pub(super) fn collect_referenced_labels(stmts: &[HirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in stmts {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub(super) fn collect_referenced_label_counts(stmts: &[HirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in stmts {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

pub(super) fn collect_stmt_referenced_labels(stmt: &HirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
            for stmt in else_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

pub(super) fn collect_stmt_referenced_label_counts(stmt: &HirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
            for stmt in else_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}
