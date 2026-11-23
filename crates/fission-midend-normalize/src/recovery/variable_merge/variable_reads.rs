use crate::HashSet;
use crate::prelude::*;

pub(super) fn collect_read_vars(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut vars = HashSet::default();
    collect_read_vars_in_stmts(stmts, &mut vars);
    vars
}

pub(super) fn collect_cooccurring_var_pairs(stmts: &[PreHirStmt]) -> HashSet<(String, String)> {
    let mut pairs = HashSet::default();
    collect_cooccurring_var_pairs_in_stmts(stmts, &mut pairs);
    pairs
}

fn collect_cooccurring_var_pairs_in_stmts(
    stmts: &[PreHirStmt],
    pairs: &mut HashSet<(String, String)>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_cooccurring_var_pairs_in_lvalue(lhs, pairs);
                collect_cooccurring_var_pairs_in_expr(rhs, pairs);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_cooccurring_var_pairs_in_expr(expr, pairs);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_cooccurring_var_pairs_in_stmts(body, pairs);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_cooccurring_var_pairs_in_expr(cond, pairs);
                collect_cooccurring_var_pairs_in_stmts(then_body, pairs);
                collect_cooccurring_var_pairs_in_stmts(else_body, pairs);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_cooccurring_var_pairs_in_stmts(std::slice::from_ref(init), pairs);
                }
                if let Some(cond) = cond {
                    collect_cooccurring_var_pairs_in_expr(cond, pairs);
                }
                if let Some(update) = update {
                    collect_cooccurring_var_pairs_in_stmts(std::slice::from_ref(update), pairs);
                }
                collect_cooccurring_var_pairs_in_stmts(body, pairs);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_cooccurring_var_pairs_in_expr(expr, pairs);
                for case in cases {
                    collect_cooccurring_var_pairs_in_stmts(&case.body, pairs);
                }
                collect_cooccurring_var_pairs_in_stmts(default, pairs);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_cooccurring_var_pairs_in_expr(va_list, pairs);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_cooccurring_var_pairs_in_lvalue(
    lval: &PreHirLValue,
    pairs: &mut HashSet<(String, String)>,
) {
    match lval {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => collect_cooccurring_var_pairs_in_expr(ptr, pairs),
        PreHirLValue::Index { base, index, .. } => {
            collect_cooccurring_var_pairs_in_expr(base, pairs);
            collect_cooccurring_var_pairs_in_expr(index, pairs);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_cooccurring_var_pairs_in_expr(base, pairs)
        }
    }
}

fn collect_cooccurring_var_pairs_in_expr(expr: &PreHirExpr, pairs: &mut HashSet<(String, String)>) {
    let mut vars = HashSet::default();
    collect_vars_in_expr(expr, &mut vars);
    let mut vars = vars.into_iter().collect::<Vec<_>>();
    vars.sort();
    for i in 0..vars.len() {
        for j in (i + 1)..vars.len() {
            pairs.insert((vars[i].clone(), vars[j].clone()));
        }
    }

    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_cooccurring_var_pairs_in_expr(inner, pairs);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_cooccurring_var_pairs_in_expr(lhs, pairs);
            collect_cooccurring_var_pairs_in_expr(rhs, pairs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_cooccurring_var_pairs_in_expr(arg, pairs);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_cooccurring_var_pairs_in_expr(cond, pairs);
            collect_cooccurring_var_pairs_in_expr(then_expr, pairs);
            collect_cooccurring_var_pairs_in_expr(else_expr, pairs);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_cooccurring_var_pairs_in_expr(base, pairs);
            collect_cooccurring_var_pairs_in_expr(index, pairs);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::Const(_, _)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_) => {}
    }
}

fn collect_read_vars_in_stmts(stmts: &[PreHirStmt], vars: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                collect_read_vars_in_lvalue(lhs, vars);
                collect_vars_in_expr(rhs, vars);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_vars_in_expr(expr, vars)
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                collect_read_vars_in_stmts(body, vars);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_vars_in_expr(cond, vars);
                collect_read_vars_in_stmts(then_body, vars);
                collect_read_vars_in_stmts(else_body, vars);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    collect_read_vars_in_stmts(std::slice::from_ref(init), vars);
                }
                if let Some(cond) = cond {
                    collect_vars_in_expr(cond, vars);
                }
                if let Some(update) = update {
                    collect_read_vars_in_stmts(std::slice::from_ref(update), vars);
                }
                collect_read_vars_in_stmts(body, vars);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_vars_in_expr(expr, vars);
                for case in cases {
                    collect_read_vars_in_stmts(&case.body, vars);
                }
                collect_read_vars_in_stmts(default, vars);
            }
            PreHirStmt::VaStart { va_list, .. } => collect_vars_in_expr(va_list, vars),
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_read_vars_in_lvalue(lval: &PreHirLValue, vars: &mut HashSet<String>) {
    match lval {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => collect_vars_in_expr(ptr, vars),
        PreHirLValue::Index { base, index, .. } => {
            collect_vars_in_expr(base, vars);
            collect_vars_in_expr(index, vars);
        }
        PreHirLValue::FieldAccess { base, .. } => collect_vars_in_expr(base, vars),
    }
}

pub(super) fn collect_vars_in_expr(expr: &PreHirExpr, vars: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfLocal(name) => {
            vars.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => collect_vars_in_expr(expr, vars),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, vars);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_vars_in_expr(cond, vars);
            collect_vars_in_expr(then_expr, vars);
            collect_vars_in_expr(else_expr, vars);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(base, vars);
            collect_vars_in_expr(index, vars);
        }
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}
