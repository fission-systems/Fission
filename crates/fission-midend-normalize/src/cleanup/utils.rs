use fission_midend_core::ir::NirType;
use fission_midend_prehir::{PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirLValue, PreHirStmt};
use fission_midend_core::is_pure_intrinsic_call;
use crate::{HashMap, HashSet};

pub(super) fn is_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name == "reg"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

/// ABI primary-return register surface names used by materialize HW binding.
pub(super) fn is_abi_return_register_name(name: &str) -> bool {
    matches!(
        name,
        "al" | "ax" | "eax" | "rax" | "r0" | "x0" | "v0" | "a0" | "w0"
    )
}

/// RHS safe to fold through `reg = rhs; return reg` → `return rhs`.
pub(super) fn is_pure_return_collapse_rhs(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Const(_, _) | PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) => true,
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            is_pure_return_collapse_rhs(expr)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            is_pure_return_collapse_rhs(lhs) && is_pure_return_collapse_rhs(rhs)
        }
        _ => false,
    }
}

pub fn expr_has_side_effects(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_has_side_effects(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effects(cond)
                || expr_has_side_effects(then_expr)
                || expr_has_side_effects(else_expr)
        }
        PreHirExpr::Call { target, args, .. } => {
            if is_pure_intrinsic_call(target) {
                args.iter().any(expr_has_side_effects)
            } else {
                true
            }
        }
    }
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
    // Accept any stack-slot name of the form `local_<hex>` as a dead-store
    // candidate.  The safety guards in the callers (Ptr, Aggregate,
    // side-effecting RHS, `slot_` prefix, and param name membership) are
    // sufficient to protect memory-observable slots — an arbitrary hex offset
    // cut-off is not needed and incorrectly excluded large offsets such as
    // local_10 / local_20 / local_28 on AArch64 or larger x86 frames.
    u64::from_str_radix(hex, 16).is_ok()
}

pub(super) fn retain_unmarked_stmts(stmts: &mut Vec<PreHirStmt>, to_remove: &[bool]) {
    let mut idx = 0usize;
    stmts.retain(|_| {
        let keep = !to_remove.get(idx).copied().unwrap_or(false);
        idx += 1;
        keep
    });
}

pub(super) fn count_uses_in_stmt_list(stmts: &[PreHirStmt], name: &str) -> usize {
    stmts
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, name))
        .sum()
}

pub(super) fn count_uses_in_bindings(bindings: &[PreHirBinding], name: &str) -> usize {
    bindings
        .iter()
        .filter(|binding| binding.name != name)
        .filter_map(|binding| binding.initializer.as_ref())
        .map(|expr| count_var_uses(expr, name))
        .sum()
}

pub(super) fn count_var_uses_in_stmt(stmt: &PreHirStmt, name: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        PreHirStmt::Expr(expr) => count_var_uses(expr, name),
        PreHirStmt::Block(stmts) => stmts
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        PreHirStmt::Switch {
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
        PreHirStmt::If {
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
        PreHirStmt::While { cond, body } => {
            count_var_uses(cond, name)
                + body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        PreHirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        PreHirStmt::For {
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
        PreHirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        PreHirStmt::VaStart { va_list, .. } => count_var_uses(va_list, name),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}

pub(super) fn count_var_uses_in_lvalue(lhs: &PreHirLValue, name: &str) -> usize {
    match lhs {
        PreHirLValue::Var(_) => 0,
        PreHirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => count_var_uses(base, name),
    }
}

pub(super) fn count_var_uses(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(var) | PreHirExpr::AddressOfGlobal(var) => usize::from(var == name),
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. } => count_var_uses(expr, name),
        PreHirExpr::Unary { expr, .. } => count_var_uses(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        PreHirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        PreHirExpr::Load { ptr, .. } => count_var_uses(ptr, name),
        PreHirExpr::PtrOffset { base, .. } => count_var_uses(base, name),
        PreHirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        PreHirExpr::AggregateCopy { src, .. } => count_var_uses(src, name),
        PreHirExpr::FieldAccess { base, .. } => count_var_uses(base, name),
        PreHirExpr::Select {
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

pub(super) fn stmt_mentions_var(stmt: &PreHirStmt, name: &str) -> bool {
    count_var_uses_in_stmt(stmt, name) > 0
}

pub(super) fn stmt_assigns_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_var(s, name)),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_var(s, name))
                || else_body.iter().any(|s| stmt_assigns_var(s, name))
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| stmt_assigns_var(s, name)))
                || default.iter().any(|s| stmt_assigns_var(s, name))
        }
        _ => false,
    }
}

