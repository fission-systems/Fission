use super::super::for_loops::stmt_list_contains_continue_pub;
use super::loop_expressions::{index_type_for_count, substitute_var_in_expr};
use super::loop_latch::{parse_break_eq_var, parse_self_add_assign, tail_meaningful_stmts};
use super::recurrences::{is_loop_invariant, loop_variant_vars};
use super::variable_uses::fresh_index_name;
use crate::prelude::*;

/// MSVC-style nested-loop outer latch sometimes merges two induction variables
/// (row counter `+= 1` and row offset `+= cols`) onto one temp after copy chains.
/// Pattern at infinite-`for` tail:
///   `v = v + 1; v = v + K; if (limit == v) break;`
/// Recover by splitting the `+1` update and exit compare onto a fresh row counter.
pub(super) fn try_split_merged_dual_iv_tail(
    stmts: &mut Vec<PreHirStmt>,
    loop_idx: usize,
    locals: &mut Vec<PreHirBinding>,
) -> bool {
    let body = match &stmts[loop_idx] {
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        } => body.clone(),
        _ => return false,
    };

    let Some(tail) = tail_meaningful_stmts(&body, 3) else {
        return false;
    };
    let (_, break_stmt) = &tail[2];
    let (_, step_k_stmt) = &tail[1];
    let (_, step_one_stmt) = &tail[0];

    let (limit_expr, compare_var) = match parse_break_eq_var(break_stmt) {
        Some(v) => v,
        None => return false,
    };
    let (var_one, add_one) = match parse_self_add_assign(step_one_stmt) {
        Some(v) => v,
        None => return false,
    };
    let (var_k, add_k) = match parse_self_add_assign(step_k_stmt) {
        Some(v) => v,
        None => return false,
    };
    if var_one != var_k || var_one != compare_var {
        return false;
    }
    if !matches!(strip_casts(&add_one), PreHirExpr::Const(1, _)) {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);
    if !is_loop_invariant(&add_k, &loop_variant) {
        return false;
    }
    if stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let row_name = fresh_index_name(locals, stmts);
    let row_ty = index_type_for_count(&limit_expr);
    locals.push(PreHirBinding {
        name: row_name.clone(),
        ty: row_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });

    let mut new_body = body;
    substitute_var_in_stmt(
        &mut std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut new_body)[tail[0].0],
        &var_one,
        &row_name,
        false,
    );
    substitute_var_in_stmt(
        &mut std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut new_body)[tail[2].0],
        &var_one,
        &row_name,
        false,
    );

    stmts[loop_idx] = PreHirStmt::For {
        init: None,
        cond: None,
        update: None,
        body: new_body,
    };

    let init_stmt = PreHirStmt::Assign {
        lhs: PreHirLValue::Var(row_name),
        rhs: PreHirExpr::Const(0, row_ty),
    };
    stmts.insert(loop_idx, init_stmt);
    true
}

/// Replace `var` with `replacement` in `stmt`. When `assign_lhs_only`, only rewrite
/// the assignment target (for splitting a shared temp across dual IV updates).
fn substitute_var_in_stmt(
    stmt: &mut PreHirStmt,
    var: &str,
    replacement: &str,
    assign_lhs_only: bool,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            if let PreHirLValue::Var(name) = lhs {
                if name == var {
                    *name = replacement.to_string();
                }
            }
            if !assign_lhs_only {
                substitute_var_in_expr(rhs, var, &PreHirExpr::Var(replacement.to_string()));
            }
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            substitute_var_in_expr(cond, var, &PreHirExpr::Var(replacement.to_string()));
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body)
                .iter_mut()
                .chain(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body).iter_mut())
            {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).iter_mut() {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body).iter_mut() {
                    substitute_var_in_stmt(s, var, replacement, false);
                }
            }
            for s in std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default).iter_mut() {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            substitute_var_in_expr(expr, var, &PreHirExpr::Var(replacement.to_string()));
        }
        _ => {}
    }
}
