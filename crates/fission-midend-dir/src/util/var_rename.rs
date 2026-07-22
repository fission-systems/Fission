//! Variable renaming helpers shared by normalize passes and preview type hints.

use crate::ir::{DirExpr, DirLValue, DirStmt};

pub fn rename_vars_in_stmts(body: &mut [DirStmt], renames: &[(String, String)]) {
    for stmt in body {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                rename_var_in_lvalue(lhs, renames);
                rename_var_in_expr(rhs, renames);
            }
            DirStmt::VaStart { va_list, .. } => rename_var_in_expr(va_list, renames),
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => rename_var_in_expr(expr, renames),
            DirStmt::Block(stmts) => rename_vars_in_stmts(stmts, renames),
            DirStmt::While { cond, body } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(body, renames);
            }
            DirStmt::DoWhile { body, cond } => {
                rename_vars_in_stmts(body, renames);
                rename_var_in_expr(cond, renames);
            }
            DirStmt::For {
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
            DirStmt::Switch {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(then_body, renames);
                rename_vars_in_stmts(else_body, renames);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn rename_var_in_lvalue(lvalue: &mut DirLValue, renames: &[(String, String)]) {
    match lvalue {
        DirLValue::Var(name) => rename_var_name(name, renames),
        DirLValue::Deref { ptr, .. } => rename_var_in_expr(ptr, renames),
        DirLValue::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        DirLValue::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
    }
}

fn rename_var_in_expr(expr: &mut DirExpr, renames: &[(String, String)]) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => rename_var_name(name, renames),
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => rename_var_in_expr(expr, renames),
        DirExpr::Binary { lhs, rhs, .. } => {
            rename_var_in_expr(lhs, renames);
            rename_var_in_expr(rhs, renames);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_var_in_expr(cond, renames);
            rename_var_in_expr(then_expr, renames);
            rename_var_in_expr(else_expr, renames);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                rename_var_in_expr(arg, renames);
            }
        }
        DirExpr::PtrOffset { base, .. } => rename_var_in_expr(base, renames),
        DirExpr::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        DirExpr::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
        DirExpr::Const(_, _) => {}
    }
}

fn rename_var_name(name: &mut String, renames: &[(String, String)]) {
    if let Some((_, replacement)) = renames.iter().find(|(from, _)| from == name) {
        *name = replacement.clone();
    }
}

/// Rewrite `DirExpr::FieldAccess`/`DirLValue::FieldAccess` field names for a
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
/// whose `base` is exactly `DirExpr::Var(base_var_name)` are matched --
/// deliberately narrow, matching the same single-level-of-indirection scope
/// as the `StructField` overlay that motivates this rewrite.
pub fn rewrite_field_access_names_in_stmts(
    body: &mut [DirStmt],
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    if renames.is_empty() {
        return;
    }
    for stmt in body {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                rewrite_field_access_names_in_lvalue(lhs, renames);
                rewrite_field_access_names_in_expr(rhs, renames);
            }
            DirStmt::VaStart { va_list, .. } => {
                rewrite_field_access_names_in_expr(va_list, renames)
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                rewrite_field_access_names_in_expr(expr, renames)
            }
            DirStmt::Block(stmts) => rewrite_field_access_names_in_stmts(stmts, renames),
            DirStmt::While { cond, body } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(body, renames);
            }
            DirStmt::DoWhile { body, cond } => {
                rewrite_field_access_names_in_stmts(body, renames);
                rewrite_field_access_names_in_expr(cond, renames);
            }
            DirStmt::For {
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
            DirStmt::Switch {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(then_body, renames);
                rewrite_field_access_names_in_stmts(else_body, renames);
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
}

fn rewrite_field_access_names_in_lvalue(
    lvalue: &mut DirLValue,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match lvalue {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => rewrite_field_access_names_in_expr(ptr, renames),
        DirLValue::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        DirLValue::FieldAccess {
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
    expr: &mut DirExpr,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_field_access_names_in_expr(expr, renames)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            rewrite_field_access_names_in_expr(lhs, renames);
            rewrite_field_access_names_in_expr(rhs, renames);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_field_access_names_in_expr(cond, renames);
            rewrite_field_access_names_in_expr(then_expr, renames);
            rewrite_field_access_names_in_expr(else_expr, renames);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_field_access_names_in_expr(arg, renames);
            }
        }
        DirExpr::PtrOffset { base, .. } => rewrite_field_access_names_in_expr(base, renames),
        DirExpr::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        DirExpr::FieldAccess {
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
    base: &DirExpr,
    offset: u32,
    field_name: &mut String,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    let DirExpr::Var(base_name) = base else {
        return;
    };
    if let Some(new_name) = renames.get(&(base_name.clone(), offset)) {
        *field_name = new_name.clone();
    }
}
