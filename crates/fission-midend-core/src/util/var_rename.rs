//! Variable renaming helpers shared by normalize passes and preview type hints.

use crate::ir::{HirExpr, HirLValue, HirStmt};

pub fn rename_vars_in_stmts(body: &mut [HirStmt], renames: &[(String, String)]) {
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
        HirLValue::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
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
        HirExpr::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
        HirExpr::Const(_, _) => {}
    }
}

fn rename_var_name(name: &mut String, renames: &[(String, String)]) {
    if let Some((_, replacement)) = renames.iter().find(|(from, _)| from == name) {
        *name = replacement.clone();
    }
}

/// Rewrite `HirExpr::FieldAccess`/`HirLValue::FieldAccess` field names for a
/// specific `(base variable name, byte offset)` key.
///
/// `FieldAccess` nodes are constructed once, during normalize's
/// pointer-arithmetic recovery (`fission-midend-normalize::memory::
/// ptr_arith`), which bakes `field_name` into the AST node at that point --
/// it does not read the name lazily from the base binding's `NirType::
/// Aggregate` fields at print time. So a later pass that wants to rename a
/// field (e.g. overlaying a real debug-info name onto a synthetic
/// `field_8`) must rewrite these already-built AST nodes directly, not just
/// the type-level `StructField` annotation.
///
/// `renames` is keyed by `(base_var_name, offset)`; only `FieldAccess` nodes
/// whose `base` is exactly `HirExpr::Var(base_var_name)` are matched --
/// deliberately narrow, matching the same single-level-of-indirection scope
/// as the `StructField` overlay that motivates this rewrite.
pub fn rewrite_field_access_names_in_stmts(
    body: &mut [HirStmt],
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    if renames.is_empty() {
        return;
    }
    for stmt in body {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                rewrite_field_access_names_in_lvalue(lhs, renames);
                rewrite_field_access_names_in_expr(rhs, renames);
            }
            HirStmt::VaStart { va_list, .. } => {
                rewrite_field_access_names_in_expr(va_list, renames)
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                rewrite_field_access_names_in_expr(expr, renames)
            }
            HirStmt::Block(stmts) => rewrite_field_access_names_in_stmts(stmts, renames),
            HirStmt::While { cond, body } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(body, renames);
            }
            HirStmt::DoWhile { body, cond } => {
                rewrite_field_access_names_in_stmts(body, renames);
                rewrite_field_access_names_in_expr(cond, renames);
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    rewrite_field_access_names_in_stmts(
                        std::slice::from_mut(init_stmt.as_mut()),
                        renames,
                    );
                }
                if let Some(cond_expr) = cond {
                    rewrite_field_access_names_in_expr(cond_expr, renames);
                }
                if let Some(update_stmt) = update {
                    rewrite_field_access_names_in_stmts(
                        std::slice::from_mut(update_stmt.as_mut()),
                        renames,
                    );
                }
                rewrite_field_access_names_in_stmts(body, renames);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite_field_access_names_in_expr(expr, renames);
                for case in cases {
                    rewrite_field_access_names_in_stmts(&mut case.body, renames);
                }
                rewrite_field_access_names_in_stmts(default, renames);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(then_body, renames);
                rewrite_field_access_names_in_stmts(else_body, renames);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn rewrite_field_access_names_in_lvalue(
    lvalue: &mut HirLValue,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match lvalue {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => rewrite_field_access_names_in_expr(ptr, renames),
        HirLValue::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        HirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ..
        } => {
            apply_field_rename(base, *offset, field_name, renames);
            rewrite_field_access_names_in_expr(base, renames);
        }
    }
}

fn rewrite_field_access_names_in_expr(
    expr: &mut HirExpr,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_field_access_names_in_expr(expr, renames)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            rewrite_field_access_names_in_expr(lhs, renames);
            rewrite_field_access_names_in_expr(rhs, renames);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_field_access_names_in_expr(cond, renames);
            rewrite_field_access_names_in_expr(then_expr, renames);
            rewrite_field_access_names_in_expr(else_expr, renames);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_field_access_names_in_expr(arg, renames);
            }
        }
        HirExpr::PtrOffset { base, .. } => rewrite_field_access_names_in_expr(base, renames),
        HirExpr::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        HirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ..
        } => {
            apply_field_rename(base, *offset, field_name, renames);
            rewrite_field_access_names_in_expr(base, renames);
        }
    }
}

fn apply_field_rename(
    base: &HirExpr,
    offset: u32,
    field_name: &mut String,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    let HirExpr::Var(base_name) = base else {
        return;
    };
    if let Some(new_name) = renames.get(&(base_name.clone(), offset)) {
        *field_name = new_name.clone();
    }
}