pub(super) fn stmt_may_bypass_following_stmts(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Goto(_) | PreHirStmt::Return(_) | PreHirStmt::Break | PreHirStmt::Continue => true,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body.iter().any(stmt_may_bypass_following_stmts),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_may_bypass_following_stmts)
                || else_body.iter().any(stmt_may_bypass_following_stmts)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(stmt_may_bypass_following_stmts))
                || default.iter().any(stmt_may_bypass_following_stmts)
        }
        _ => false,
    }
}

pub(super) fn stmt_assigns_any_expr_var(stmt: &PreHirStmt, expr: &PreHirExpr) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => lvalue_assigns_any_expr_var(lhs, expr),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
                || else_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)))
                || default.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        _ => false,
    }
}

pub(super) fn lvalue_assigns_any_expr_var(lhs: &PreHirLValue, expr: &PreHirExpr) -> bool {
    match lhs {
        PreHirLValue::Var(name) => expr_contains_var(expr, name),
        PreHirLValue::Deref { .. } | PreHirLValue::Index { .. } | PreHirLValue::FieldAccess { .. } => false,
    }
}

pub(super) fn expr_contains_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(var) | PreHirExpr::AddressOfGlobal(var) => var == name,
        PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        PreHirExpr::Select {
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

pub(super) fn replace_var_in_stmt(stmt: &mut PreHirStmt, name: &str, replacement: &PreHirExpr) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        PreHirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Block(stmts) => {
            for stmt in stmts {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::Switch {
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
        PreHirStmt::If {
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
        PreHirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::DoWhile { body, cond } => {
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        PreHirStmt::For {
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
        PreHirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub(super) fn replace_var_in_lvalue(lhs: &mut PreHirLValue, name: &str, replacement: &PreHirExpr) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        PreHirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirLValue::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
    }
}

pub(super) fn replace_var_in_expr(expr: &mut PreHirExpr, name: &str, replacement: &PreHirExpr) {
    match expr {
        PreHirExpr::Var(var) if var == name => *expr = replacement.clone(),
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. } => replace_var_in_expr(expr, name, replacement),
        PreHirExpr::Unary { expr, .. } => replace_var_in_expr(expr, name, replacement),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        PreHirExpr::Load { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        PreHirExpr::PtrOffset { base, .. } => replace_var_in_expr(base, name, replacement),
        PreHirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirExpr::AggregateCopy { src, .. } => replace_var_in_expr(src, name, replacement),
        PreHirExpr::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
        PreHirExpr::Select {
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

pub(super) fn expr_mentions_var(expr: &PreHirExpr, name: &str) -> bool {
    count_var_uses(expr, name) > 0
}

pub(super) fn var_is_assigned_in_stmts(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| var_is_assigned_in_stmt(stmt, name))
}

pub(super) fn var_is_assigned_in_stmt(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => var_is_assigned_in_stmts(body, name),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => var_is_assigned_in_stmts(then_body, name) || var_is_assigned_in_stmts(else_body, name),
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| var_is_assigned_in_stmts(&case.body, name))
                || var_is_assigned_in_stmts(default, name)
        }
    }
}

pub fn collect_referenced_labels(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::default();
    for stmt in stmts {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub(super) fn collect_referenced_label_counts(stmts: &[PreHirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
    for stmt in stmts {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

pub(super) fn collect_stmt_referenced_labels(stmt: &PreHirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        PreHirStmt::If {
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
        PreHirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub(super) fn collect_stmt_referenced_label_counts(
    stmt: &PreHirStmt,
    counts: &mut HashMap<String, usize>,
) {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        PreHirStmt::If {
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
        PreHirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}
