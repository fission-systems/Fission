//! Pure HIR rewrite helpers used by guarded-tail promotion/execution.

use fission_midend_core::ir::{DirExpr, DirLValue, DirStmt};

pub fn expr_contains_var(expr: &DirExpr, name: &str) -> bool {
        match expr {
            DirExpr::Var(var) | DirExpr::AddressOfGlobal(var) => var == name,
            DirExpr::Const(_, _) => false,
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::Load { ptr: expr, .. }
            | DirExpr::PtrOffset { base: expr, .. }
            | DirExpr::FieldAccess { base: expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
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

pub fn lvalue_contains_var(lhs: &DirLValue, name: &str) -> bool {
        match lhs {
            DirLValue::Var(_) => false,
            DirLValue::Deref { ptr, .. } => expr_contains_var(ptr, name),
            DirLValue::Index { base, index, .. } => {
                expr_contains_var(base, name) || expr_contains_var(index, name)
            }
            DirLValue::FieldAccess { base, .. } => expr_contains_var(base, name),
        }
    }

pub fn replace_var_in_expr(expr: &mut DirExpr, name: &str, replacement: &DirExpr) {
        match expr {
            DirExpr::Var(var) if var == name => *expr = replacement.clone(),
            DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::Load { ptr: expr, .. }
            | DirExpr::PtrOffset { base: expr, .. }
            | DirExpr::FieldAccess { base: expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => {
                replace_var_in_expr(expr, name, replacement);
            }
            DirExpr::Binary { lhs, rhs, .. } => {
                replace_var_in_expr(lhs, name, replacement);
                replace_var_in_expr(rhs, name, replacement);
            }
            DirExpr::Call { args, .. } => {
                for arg in args {
                    replace_var_in_expr(arg, name, replacement);
                }
            }
            DirExpr::Index { base, index, .. } => {
                replace_var_in_expr(base, name, replacement);
                replace_var_in_expr(index, name, replacement);
            }
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

pub fn replace_var_in_lvalue(lhs: &mut DirLValue, name: &str, replacement: &DirExpr) {
        match lhs {
            DirLValue::Var(_) => {}
            DirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
            DirLValue::Index { base, index, .. } => {
                replace_var_in_expr(base, name, replacement);
                replace_var_in_expr(index, name, replacement);
            }
            DirLValue::FieldAccess { base, .. } => {
                replace_var_in_expr(base, name, replacement);
            }
        }
    }

pub fn replace_var_in_stmt(stmt: &mut DirStmt, name: &str, replacement: &DirExpr) {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                replace_var_in_lvalue(lhs, name, replacement);
                replace_var_in_expr(rhs, name, replacement);
            }
            DirStmt::VaStart { va_list, .. } => {
                replace_var_in_expr(va_list, name, replacement)
            }
            DirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
            DirStmt::Block(stmts) => {
                for stmt in stmts {
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
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init_stmt) = init {
                    replace_var_in_stmt(init_stmt, name, replacement);
                }
                if let Some(cond) = cond {
                    replace_var_in_expr(cond, name, replacement);
                }
                if let Some(update_stmt) = update {
                    replace_var_in_stmt(update_stmt, name, replacement);
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

pub fn count_var_defs_stmt(stmt: &DirStmt, target: &str) -> usize {
        match stmt {
            DirStmt::Assign { lhs, .. } => {
                usize::from(matches!(lhs, DirLValue::Var(name) if name == target))
            }
            DirStmt::Block(stmts)
            | DirStmt::While { body: stmts, .. }
            | DirStmt::DoWhile { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| count_var_defs_stmt(stmt, target))
                .sum(),
            DirStmt::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_var_defs_stmt(stmt, target))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                then_body
                    .iter()
                    .map(|stmt| count_var_defs_stmt(stmt, target))
                    .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                init.iter()
                    .map(|stmt| count_var_defs_stmt(stmt, target))
                    .sum::<usize>()
                    + update
                        .iter()
                        .map(|stmt| count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
                    + body
                        .iter()
                        .map(|stmt| count_var_defs_stmt(stmt, target))
                        .sum::<usize>()
            }
            DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => 0,
        }
    }

pub fn count_var_reads_expr(expr: &DirExpr, name: &str) -> usize {
        match expr {
            DirExpr::Var(var) | DirExpr::AddressOfGlobal(var) => usize::from(var == name),
            DirExpr::Const(_, _) => 0,
            DirExpr::Cast { expr, .. }
            | DirExpr::Unary { expr, .. }
            | DirExpr::Load { ptr: expr, .. }
            | DirExpr::PtrOffset { base: expr, .. }
            | DirExpr::FieldAccess { base: expr, .. }
            | DirExpr::AggregateCopy { src: expr, .. } => count_var_reads_expr(expr, name),
            DirExpr::Binary { lhs, rhs, .. } => {
                count_var_reads_expr(lhs, name) + count_var_reads_expr(rhs, name)
            }
            DirExpr::Call { args, .. } => args
                .iter()
                .map(|arg| count_var_reads_expr(arg, name))
                .sum(),
            DirExpr::Index { base, index, .. } => {
                count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
            }
            DirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                count_var_reads_expr(cond, name)
                    + count_var_reads_expr(then_expr, name)
                    + count_var_reads_expr(else_expr, name)
            }
        }
    }

pub fn count_var_reads_lvalue(lhs: &DirLValue, name: &str) -> usize {
        match lhs {
            DirLValue::Var(_) => 0,
            DirLValue::Deref { ptr, .. } => count_var_reads_expr(ptr, name),
            DirLValue::Index { base, index, .. } => {
                count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
            }
            DirLValue::FieldAccess { base, .. } => count_var_reads_expr(base, name),
        }
    }

pub fn count_var_reads_stmt(stmt: &DirStmt, name: &str) -> usize {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                count_var_reads_lvalue(lhs, name) + count_var_reads_expr(rhs, name)
            }
            DirStmt::VaStart { va_list, .. } => count_var_reads_expr(va_list, name),
            DirStmt::Expr(expr) => count_var_reads_expr(expr, name),
            DirStmt::Block(stmts) | DirStmt::While { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| count_var_reads_stmt(stmt, name))
                .sum(),
            DirStmt::DoWhile { body, cond } => {
                body.iter()
                    .map(|stmt| count_var_reads_stmt(stmt, name))
                    .sum::<usize>()
                    + count_var_reads_expr(cond, name)
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                count_var_reads_expr(expr, name)
                    + cases
                        .iter()
                        .map(|case| {
                            case.body
                                .iter()
                                .map(|stmt| count_var_reads_stmt(stmt, name))
                                .sum::<usize>()
                        })
                        .sum::<usize>()
                    + default
                        .iter()
                        .map(|stmt| count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                count_var_reads_expr(cond, name)
                    + then_body
                        .iter()
                        .map(|stmt| count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
                    + else_body
                        .iter()
                        .map(|stmt| count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.iter()
                    .map(|stmt| count_var_reads_stmt(stmt, name))
                    .sum::<usize>()
                    + cond
                        .as_ref()
                        .map(|expr| count_var_reads_expr(expr, name))
                        .unwrap_or(0)
                    + update
                        .iter()
                        .map(|stmt| count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
                    + body
                        .iter()
                        .map(|stmt| count_var_reads_stmt(stmt, name))
                        .sum::<usize>()
            }
            DirStmt::Return(Some(expr)) => count_var_reads_expr(expr, name),
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(None)
            | DirStmt::Break
            | DirStmt::Continue => 0,
        }
    }
