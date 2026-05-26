//! Variable renaming helpers shared by normalize passes and preview type hints.

use crate::nir::types::{HirExpr, HirLValue, HirStmt};

pub(crate) fn rename_vars_in_stmts(body: &mut [HirStmt], renames: &[(String, String)]) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                rename_var_in_lvalue(lhs, renames);
                rename_var_in_expr(rhs, renames);
            }
            HirStmt::VaStart { va_list, .. } => rename_var_in_expr(va_list, renames),
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => rename_var_in_expr(expr, renames),
            HirStmt::Block(stmts) => rename_vars_in_stmts(stmts, renames),
            HirStmt::While { cond, body } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(body, renames);
            }
            HirStmt::DoWhile { body, cond } => {
                rename_vars_in_stmts(body, renames);
                rename_var_in_expr(cond, renames);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    rename_vars_in_stmts(std::slice::from_mut(init_stmt.as_mut()), renames);
                }
                if let Some(cond_expr) = cond {
                    rename_var_in_expr(cond_expr, renames);
                }
                if let Some(update_stmt) = update {
                    rename_vars_in_stmts(std::slice::from_mut(update_stmt.as_mut()), renames);
                }
                rename_vars_in_stmts(body, renames);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rename_var_in_expr(expr, renames);
                for case in cases {
                    rename_vars_in_stmts(&mut case.body, renames);
                }
                rename_vars_in_stmts(default, renames);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(then_body, renames);
                rename_vars_in_stmts(else_body, renames);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn rename_var_in_lvalue(lvalue: &mut HirLValue, renames: &[(String, String)]) {
    match lvalue {
        HirLValue::Var(name) => rename_var_name(name, renames),
        HirLValue::Deref { ptr, .. } => rename_var_in_expr(ptr, renames),
        HirLValue::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
    }
}

fn rename_var_in_expr(expr: &mut HirExpr, renames: &[(String, String)]) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => rename_var_name(name, renames),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => rename_var_in_expr(expr, renames),
        HirExpr::Binary { lhs, rhs, .. } => {
            rename_var_in_expr(lhs, renames);
            rename_var_in_expr(rhs, renames);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_var_in_expr(cond, renames);
            rename_var_in_expr(then_expr, renames);
            rename_var_in_expr(else_expr, renames);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                rename_var_in_expr(arg, renames);
            }
        }
        HirExpr::PtrOffset { base, .. } => rename_var_in_expr(base, renames),
        HirExpr::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        HirExpr::Const(_, _) => {}
    }
}

fn rename_var_name(name: &mut String, renames: &[(String, String)]) {
    if let Some((_, replacement)) = renames.iter().find(|(from, _)| from == name) {
        *name = replacement.clone();
    }
}
