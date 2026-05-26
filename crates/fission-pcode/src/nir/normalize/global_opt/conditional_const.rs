use super::super::*;
use std::collections::{HashMap, HashSet};

/// Scans the structured HIR and propagates constant values within branch scopes
/// where they are constrained by conditions (e.g., `if (x == 5) { ... }`).
pub(crate) fn apply_conditional_const_pass(func: &mut HirFunction) -> bool {
    let mut binding_types = HashMap::new();
    for local in &func.locals {
        binding_types.insert(local.name.clone(), local.ty.clone());
    }
    for param in &func.params {
        binding_types.insert(param.name.clone(), param.ty.clone());
    }

    let mut env = HashMap::new();
    visit_stmts(&mut func.body, &mut env, &binding_types)
}

fn visit_stmts(
    stmts: &mut [HirStmt],
    env: &mut HashMap<String, HirExpr>,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        changed |= visit_stmt(stmt, env, binding_types);
    }
    changed
}

fn visit_stmt(
    stmt: &mut HirStmt,
    env: &mut HashMap<String, HirExpr>,
    binding_types: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // 1. Substitute in RHS
            changed |= substitute_expr(rhs, env);
            // 2. Substitute in LHS (indices / dereferences)
            changed |= substitute_lvalue(lhs, env);
            // 3. Invalidate written variable in env
            if let HirLValue::Var(name) = lhs {
                env.remove(name);
            }
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= substitute_expr(expr, env);
        }
        HirStmt::Block(body) => {
            changed |= visit_stmts(body, env, binding_types);
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            changed |= substitute_expr(cond, env);

            let mut loop_env = env.clone();
            let mut written = HashSet::new();
            collect_written_vars(body, &mut written);
            for v in written {
                loop_env.remove(&v);
            }
            changed |= visit_stmts(body, &mut loop_env, binding_types);
        }
        HirStmt::For { init, cond, update, body } => {
            let mut loop_env = env.clone();
            let mut written = HashSet::new();
            if let Some(i) = init {
                collect_written_vars(std::slice::from_ref(i.as_ref()), &mut written);
            }
            if let Some(u) = update {
                collect_written_vars(std::slice::from_ref(u.as_ref()), &mut written);
            }
            collect_written_vars(body, &mut written);
            for v in written {
                loop_env.remove(&v);
            }

            if let Some(i) = init {
                changed |= visit_stmt(i.as_mut(), &mut loop_env, binding_types);
            }
            if let Some(c) = cond {
                changed |= substitute_expr(c, &loop_env);
            }
            if let Some(u) = update {
                changed |= visit_stmt(u.as_mut(), &mut loop_env, binding_types);
            }
            changed |= visit_stmts(body, &mut loop_env, binding_types);
        }
        HirStmt::If { cond, then_body, else_body } => {
            changed |= substitute_expr(cond, env);

            let mut then_env = env.clone();
            let mut else_env = env.clone();

            extract_constraints(cond, true, &mut then_env);
            extract_constraints(cond, false, &mut else_env);

            changed |= visit_stmts(then_body, &mut then_env, binding_types);
            changed |= visit_stmts(else_body, &mut else_env, binding_types);
        }
        HirStmt::Switch { expr, cases, default } => {
            changed |= substitute_expr(expr, env);
            for case in cases {
                let mut case_env = env.clone();
                if let HirExpr::Var(x) = expr {
                    if case.values.len() == 1 {
                        let val = case.values[0];
                        if let Some(ty) = binding_types.get(x) {
                            case_env.insert(x.clone(), HirExpr::Const(val, ty.clone()));
                        }
                    }
                }
                changed |= visit_stmts(&mut case.body, &mut case_env, binding_types);
            }
            changed |= visit_stmts(default, env, binding_types);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= substitute_expr(va_list, env);
        }
        _ => {}
    }
    changed
}

