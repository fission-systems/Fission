//! Variable mention queries and in-place substitution over HIR trees.

use super::*;

pub(super) fn expr_mentions_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(n) => n == name,
        HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => expr_mentions_var(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_mentions_var(lhs, name) || expr_mentions_var(rhs, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_mentions_var(cond, name)
                || expr_mentions_var(then_expr, name)
                || expr_mentions_var(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|a| expr_mentions_var(a, name)),
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => expr_mentions_var(ptr, name),
        HirExpr::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
    }
}

pub(super) fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
    match expr {
        HirExpr::Var(var) if var == name => *expr = replacement.clone(),
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            replace_var_in_expr(expr, name, replacement)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
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
        HirExpr::Call { args, .. } => {
            for a in args {
                replace_var_in_expr(a, name, replacement);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
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
        HirLValue::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
    }
}

pub(super) fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            replace_var_in_expr(e, name, replacement)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            replace_var_in_expr(cond, name, replacement);
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for s in then_body {
                replace_var_in_stmt(s, name, replacement);
            }
            for s in else_body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                replace_var_in_stmt(i, name, replacement);
            }
            if let Some(c) = cond {
                replace_var_in_expr(c, name, replacement);
            }
            if let Some(u) = update {
                replace_var_in_stmt(u, name, replacement);
            }
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for s in &mut case.body {
                    replace_var_in_stmt(s, name, replacement);
                }
            }
            for s in default {
                replace_var_in_stmt(s, name, replacement);
            }
        }
    }
}

/// Occurrences of `name` in `expr`.
pub(super) fn count_var_mentions(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) => usize::from(n == name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_var_mentions(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_mentions(lhs, name) + count_var_mentions(rhs, name)
        }
        HirExpr::Index { base, index, .. } => {
            count_var_mentions(base, name) + count_var_mentions(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_mentions(cond, name)
                + count_var_mentions(then_expr, name)
                + count_var_mentions(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_var_mentions(a, name)).sum(),
        _ => 0,
    }
}

/// Replace every `Var(name)` in `expr` with `value`.
pub(super) fn substitute_var(expr: &mut HirExpr, name: &str, value: &HirExpr) {
    match expr {
        HirExpr::Var(n) if n == name => *expr = value.clone(),
        HirExpr::Var(_) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => substitute_var(inner, name, value),
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_var(lhs, name, value);
            substitute_var(rhs, name, value);
        }
        HirExpr::Index { base, index, .. } => {
            substitute_var(base, name, value);
            substitute_var(index, name, value);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_var(cond, name, value);
            substitute_var(then_expr, name, value);
            substitute_var(else_expr, name, value);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                substitute_var(a, name, value);
            }
        }
        _ => {}
    }
}
