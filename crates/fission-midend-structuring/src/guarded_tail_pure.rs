//! Pure HIR rewrite helpers used by guarded-tail promotion/execution.

use fission_midend_core::ir::{HirExpr, HirLValue, HirStmt};

pub fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
        match expr {
            HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => var == name,
            HirExpr::Const(_, _) => false,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
            HirExpr::Binary { lhs, rhs, .. } => {
                expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
            }
            HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
            HirExpr::Index { base, index, .. } => {
                expr_contains_var(base, name) || expr_contains_var(index, name)
            }
            HirExpr::Select {
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

pub fn lvalue_contains_var(lhs: &HirLValue, name: &str) -> bool {
        match lhs {
            HirLValue::Var(_) => false,
            HirLValue::Deref { ptr, .. } => expr_contains_var(ptr, name),
            HirLValue::Index { base, index, .. } => {
                expr_contains_var(base, name) || expr_contains_var(index, name)
            }
            HirLValue::FieldAccess { base, .. } => expr_contains_var(base, name),
        }
    }

pub fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
        match expr {
            HirExpr::Var(var) if var == name => *expr = replacement.clone(),
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => {
                replace_var_in_expr(expr, name, replacement);
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                replace_var_in_expr(lhs, name, replacement);
                replace_var_in_expr(rhs, name, replacement);
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    replace_var_in_expr(arg, name, replacement);
                }
            }
            HirExpr::Index { base, index, .. } => {
                replace_var_in_expr(base, name, replacement);
                replace_var_in_expr(index, name, replacement);
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
        }
    }

pub fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
        match lhs {
            HirLValue::Var(_) => {}
            HirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
            HirLValue::Index { base, index, .. } => {
                replace_var_in_expr(base, name, replacement);
                replace_var_in_expr(index, name, replacement);
            }
            HirLValue::FieldAccess { base, .. } => {
                replace_var_in_expr(base, name, replacement);
            }
        }
    }

pub fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                replace_var_in_lvalue(lhs, name, replacement);
                replace_var_in_expr(rhs, name, replacement);
            }
            HirStmt::VaStart { va_list, .. } => {
                replace_var_in_expr(va_list, name, replacement)
            }
            HirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
            HirStmt::Block(stmts) => {
                for stmt in stmts {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::While { cond, body } => {
                replace_var_in_expr(cond, name, replacement);
                for stmt in body {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            HirStmt::DoWhile { body, cond } => {
                for stmt in body {
                    replace_var_in_stmt(stmt, name, replacement);
                }
                replace_var_in_expr(cond, name, replacement);
            }
            HirStmt::Switch {
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
            HirStmt::If {
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
            HirStmt::For {
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
            HirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

pub fn count_var_defs_stmt(stmt: &HirStmt, target: &str) -> usize {
        match stmt {
            HirStmt::Assign { lhs, .. } => {
                usize::from(matches!(lhs, HirLValue::Var(name) if name == target))
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| count_var_defs_stmt(stmt, target))
                .sum(),
            HirStmt::Switch { cases, default, .. } => {
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
            HirStmt::If {
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
            HirStmt::For {
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
            HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

pub fn count_var_reads_expr(expr: &HirExpr, name: &str) -> usize {
        match expr {
            HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
            HirExpr::Const(_, _) => 0,
            HirExpr::Cast { expr, .. }
            | HirExpr::Unary { expr, .. }
            | HirExpr::Load { ptr: expr, .. }
            | HirExpr::PtrOffset { base: expr, .. }
            | HirExpr::FieldAccess { base: expr, .. }
            | HirExpr::AggregateCopy { src: expr, .. } => count_var_reads_expr(expr, name),
            HirExpr::Binary { lhs, rhs, .. } => {
                count_var_reads_expr(lhs, name) + count_var_reads_expr(rhs, name)
            }
            HirExpr::Call { args, .. } => args
                .iter()
                .map(|arg| count_var_reads_expr(arg, name))
                .sum(),
            HirExpr::Index { base, index, .. } => {
                count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
            }
            HirExpr::Select {
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

pub fn count_var_reads_lvalue(lhs: &HirLValue, name: &str) -> usize {
        match lhs {
            HirLValue::Var(_) => 0,
            HirLValue::Deref { ptr, .. } => count_var_reads_expr(ptr, name),
            HirLValue::Index { base, index, .. } => {
                count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
            }
            HirLValue::FieldAccess { base, .. } => count_var_reads_expr(base, name),
        }
    }

pub fn count_var_reads_stmt(stmt: &HirStmt, name: &str) -> usize {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                count_var_reads_lvalue(lhs, name) + count_var_reads_expr(rhs, name)
            }
            HirStmt::VaStart { va_list, .. } => count_var_reads_expr(va_list, name),
            HirStmt::Expr(expr) => count_var_reads_expr(expr, name),
            HirStmt::Block(stmts) | HirStmt::While { body: stmts, .. } => stmts
                .iter()
                .map(|stmt| count_var_reads_stmt(stmt, name))
                .sum(),
            HirStmt::DoWhile { body, cond } => {
                body.iter()
                    .map(|stmt| count_var_reads_stmt(stmt, name))
                    .sum::<usize>()
                    + count_var_reads_expr(cond, name)
            }
            HirStmt::Switch {
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
            HirStmt::If {
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
            HirStmt::For {
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
            HirStmt::Return(Some(expr)) => count_var_reads_expr(expr, name),
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }
