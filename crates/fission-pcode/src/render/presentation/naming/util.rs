use super::{HirExpr, HirFunction, HirLValue, HirStmt};

/// `true` for names produced by earlier pipeline stages as generic
/// placeholders -- the only ones semantic naming is allowed to touch.
/// Anything else (a DWARF-sourced name, an earlier semantic-naming rename,
/// a name a user supplied) is left alone.
pub(super) fn is_renamable_temp_name(name: &str) -> bool {
    if let Some(rest) = name.strip_prefix("param_") {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    for prefix in ["uVar", "iVar", "xVar", "bVar"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
        }
    }
    false
}

/// Peel `Cast` to find the underlying variable name, if any -- mirrors how
/// materialize/normalize already treat a cast of a variable as still
/// "being" that variable for pattern-matching purposes.
pub(super) fn as_var(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => as_var(expr),
        _ => None,
    }
}

/// Rename every occurrence of variable `old` to `new`: its binding (param or
/// local) and every read/write of it throughout the body. A pure relabeling
/// -- evaluation count, order, and side effects are unchanged (ADR 0011).
pub(super) fn rename_var_everywhere(func: &mut HirFunction, old: &str, new: &str) {
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if binding.name == old {
            binding.name = new.to_string();
        }
    }
    for stmt in &mut func.body {
        rename_var_in_stmt(stmt, old, new);
    }
}

fn rename_var_in_lvalue(lhs: &mut HirLValue, old: &str, new: &str) {
    match lhs {
        HirLValue::Var(n) => {
            if n == old {
                *n = new.to_string();
            }
        }
        HirLValue::Deref { ptr, .. } => rename_var_in_expr(ptr, old, new),
        HirLValue::Index { base, index, .. } => {
            rename_var_in_expr(base, old, new);
            rename_var_in_expr(index, old, new);
        }
        HirLValue::FieldAccess { base, .. } => rename_var_in_expr(base, old, new),
    }
}

fn rename_var_in_expr(expr: &mut HirExpr, old: &str, new: &str) {
    match expr {
        HirExpr::Var(n) => {
            if n == old {
                *n = new.to_string();
            }
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            rename_var_in_expr(expr, old, new)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            rename_var_in_expr(lhs, old, new);
            rename_var_in_expr(rhs, old, new);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_var_in_expr(cond, old, new);
            rename_var_in_expr(then_expr, old, new);
            rename_var_in_expr(else_expr, old, new);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                rename_var_in_expr(a, old, new);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => rename_var_in_expr(ptr, old, new),
        HirExpr::Index { base, index, .. } => {
            rename_var_in_expr(base, old, new);
            rename_var_in_expr(index, old, new);
        }
    }
}

fn rename_var_in_stmt(stmt: &mut HirStmt, old: &str, new: &str) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            rename_var_in_lvalue(lhs, old, new);
            rename_var_in_expr(rhs, old, new);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            rename_var_in_expr(e, old, new)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body {
                rename_var_in_stmt(s, old, new);
            }
        }
        HirStmt::While { cond, body } => {
            rename_var_in_expr(cond, old, new);
            for s in body {
                rename_var_in_stmt(s, old, new);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for s in body {
                rename_var_in_stmt(s, old, new);
            }
            rename_var_in_expr(cond, old, new);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rename_var_in_expr(cond, old, new);
            for s in then_body {
                rename_var_in_stmt(s, old, new);
            }
            for s in else_body {
                rename_var_in_stmt(s, old, new);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                rename_var_in_stmt(i, old, new);
            }
            if let Some(c) = cond {
                rename_var_in_expr(c, old, new);
            }
            if let Some(u) = update {
                rename_var_in_stmt(u, old, new);
            }
            for s in body {
                rename_var_in_stmt(s, old, new);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rename_var_in_expr(expr, old, new);
            for case in cases {
                for s in &mut case.body {
                    rename_var_in_stmt(s, old, new);
                }
            }
            for s in default {
                rename_var_in_stmt(s, old, new);
            }
        }
    }
}
