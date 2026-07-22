use fission_midend_core::ir::{DirBinaryOp, DirExpr, DirLValue, DirStmt, DirBinding, NirType};
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
pub(super) fn is_pure_return_collapse_rhs(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Const(_, _) | DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) => true,
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            is_pure_return_collapse_rhs(expr)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            is_pure_return_collapse_rhs(lhs) && is_pure_return_collapse_rhs(rhs)
        }
        _ => false,
    }
}

pub fn expr_has_side_effects(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => false,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => expr_has_side_effects(expr),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        DirExpr::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effects(cond)
                || expr_has_side_effects(then_expr)
                || expr_has_side_effects(else_expr)
        }
        DirExpr::Call { target, args, .. } => {
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

pub(super) fn retain_unmarked_stmts(stmts: &mut Vec<DirStmt>, to_remove: &[bool]) {
    let mut idx = 0usize;
    stmts.retain(|_| {
        let keep = !to_remove.get(idx).copied().unwrap_or(false);
        idx += 1;
        keep
    });
}

pub(super) fn count_uses_in_stmt_list(stmts: &[DirStmt], name: &str) -> usize {
    stmts
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, name))
        .sum()
}

pub(super) fn count_uses_in_bindings(bindings: &[DirBinding], name: &str) -> usize {
    bindings
        .iter()
        .filter(|binding| binding.name != name)
        .filter_map(|binding| binding.initializer.as_ref())
        .map(|expr| count_var_uses(expr, name))
        .sum()
}

pub(super) fn count_var_uses_in_stmt(stmt: &DirStmt, name: &str) -> usize {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        DirStmt::Expr(expr) => count_var_uses(expr, name),
        DirStmt::Block(stmts) => stmts
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        DirStmt::Switch {
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
        DirStmt::If {
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
        DirStmt::While { cond, body } => {
            count_var_uses(cond, name)
                + body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        DirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        DirStmt::For {
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
        DirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        DirStmt::VaStart { va_list, .. } => count_var_uses(va_list, name),
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => 0,
    }
}

pub(super) fn count_var_uses_in_lvalue(lhs: &DirLValue, name: &str) -> usize {
    match lhs {
        DirLValue::Var(_) => 0,
        DirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        DirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        DirLValue::FieldAccess { base, .. } => count_var_uses(base, name),
    }
}

pub(super) fn count_var_uses(expr: &DirExpr, name: &str) -> usize {
    match expr {
        DirExpr::Var(var) | DirExpr::AddressOfGlobal(var) => usize::from(var == name),
        DirExpr::Const(_, _) => 0,
        DirExpr::Cast { expr, .. } => count_var_uses(expr, name),
        DirExpr::Unary { expr, .. } => count_var_uses(expr, name),
        DirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        DirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        DirExpr::Load { ptr, .. } => count_var_uses(ptr, name),
        DirExpr::PtrOffset { base, .. } => count_var_uses(base, name),
        DirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        DirExpr::AggregateCopy { src, .. } => count_var_uses(src, name),
        DirExpr::FieldAccess { base, .. } => count_var_uses(base, name),
        DirExpr::Select {
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

pub(super) fn stmt_mentions_var(stmt: &DirStmt, name: &str) -> bool {
    count_var_uses_in_stmt(stmt, name) > 0
}

pub(super) fn stmt_assigns_var(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_var(s, name)),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_var(s, name))
                || else_body.iter().any(|s| stmt_assigns_var(s, name))
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| stmt_assigns_var(s, name)))
                || default.iter().any(|s| stmt_assigns_var(s, name))
        }
        _ => false,
    }
}

pub(super) fn stmt_may_bypass_following_stmts(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Goto(_) | DirStmt::Return(_) | DirStmt::Break | DirStmt::Continue => true,
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => body.iter().any(stmt_may_bypass_following_stmts),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_may_bypass_following_stmts)
                || else_body.iter().any(stmt_may_bypass_following_stmts)
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(stmt_may_bypass_following_stmts))
                || default.iter().any(stmt_may_bypass_following_stmts)
        }
        _ => false,
    }
}

pub(super) fn stmt_assigns_any_expr_var(stmt: &DirStmt, expr: &DirExpr) -> bool {
    match stmt {
        DirStmt::Assign { lhs, .. } => lvalue_assigns_any_expr_var(lhs, expr),
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
                || else_body.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| stmt_assigns_any_expr_var(s, expr)))
                || default.iter().any(|s| stmt_assigns_any_expr_var(s, expr))
        }
        _ => false,
    }
}

pub(super) fn lvalue_assigns_any_expr_var(lhs: &DirLValue, expr: &DirExpr) -> bool {
    match lhs {
        DirLValue::Var(name) => expr_contains_var(expr, name),
        DirLValue::Deref { .. } | DirLValue::Index { .. } | DirLValue::FieldAccess { .. } => false,
    }
}

pub(super) fn expr_contains_var(expr: &DirExpr, name: &str) -> bool {
    match expr {
        DirExpr::Var(var) | DirExpr::AddressOfGlobal(var) => var == name,
        DirExpr::Const(_, _) => false,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => expr_contains_var(expr, name),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        DirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        DirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        DirExpr::Select {
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

pub(super) fn replace_var_in_stmt(stmt: &mut DirStmt, name: &str, replacement: &DirExpr) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        DirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        DirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        DirStmt::Block(stmts) => {
            for stmt in stmts {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        DirStmt::Switch {
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
        DirStmt::If {
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
        DirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        DirStmt::DoWhile { body, cond } => {
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        DirStmt::For {
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
        DirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

pub(super) fn replace_var_in_lvalue(lhs: &mut DirLValue, name: &str, replacement: &DirExpr) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        DirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        DirLValue::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
    }
}

pub(super) fn replace_var_in_expr(expr: &mut DirExpr, name: &str, replacement: &DirExpr) {
    match expr {
        DirExpr::Var(var) if var == name => *expr = replacement.clone(),
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr, .. } => replace_var_in_expr(expr, name, replacement),
        DirExpr::Unary { expr, .. } => replace_var_in_expr(expr, name, replacement),
        DirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        DirExpr::Load { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        DirExpr::PtrOffset { base, .. } => replace_var_in_expr(base, name, replacement),
        DirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        DirExpr::AggregateCopy { src, .. } => replace_var_in_expr(src, name, replacement),
        DirExpr::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
        DirExpr::Select {
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

pub(super) fn expr_mentions_var(expr: &DirExpr, name: &str) -> bool {
    count_var_uses(expr, name) > 0
}

pub(super) fn var_is_assigned_in_stmts(stmts: &[DirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| var_is_assigned_in_stmt(stmt, name))
}

pub(super) fn var_is_assigned_in_stmt(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Return(_)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => false,
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => var_is_assigned_in_stmts(body, name),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => var_is_assigned_in_stmts(then_body, name) || var_is_assigned_in_stmts(else_body, name),
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| var_is_assigned_in_stmts(&case.body, name))
                || var_is_assigned_in_stmts(default, name)
        }
    }
}

pub fn collect_referenced_labels(stmts: &[DirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::default();
    for stmt in stmts {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub(super) fn collect_referenced_label_counts(stmts: &[DirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
    for stmt in stmts {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

pub(super) fn collect_stmt_referenced_labels(stmt: &DirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        DirStmt::If {
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
        DirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

pub(super) fn collect_stmt_referenced_label_counts(
    stmt: &DirStmt,
    counts: &mut HashMap<String, usize>,
) {
    match stmt {
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        DirStmt::If {
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
        DirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}
