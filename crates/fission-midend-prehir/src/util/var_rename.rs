//! Variable renaming helpers shared by normalize passes and preview type hints.

use crate::ir::{PreHirExpr, PreHirLValue, PreHirStmt};

pub fn rename_vars_in_stmts(body: &mut [PreHirStmt], renames: &[(String, String)]) {
    for stmt in body {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                rename_var_in_lvalue(lhs, renames);
                rename_var_in_expr(rhs, renames);
            }
            PreHirStmt::VaStart { va_list, .. } => rename_var_in_expr(va_list, renames),
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                rename_var_in_expr(expr, renames)
            }
            PreHirStmt::Block(stmts) => {
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts), renames)
            }
            PreHirStmt::While { cond, body } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), renames);
            }
            PreHirStmt::DoWhile { body, cond } => {
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), renames);
                rename_var_in_expr(cond, renames);
            }
            PreHirStmt::For {
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
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), renames);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rename_var_in_expr(expr, renames);
                for case in cases {
                    rename_vars_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        renames,
                    );
                }
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), renames);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rename_var_in_expr(cond, renames);
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), renames);
                rename_vars_in_stmts(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), renames);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn rename_var_in_lvalue(lvalue: &mut PreHirLValue, renames: &[(String, String)]) {
    match lvalue {
        PreHirLValue::Var(name) => rename_var_name(name, renames),
        PreHirLValue::Deref { ptr, .. } => rename_var_in_expr(ptr, renames),
        PreHirLValue::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        PreHirLValue::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
    }
}

fn rename_var_in_expr(expr: &mut PreHirExpr, renames: &[(String, String)]) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => rename_var_name(name, renames),
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => rename_var_in_expr(expr, renames),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rename_var_in_expr(lhs, renames);
            rename_var_in_expr(rhs, renames);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_var_in_expr(cond, renames);
            rename_var_in_expr(then_expr, renames);
            rename_var_in_expr(else_expr, renames);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rename_var_in_expr(arg, renames);
            }
        }
        PreHirExpr::PtrOffset { base, .. } => rename_var_in_expr(base, renames),
        PreHirExpr::Index { base, index, .. } => {
            rename_var_in_expr(base, renames);
            rename_var_in_expr(index, renames);
        }
        PreHirExpr::FieldAccess { base, .. } => rename_var_in_expr(base, renames),
        PreHirExpr::Const(_, _) => {}
    }
}

/// Rename variable references in a standalone expression.
///
/// This complements [`rename_vars_in_stmts`] for function binding
/// initializers, which live outside the statement body.
pub fn rename_vars_in_expr(expr: &mut PreHirExpr, renames: &[(String, String)]) {
    rename_var_in_expr(expr, renames);
}

fn rename_var_name(name: &mut String, renames: &[(String, String)]) {
    if let Some((_, replacement)) = renames.iter().find(|(from, _)| from == name) {
        *name = replacement.clone();
    }
}

/// Rewrite `PreHirExpr::FieldAccess`/`PreHirLValue::FieldAccess` field names for a
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
/// whose `base` is exactly `PreHirExpr::Var(base_var_name)` are matched --
/// deliberately narrow, matching the same single-level-of-indirection scope
/// as the `StructField` overlay that motivates this rewrite.
pub fn rewrite_field_access_names_in_stmts(
    body: &mut [PreHirStmt],
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    if renames.is_empty() {
        return;
    }
    for stmt in body {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                rewrite_field_access_names_in_lvalue(lhs, renames);
                rewrite_field_access_names_in_expr(rhs, renames);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                rewrite_field_access_names_in_expr(va_list, renames)
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                rewrite_field_access_names_in_expr(expr, renames)
            }
            PreHirStmt::Block(stmts) => rewrite_field_access_names_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                renames,
            ),
            PreHirStmt::While { cond, body } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    renames,
                );
            }
            PreHirStmt::DoWhile { body, cond } => {
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    renames,
                );
                rewrite_field_access_names_in_expr(cond, renames);
            }
            PreHirStmt::For {
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
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    renames,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                rewrite_field_access_names_in_expr(expr, renames);
                for case in cases {
                    rewrite_field_access_names_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        renames,
                    );
                }
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    renames,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                rewrite_field_access_names_in_expr(cond, renames);
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    renames,
                );
                rewrite_field_access_names_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    renames,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn rewrite_field_access_names_in_lvalue(
    lvalue: &mut PreHirLValue,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match lvalue {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => rewrite_field_access_names_in_expr(ptr, renames),
        PreHirLValue::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        PreHirLValue::FieldAccess {
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
    expr: &mut PreHirExpr,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_field_access_names_in_expr(expr, renames)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_field_access_names_in_expr(lhs, renames);
            rewrite_field_access_names_in_expr(rhs, renames);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_field_access_names_in_expr(cond, renames);
            rewrite_field_access_names_in_expr(then_expr, renames);
            rewrite_field_access_names_in_expr(else_expr, renames);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_field_access_names_in_expr(arg, renames);
            }
        }
        PreHirExpr::PtrOffset { base, .. } => rewrite_field_access_names_in_expr(base, renames),
        PreHirExpr::Index { base, index, .. } => {
            rewrite_field_access_names_in_expr(base, renames);
            rewrite_field_access_names_in_expr(index, renames);
        }
        PreHirExpr::FieldAccess {
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
    base: &PreHirExpr,
    offset: u32,
    field_name: &mut String,
    renames: &std::collections::HashMap<(String, u32), String>,
) {
    let PreHirExpr::Var(base_name) = base else {
        return;
    };
    if let Some(new_name) = renames.get(&(base_name.clone(), offset)) {
        *field_name = new_name.clone();
    }
}
