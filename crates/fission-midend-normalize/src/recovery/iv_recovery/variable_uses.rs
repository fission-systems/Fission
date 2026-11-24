use super::loop_expressions::expr_vars;
use crate::HashSet;
use crate::prelude::*;

pub(super) fn count_var_uses_in_stmt(stmt: &PreHirStmt, name: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            count_var_uses(va_list, name)
        }
        PreHirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => body
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
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
            init.as_deref()
                .map_or(0, |stmt| count_var_uses_in_stmt(stmt, name))
                + cond.as_ref().map_or(0, |expr| count_var_uses(expr, name))
                + update
                    .as_deref()
                    .map_or(0, |stmt| count_var_uses_in_stmt(stmt, name))
                + body
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
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}

fn count_var_uses_in_lvalue(lhs: &PreHirLValue, name: &str) -> usize {
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
        PreHirExpr::Var(var)
        | PreHirExpr::AddressOfGlobal(var)
        | PreHirExpr::AddressOfLocal(var) => usize::from(var == name),
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => count_var_uses(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_uses(lhs, name) + count_var_uses(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        PreHirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
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

fn collect_names_in_lvalue(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(name) => {
            out.insert(name.clone());
        }
        PreHirLValue::Deref { ptr, .. } => expr_vars(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            expr_vars(base, out);
        }
    }
}

fn collect_names_in_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_names_in_lvalue(lhs, out);
            expr_vars(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => expr_vars(va_list, out),
        PreHirStmt::Return(Some(expr)) => expr_vars(expr, out),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
            collect_names_in_stmts(body, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_names_in_stmts(body, out);
            expr_vars(cond, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_names_in_stmt(init, out);
            }
            if let Some(cond) = cond {
                expr_vars(cond, out);
            }
            if let Some(update) = update {
                collect_names_in_stmt(update, out);
            }
            collect_names_in_stmts(body, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_vars(cond, out);
            collect_names_in_stmts(then_body, out);
            collect_names_in_stmts(else_body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_vars(expr, out);
            for case in cases {
                collect_names_in_stmts(&case.body, out);
            }
            collect_names_in_stmts(default, out);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_names_in_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_names_in_stmt(stmt, out);
    }
}

pub(super) fn fresh_index_name(locals: &[PreHirBinding], stmts: &[PreHirStmt]) -> String {
    let mut used = HashSet::default();
    for local in locals {
        used.insert(local.name.clone());
    }
    collect_names_in_stmts(stmts, &mut used);
    for id in 0.. {
        let name = format!("iVar{id}");
        if !used.contains(&name) {
            return name;
        }
    }
    unreachable!()
}