fn substitute_expr(expr: &mut HirExpr, env: &HashMap<String, HirExpr>) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Var(name) => {
            if let Some(cst) = env.get(name) {
                *expr = cst.clone();
                changed = true;
            }
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= substitute_expr(inner, env);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= substitute_expr(lhs, env);
            changed |= substitute_expr(rhs, env);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= substitute_expr(arg, env);
            }
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            changed |= substitute_expr(cond, env);
            changed |= substitute_expr(then_expr, env);
            changed |= substitute_expr(else_expr, env);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= substitute_expr(base, env);
            changed |= substitute_expr(index, env);
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn substitute_lvalue(lval: &mut HirLValue, env: &HashMap<String, HirExpr>) -> bool {
    let mut changed = false;
    match lval {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            changed |= substitute_expr(ptr, env);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= substitute_expr(base, env);
            changed |= substitute_expr(index, env);
        }
        HirLValue::FieldAccess { base, .. } => {
            changed |= substitute_expr(base, env);
        }
    }
    changed
}

fn extract_constraints(
    cond: &HirExpr,
    is_then_branch: bool,
    env: &mut HashMap<String, HirExpr>,
) {
    match cond {
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } => {
            if is_then_branch {
                match (lhs.as_ref(), rhs.as_ref()) {
                    (HirExpr::Var(name), HirExpr::Const(val, ty)) => {
                        env.insert(name.clone(), HirExpr::Const(*val, ty.clone()));
                    }
                    (HirExpr::Const(val, ty), HirExpr::Var(name)) => {
                        env.insert(name.clone(), HirExpr::Const(*val, ty.clone()));
                    }
                    _ => {}
                }
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => {
            if !is_then_branch {
                match (lhs.as_ref(), rhs.as_ref()) {
                    (HirExpr::Var(name), HirExpr::Const(val, ty)) => {
                        env.insert(name.clone(), HirExpr::Const(*val, ty.clone()));
                    }
                    (HirExpr::Const(val, ty), HirExpr::Var(name)) => {
                        env.insert(name.clone(), HirExpr::Const(*val, ty.clone()));
                    }
                    _ => {}
                }
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            if is_then_branch {
                extract_constraints(lhs, is_then_branch, env);
                extract_constraints(rhs, is_then_branch, env);
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ..
        } => {
            if !is_then_branch {
                extract_constraints(lhs, is_then_branch, env);
                extract_constraints(rhs, is_then_branch, env);
            }
        }
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => {
            extract_constraints(expr, !is_then_branch, env);
        }
        _ => {}
    }
}

fn collect_written_vars(stmts: &[HirStmt], written: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                collect_written_vars_lvalue(lhs, written);
                collect_written_vars_expr(rhs, written);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_written_vars_expr(expr, written);
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_written_vars(body, written);
            }
            HirStmt::For { init, cond, update, body } => {
                if let Some(i) = init {
                    collect_written_vars(std::slice::from_ref(i.as_ref()), written);
                }
                if let Some(c) = cond {
                    collect_written_vars_expr(c, written);
                }
                if let Some(u) = update {
                    collect_written_vars(std::slice::from_ref(u.as_ref()), written);
                }
                collect_written_vars(body, written);
            }
            HirStmt::If { cond, then_body, else_body } => {
                collect_written_vars_expr(cond, written);
                collect_written_vars(then_body, written);
                collect_written_vars(else_body, written);
            }
            HirStmt::Switch { expr, cases, default } => {
                collect_written_vars_expr(expr, written);
                for case in cases {
                    collect_written_vars(&case.body, written);
                }
                collect_written_vars(default, written);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_written_vars_expr(va_list, written);
            }
            _ => {}
        }
    }
}

fn collect_written_vars_lvalue(lval: &HirLValue, written: &mut HashSet<String>) {
    match lval {
        HirLValue::Var(name) => {
            written.insert(name.clone());
        }
        HirLValue::Deref { ptr, .. } => {
            collect_written_vars_expr(ptr, written);
        }
        HirLValue::Index { base, index, .. } => {
            collect_written_vars_expr(base, written);
            collect_written_vars_expr(index, written);
        }
        HirLValue::FieldAccess { base, .. } => {
            collect_written_vars_expr(base, written);
        }
    }
}

fn collect_written_vars_expr(expr: &HirExpr, written: &mut HashSet<String>) {
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            collect_written_vars_expr(inner, written);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_written_vars_expr(lhs, written);
            collect_written_vars_expr(rhs, written);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_written_vars_expr(arg, written);
            }
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            collect_written_vars_expr(cond, written);
            collect_written_vars_expr(then_expr, written);
            collect_written_vars_expr(else_expr, written);
        }
        HirExpr::Index { base, index, .. } => {
            collect_written_vars_expr(base, written);
            collect_written_vars_expr(index, written);
        }
        _ => {}
    }
}
