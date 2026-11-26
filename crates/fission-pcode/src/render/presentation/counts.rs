//! Definition and use counts over HIR statements.

use super::*;

pub(super) fn count_defs_in_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for s in stmts {
        count_defs_in_stmt(s, out);
    }
}

pub(super) fn count_defs_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            count_defs_in_stmts(body, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_defs_in_stmts(then_body, out);
            count_defs_in_stmts(else_body, out);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                count_defs_in_stmt(i, out);
            }
            if let Some(u) = update {
                count_defs_in_stmt(u, out);
            }
            count_defs_in_stmts(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_defs_in_stmts(&case.body, out);
            }
            count_defs_in_stmts(default, out);
        }
        _ => {}
    }
}

pub(super) fn count_uses_in_stmts(stmts: &[HirStmt], name: &str) -> usize {
    stmts.iter().map(|s| count_uses_in_stmt(s, name)).sum()
}

pub(super) fn count_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_uses_in_lvalue(lhs, name) + count_uses_in_expr(rhs, name)
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            count_uses_in_expr(e, name)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => 0,
        HirStmt::Block(body) => count_uses_in_stmts(body, name),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            count_uses_in_expr(cond, name) + count_uses_in_stmts(body, name)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_uses_in_expr(cond, name)
                + count_uses_in_stmts(then_body, name)
                + count_uses_in_stmts(else_body, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().map_or(0, |s| count_uses_in_stmt(s, name))
                + cond.as_ref().map_or(0, |e| count_uses_in_expr(e, name))
                + update.as_ref().map_or(0, |s| count_uses_in_stmt(s, name))
                + count_uses_in_stmts(body, name)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_uses_in_expr(expr, name)
                + cases
                    .iter()
                    .map(|c| count_uses_in_stmts(&c.body, name))
                    .sum::<usize>()
                + count_uses_in_stmts(default, name)
        }
    }
}

pub(super) fn count_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(_) => 0,
        HirLValue::Deref { ptr, .. } => count_uses_in_expr(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_uses_in_expr(base, name) + count_uses_in_expr(index, name)
        }
        HirLValue::FieldAccess { base, .. } => count_uses_in_expr(base, name),
    }
}

pub(super) fn count_uses_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfLocal(n) => usize::from(n == name),
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => 0,
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => count_uses_in_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_uses_in_expr(lhs, name) + count_uses_in_expr(rhs, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_uses_in_expr(cond, name)
                + count_uses_in_expr(then_expr, name)
                + count_uses_in_expr(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_uses_in_expr(a, name)).sum(),
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => count_uses_in_expr(ptr, name),
        HirExpr::Index { base, index, .. } => {
            count_uses_in_expr(base, name) + count_uses_in_expr(index, name)
        }
    }
}

/// Count every rvalue/address-taken variable use across `stmts` in one pass.
pub(super) fn count_all_uses_in_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_all_uses_in_stmt(stmt, out);
    }
}

pub(super) fn count_all_uses_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_all_uses_in_lvalue(lhs, out);
            count_all_uses_in_expr(rhs, out);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            count_all_uses_in_expr(e, out);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => count_all_uses_in_stmts(body, out),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            count_all_uses_in_expr(cond, out);
            count_all_uses_in_stmts(body, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_all_uses_in_expr(cond, out);
            count_all_uses_in_stmts(then_body, out);
            count_all_uses_in_stmts(else_body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                count_all_uses_in_stmt(i, out);
            }
            if let Some(c) = cond {
                count_all_uses_in_expr(c, out);
            }
            if let Some(u) = update {
                count_all_uses_in_stmt(u, out);
            }
            count_all_uses_in_stmts(body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_all_uses_in_expr(expr, out);
            for case in cases {
                count_all_uses_in_stmts(&case.body, out);
            }
            count_all_uses_in_stmts(default, out);
        }
    }
}

pub(super) fn count_all_uses_in_lvalue(lhs: &HirLValue, out: &mut HashMap<String, usize>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => count_all_uses_in_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            count_all_uses_in_expr(base, out);
            count_all_uses_in_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => count_all_uses_in_expr(base, out),
    }
}

pub(super) fn count_all_uses_in_expr(expr: &HirExpr, out: &mut HashMap<String, usize>) {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfLocal(n) => {
            *out.entry(n.clone()).or_insert(0) += 1;
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            count_all_uses_in_expr(expr, out);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            count_all_uses_in_expr(lhs, out);
            count_all_uses_in_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_all_uses_in_expr(cond, out);
            count_all_uses_in_expr(then_expr, out);
            count_all_uses_in_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                count_all_uses_in_expr(a, out);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => count_all_uses_in_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            count_all_uses_in_expr(base, out);
            count_all_uses_in_expr(index, out);
        }
    }
}
