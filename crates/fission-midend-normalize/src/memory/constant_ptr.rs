use crate::prelude::*;
use crate::pipeline::GLOBAL_SYMBOL_CONTEXT;
use crate::pipeline::GlobalSymbolContext;

pub fn apply_constant_ptr_recovery_pass(func: &mut DirFunction) -> bool {
    let mut context = None;
    GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        if let Some(c) = ctx.borrow().as_ref() {
            context = Some(c.clone());
        }
    });

    let Some(context) = context else {
        return false;
    };

    if context.names.is_empty() {
        return false;
    }

    let mut changed = false;
    process_statement_list(&mut func.body, &context, &mut changed);
    changed
}

fn process_statement_list(
    stmts: &mut [DirStmt],
    context: &GlobalSymbolContext,
    changed: &mut bool,
) {
    for stmt in stmts {
        process_stmt(stmt, context, changed);
    }
}

fn process_stmt(stmt: &mut DirStmt, context: &GlobalSymbolContext, changed: &mut bool) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            process_lvalue(lhs, context, changed);
            process_expr(rhs, context, changed);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            process_expr(expr, context, changed);
        }
        DirStmt::VaStart { va_list, .. } => {
            process_expr(va_list, context, changed);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            process_statement_list(body, context, changed);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            process_expr(cond, context, changed);
            process_statement_list(then_body, context, changed);
            process_statement_list(else_body, context, changed);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            process_expr(expr, context, changed);
            for case in cases {
                process_statement_list(&mut case.body, context, changed);
            }
            process_statement_list(default, context, changed);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                process_stmt(init, context, changed);
            }
            if let Some(cond) = cond {
                process_expr(cond, context, changed);
            }
            if let Some(update) = update {
                process_stmt(update, context, changed);
            }
            process_statement_list(body, context, changed);
        }
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn process_lvalue(lval: &mut DirLValue, context: &GlobalSymbolContext, changed: &mut bool) {
    match lval {
        DirLValue::Deref { ptr, .. } => {
            process_expr(ptr, context, changed);
        }
        DirLValue::Index { base, index, .. } => {
            process_expr(base, context, changed);
            process_expr(index, context, changed);
        }
        DirLValue::Var(_) => {}
        DirLValue::FieldAccess { base, .. } => {
            process_expr(base, context, changed);
        }
    }
}

fn process_expr(expr: &mut DirExpr, context: &GlobalSymbolContext, changed: &mut bool) {
    // Walk inner expressions first
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            process_expr(inner, context, changed);
        }
        DirExpr::Binary { lhs, rhs, .. }
        | DirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            process_expr(lhs, context, changed);
            process_expr(rhs, context, changed);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                process_expr(arg, context, changed);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            process_expr(cond, context, changed);
            process_expr(then_expr, context, changed);
            process_expr(else_expr, context, changed);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
    }

    // Rewrite constant pointers if applicable
    if let DirExpr::Const(val, _) = expr {
        let addr = *val as u64;
        if addr != 0 {
            if let Some((global_addr, global_name)) = find_global_symbol(addr, context) {
                if addr == global_addr {
                    *expr = DirExpr::AddressOfGlobal(global_name);
                } else {
                    let offset = (addr - global_addr) as i64;
                    *expr = DirExpr::PtrOffset {
                        base: Box::new(DirExpr::AddressOfGlobal(global_name)),
                        offset,
                    };
                }
                *changed = true;
            }
        }
    }
}

fn find_global_symbol(addr: u64, context: &GlobalSymbolContext) -> Option<(u64, String)> {
    // First check exact match
    if let Some(name) = context.names.get(&addr) {
        return Some((addr, name.clone()));
    }

    // Find all symbols containing addr
    let mut best_match: Option<(u64, String)> = None;
    for (&symbol_addr, name) in &context.names {
        if symbol_addr == 0 {
            continue;
        }
        let size = context.sizes.get(&symbol_addr).copied().unwrap_or(0);
        if size > 0 && addr > symbol_addr && addr < symbol_addr + size as u64 {
            match best_match {
                None => best_match = Some((symbol_addr, name.clone())),
                Some((best_addr, _)) => {
                    if symbol_addr > best_addr {
                        best_match = Some((symbol_addr, name.clone()));
                    }
                }
            }
        }
    }

    best_match
}
