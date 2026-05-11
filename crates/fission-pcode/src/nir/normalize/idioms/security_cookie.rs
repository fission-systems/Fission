use super::super::wave_stats::{add_call_signature_refinements, add_security_cookie_folds};
use super::super::*;

fn is_stack_pointer_name(name: &str) -> bool {
    matches!(name, "rsp" | "rbp" | "esp" | "ebp")
}

fn expr_uses_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => var == name,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_uses_var(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => expr_uses_var(lhs, name) || expr_uses_var(rhs, name),
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_uses_var(arg, name)),
        HirExpr::PtrOffset { base, .. } => expr_uses_var(base, name),
        HirExpr::Index { base, index, .. } => {
            expr_uses_var(base, name) || expr_uses_var(index, name)
        }
        HirExpr::Const(_, _) => false,
    }
}

fn is_cookie_seed_expr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } => {
            matches!(lhs.as_ref(), HirExpr::Var(name) if is_stack_pointer_name(name))
                || matches!(rhs.as_ref(), HirExpr::Var(name) if is_stack_pointer_name(name))
        }
        _ => false,
    }
}

fn refine_cookie_calls(
    stmts: &mut [HirStmt],
    cookie_vars: &[String],
    folds: &mut usize,
    renamed: &mut usize,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Expr(HirExpr::Call { target, args, .. })
            | HirStmt::Assign {
                rhs: HirExpr::Call { target, args, .. },
                ..
            } => {
                if args.len() == 1
                    && cookie_vars.iter().any(|name| expr_uses_var(&args[0], name))
                    && is_cookie_seed_expr(&args[0])
                {
                    if target.starts_with("sub_") || target.starts_with("FUN_") {
                        *target = "__security_check_cookie".to_string();
                        *renamed += 1;
                    }
                    *folds += 1;
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => refine_cookie_calls(body, cookie_vars, folds, renamed),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                refine_cookie_calls(then_body, cookie_vars, folds, renamed);
                refine_cookie_calls(else_body, cookie_vars, folds, renamed);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    refine_cookie_calls(&mut case.body, cookie_vars, folds, renamed);
                }
                refine_cookie_calls(default, cookie_vars, folds, renamed);
            }
            HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
            _ => {}
        }
    }
}

pub(crate) fn apply_security_cookie_pass(func: &mut HirFunction) -> bool {
    let mut cookie_vars = Vec::new();
    for stmt in &func.body {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
            && is_cookie_seed_expr(rhs)
        {
            cookie_vars.push(name.clone());
        }
    }
    if cookie_vars.is_empty() {
        return false;
    }
    let mut folds = 0usize;
    let mut renamed = 0usize;
    refine_cookie_calls(&mut func.body, &cookie_vars, &mut folds, &mut renamed);
    add_security_cookie_folds(folds);
    add_call_signature_refinements(renamed);
    renamed > 0
}
