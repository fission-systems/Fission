//! Pure HIR rewrite helpers used by guarded-tail promotion/execution.

use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

pub fn expr_contains_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(var) | PreHirExpr::AddressOfGlobal(var) => var == name,
        PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        PreHirExpr::Select {
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

pub fn lvalue_contains_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_contains_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_contains_var(base, name),
    }
}

pub fn replace_var_in_expr(expr: &mut PreHirExpr, name: &str, replacement: &PreHirExpr) {
    match expr {
        PreHirExpr::Var(var) if var == name => *expr = replacement.clone(),
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            replace_var_in_expr(expr, name, replacement);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirExpr::Select {
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

pub fn replace_var_in_lvalue(lhs: &mut PreHirLValue, name: &str, replacement: &PreHirExpr) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        PreHirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            replace_var_in_expr(base, name, replacement);
        }
    }
}

pub fn replace_var_in_stmt(stmt: &mut PreHirStmt, name: &str, replacement: &PreHirExpr) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        PreHirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        PreHirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Block(stmts) => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::DoWhile { body, cond } => {
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body) {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::For {
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
            for stmt in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body) {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        PreHirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub fn count_var_defs_stmt(stmt: &PreHirStmt, target: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, .. } => {
            usize::from(matches!(lhs, PreHirLValue::Var(name) if name == target))
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => stmts
            .iter()
            .map(|stmt| count_var_defs_stmt(stmt, target))
            .sum(),
        PreHirStmt::Switch { cases, default, .. } => {
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
        PreHirStmt::If {
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
        PreHirStmt::For {
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
        PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}

pub fn count_var_reads_expr(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(var) | PreHirExpr::AddressOfGlobal(var) => usize::from(var == name),
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => count_var_reads_expr(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_reads_expr(lhs, name) + count_var_reads_expr(rhs, name)
        }
        PreHirExpr::Call { args, .. } => {
            args.iter().map(|arg| count_var_reads_expr(arg, name)).sum()
        }
        PreHirExpr::Index { base, index, .. } => {
            count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
        }
        PreHirExpr::Select {
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

pub fn count_var_reads_lvalue(lhs: &PreHirLValue, name: &str) -> usize {
    match lhs {
        PreHirLValue::Var(_) => 0,
        PreHirLValue::Deref { ptr, .. } => count_var_reads_expr(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            count_var_reads_expr(base, name) + count_var_reads_expr(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => count_var_reads_expr(base, name),
    }
}

pub fn count_var_reads_stmt(stmt: &PreHirStmt, name: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            count_var_reads_lvalue(lhs, name) + count_var_reads_expr(rhs, name)
        }
        PreHirStmt::VaStart { va_list, .. } => count_var_reads_expr(va_list, name),
        PreHirStmt::Expr(expr) => count_var_reads_expr(expr, name),
        PreHirStmt::Block(stmts) | PreHirStmt::While { body: stmts, .. } => stmts
            .iter()
            .map(|stmt| count_var_reads_stmt(stmt, name))
            .sum(),
        PreHirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_reads_stmt(stmt, name))
                .sum::<usize>()
                + count_var_reads_expr(cond, name)
        }
        PreHirStmt::Switch {
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
        PreHirStmt::If {
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
        PreHirStmt::For {
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
        PreHirStmt::Return(Some(expr)) => count_var_reads_expr(expr, name),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => 0,
    }
}
