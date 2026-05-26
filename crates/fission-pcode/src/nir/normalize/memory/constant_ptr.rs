use super::super::*;
use crate::nir::normalize::pipeline::GLOBAL_SYMBOL_CONTEXT;
use crate::nir::normalize::pipeline::GlobalSymbolContext;

pub(crate) fn apply_constant_ptr_recovery_pass(func: &mut HirFunction) -> bool {
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
    stmts: &mut [HirStmt],
    context: &GlobalSymbolContext,
    changed: &mut bool,
) {
    for stmt in stmts {
        process_stmt(stmt, context, changed);
    }
}

fn process_stmt(stmt: &mut HirStmt, context: &GlobalSymbolContext, changed: &mut bool) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            process_lvalue(lhs, context, changed);
            process_expr(rhs, context, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            process_expr(expr, context, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            process_expr(va_list, context, changed);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            process_statement_list(body, context, changed);
        }
        HirStmt::If { cond, then_body, else_body } => {
            process_expr(cond, context, changed);
            process_statement_list(then_body, context, changed);
            process_statement_list(else_body, context, changed);
        }
        HirStmt::Switch { expr, cases, default } => {
            process_expr(expr, context, changed);
            for case in cases {
                process_statement_list(&mut case.body, context, changed);
            }
            process_statement_list(default, context, changed);
        }
        HirStmt::For { init, cond, update, body } => {
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
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn process_lvalue(lval: &mut HirLValue, context: &GlobalSymbolContext, changed: &mut bool) {
    match lval {
        HirLValue::Deref { ptr, .. } => {
            process_expr(ptr, context, changed);
        }
        HirLValue::Index { base, index, .. } => {
            process_expr(base, context, changed);
            process_expr(index, context, changed);
        }
        HirLValue::Var(_) => {}
        HirLValue::FieldAccess { base, .. } => {
            process_expr(base, context, changed);
        }
    }
}

fn process_expr(expr: &mut HirExpr, context: &GlobalSymbolContext, changed: &mut bool) {
    // Walk inner expressions first
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            process_expr(inner, context, changed);
        }
        HirExpr::Binary { lhs, rhs, .. }
        | HirExpr::Index { base: lhs, index: rhs, .. } => {
            process_expr(lhs, context, changed);
            process_expr(rhs, context, changed);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                process_expr(arg, context, changed);
            }
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            process_expr(cond, context, changed);
            process_expr(then_expr, context, changed);
            process_expr(else_expr, context, changed);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }

    // Rewrite constant pointers if applicable
    if let HirExpr::Const(val, _) = expr {
        let addr = *val as u64;
        if addr != 0 {
            if let Some((global_addr, global_name)) = find_global_symbol(addr, context) {
                if addr == global_addr {
                    *expr = HirExpr::AddressOfGlobal(global_name);
                } else {
                    let offset = (addr - global_addr) as i64;
                    *expr = HirExpr::PtrOffset {
                        base: Box::new(HirExpr::AddressOfGlobal(global_name)),
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
