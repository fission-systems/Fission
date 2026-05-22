use super::super::analysis::defuse::DefUseMap;
use super::super::*;
use super::utils::*;
use std::collections::HashMap;

pub(crate) fn collapse_loop_exit_alias_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx + 1 < stmts.len() {
        let Some(alias) = return_var_name(&stmts[idx + 1]).map(str::to_string) else {
            idx += 1;
            continue;
        };
        if count_uses_in_stmt_list(stmts, &alias) != 1 {
            idx += 1;
            continue;
        }
        if !loop_executes_before_exit_return(stmts, idx) {
            idx += 1;
            continue;
        }
        let Some(source) = loop_exit_alias_source(&stmts[idx], &alias) else {
            idx += 1;
            continue;
        };
        let source_expr = HirExpr::Var(source.clone());
        if remove_loop_exit_alias_assignment(&mut stmts[idx], &alias, &source) {
            stmts[idx + 1] = HirStmt::Return(Some(source_expr));
            changed = true;
        }
        idx += 1;
    }

    changed
}

fn return_var_name(stmt: &HirStmt) -> Option<&str> {
    match stmt {
        HirStmt::Return(Some(HirExpr::Var(name))) => Some(name.as_str()),
        _ => None,
    }
}

fn loop_executes_before_exit_return(stmts: &[HirStmt], loop_idx: usize) -> bool {
    match stmts.get(loop_idx) {
        Some(HirStmt::DoWhile { .. }) => true,
        Some(HirStmt::For { init, cond, .. }) => {
            for_loop_guard_proves_first_iteration(stmts, loop_idx, init.as_deref(), cond.as_ref())
        }
        _ => false,
    }
}

fn loop_exit_alias_source(stmt: &HirStmt, alias: &str) -> Option<String> {
    match stmt {
        HirStmt::DoWhile { body, cond } => loop_body_exit_alias_source(body, alias)
            .filter(|source| !expr_mentions_var(cond, alias) && !expr_mentions_var(cond, source)),
        HirStmt::For {
            update, body, cond, ..
        } => loop_body_exit_alias_source(body, alias).filter(|source| {
            cond.as_ref()
                .is_none_or(|cond| !expr_mentions_var(cond, alias))
                && update.as_deref().is_none_or(|update| {
                    !stmt_mentions_var(update, alias) && !stmt_assigns_var(update, source)
                })
        }),
        _ => None,
    }
}

fn loop_body_exit_alias_source(body: &[HirStmt], alias: &str) -> Option<String> {
    let mut match_idx = None;
    let mut match_source = None;

    for (idx, stmt) in body.iter().enumerate() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            rhs: HirExpr::Var(source),
        } = stmt
        {
            if lhs == alias && source != alias {
                if match_idx.is_some() {
                    return None;
                }
                match_idx = Some(idx);
                match_source = Some(source.clone());
            }
        } else if stmt_assigns_var(stmt, alias) {
            return None;
        }
    }

    let idx = match_idx?;
    let source = match_source?;
    if body[idx + 1..]
        .iter()
        .any(|stmt| stmt_assigns_var(stmt, &source) || stmt_mentions_var(stmt, alias))
    {
        return None;
    }
    Some(source)
}

fn remove_loop_exit_alias_assignment(stmt: &mut HirStmt, alias: &str, source: &str) -> bool {
    let body = match stmt {
        HirStmt::DoWhile { body, .. } | HirStmt::For { body, .. } => body,
        _ => return false,
    };
    let Some(idx) = body.iter().position(|stmt| {
        matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(lhs),
                rhs: HirExpr::Var(rhs),
            } if lhs == alias && rhs == source
        )
    }) else {
        return false;
    };
    body.remove(idx);
    true
}

fn for_loop_guard_proves_first_iteration(
    stmts: &[HirStmt],
    loop_idx: usize,
    init: Option<&HirStmt>,
    cond: Option<&HirExpr>,
) -> bool {
    let Some(exit_label) = stmts.get(loop_idx + 2).and_then(|stmt| match stmt {
        HirStmt::Label(label) => Some(label.as_str()),
        _ => None,
    }) else {
        return false;
    };
    let Some((_iv, bound)) = zero_based_less_than_bound(init, cond) else {
        return false;
    };

    stmts[..loop_idx].iter().any(|stmt| {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return false;
        };
        else_body.is_empty()
            && matches_single_goto(then_body, exit_label)
            && guard_excludes_zero_iteration(cond, &bound)
    })
}

fn zero_based_less_than_bound(
    init: Option<&HirStmt>,
    cond: Option<&HirExpr>,
) -> Option<(String, String)> {
    let HirStmt::Assign {
        lhs: HirLValue::Var(init_var),
        rhs,
    } = init?
    else {
        return None;
    };
    if expr_as_const_ignoring_casts(rhs) != Some(0) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Lt | HirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = cond?
    else {
        return None;
    };
    let cond_var = expr_as_var_ignoring_casts(lhs)?;
    if cond_var != init_var {
        return None;
    }
    let bound = expr_as_var_ignoring_casts(rhs)?;
    Some((init_var.clone(), bound.to_string()))
}

fn guard_excludes_zero_iteration(cond: &HirExpr, bound: &str) -> bool {
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return false;
    };
    let lhs_var = expr_as_var_ignoring_casts(lhs);
    let rhs_var = expr_as_var_ignoring_casts(rhs);
    let lhs_const = expr_as_const_ignoring_casts(lhs);
    let rhs_const = expr_as_const_ignoring_casts(rhs);

    matches!(
        (op, lhs_var, rhs_const),
        (HirBinaryOp::Le | HirBinaryOp::SLe, Some(var), Some(0)) if var == bound
    ) || matches!(
        (op, lhs_const, rhs_var),
        (HirBinaryOp::Ge | HirBinaryOp::SGe, Some(0), Some(var)) if var == bound
    )
}

fn expr_as_var_ignoring_casts(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => expr_as_var_ignoring_casts(expr),
        _ => None,
    }
}

fn expr_as_const_ignoring_casts(expr: &HirExpr) -> Option<i64> {
    match expr {
        HirExpr::Const(value, _) => Some(*value),
        HirExpr::Cast { expr, .. } => expr_as_const_ignoring_casts(expr),
        _ => None,
    }
}

fn matches_single_goto(body: &[HirStmt], label: &str) -> bool {
    matches!(body, [HirStmt::Goto(target)] if target == label)
}

pub(crate) fn inline_loop_condition_trailing_temps(
    func: &mut HirFunction,
) -> bool {
    let mut changed = false;
    for _ in 0..8 {
        let use_count = DefUseMap::build(&func.body).use_count;
        if !inline_loop_condition_trailing_temps_in_stmts(&mut func.body, &use_count) {
            break;
        }
        changed = true;
    }
    changed
}

fn inline_loop_condition_trailing_temps_in_stmts(
    stmts: &mut Vec<HirStmt>,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            HirStmt::DoWhile { body, cond } => {
                changed |= inline_trailing_temps_into_condition(body, cond, read_counts);
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::While { body, .. } | HirStmt::Block(body) => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init
                    && let HirStmt::Block(body) = init.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
                }
                if let Some(update) = update
                    && let HirStmt::Block(body) = update.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(then_body, read_counts);
                changed |= inline_loop_condition_trailing_temps_in_stmts(else_body, read_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        inline_loop_condition_trailing_temps_in_stmts(&mut case.body, read_counts);
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(default, read_counts);
            }
            _ => {}
        }
    }
    changed
}

fn inline_trailing_temps_into_condition(
    body: &mut Vec<HirStmt>,
    cond: &mut HirExpr,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    loop {
        let Some(HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        }) = body.last()
        else {
            break;
        };
        if !is_trivial_temp_name(name)
            || expr_has_side_effects(rhs)
            || !expr_is_low_cost_inline_candidate(rhs)
            || expr_mentions_var(rhs, name)
        {
            break;
        }
        let cond_uses = count_var_uses(cond, name);
        if cond_uses == 0 || read_counts.get(name).copied().unwrap_or(0) != cond_uses {
            break;
        }
        let replacement = rhs.clone();
        replace_var_in_expr(cond, name, &replacement);
        body.pop();
        changed = true;
    }
    changed
}

fn expr_is_low_cost_inline_candidate(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
        HirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            expr_is_low_cost_inline_candidate(expr)
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
                    | HirBinaryOp::And
                    | HirBinaryOp::Or
                    | HirBinaryOp::Xor
                    | HirBinaryOp::Add
                    | HirBinaryOp::Sub
                    | HirBinaryOp::Shl
                    | HirBinaryOp::Shr
                    | HirBinaryOp::Sar
                    | HirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

pub(crate) fn collapse_redundant_conditional_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some(HirStmt::If {
            cond,
            then_body,
            else_body,
        }) = stmts.get(idx)
        else {
            rewritten.push(stmts[idx].clone());
            idx += 1;
            continue;
        };

        let then_ret = single_return_stmt(then_body);
        let else_ret = single_return_stmt(else_body);

        if let (Some(then_ret), Some(else_ret)) = (then_ret.clone(), else_ret.clone())
            && then_ret == else_ret
        {
            changed = true;
            if expr_has_side_effects(cond) {
                rewritten.push(HirStmt::Expr(cond.clone()));
            }
            rewritten.push(then_ret);
            idx += 1;
            continue;
        }

        if let Some(next_ret) = stmts.get(idx + 1).and_then(as_return_stmt) {
            let then_matches_next =
                then_ret.as_ref().is_some_and(|ret| ret == next_ret) && else_body.is_empty();
            let else_matches_next =
                else_ret.as_ref().is_some_and(|ret| ret == next_ret) && then_body.is_empty();
            if then_matches_next || else_matches_next {
                changed = true;
                if expr_has_side_effects(cond) {
                    rewritten.push(HirStmt::Expr(cond.clone()));
                }
                idx += 1;
                continue;
            }
        }

        rewritten.push(stmts[idx].clone());
        idx += 1;
    }

    if changed {
        *stmts = rewritten;
    }
    changed
}

fn as_return_stmt(stmt: &HirStmt) -> Option<&HirStmt> {
    matches!(stmt, HirStmt::Return(_)).then_some(stmt)
}

fn return_expr(stmt: &HirStmt) -> Option<&HirExpr> {
    match stmt {
        HirStmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

fn single_return_stmt(body: &[HirStmt]) -> Option<HirStmt> {
    match body {
        [HirStmt::Return(expr)] => Some(HirStmt::Return(expr.clone())),
        _ => None,
    }
}

fn single_return_expr(body: &[HirStmt]) -> Option<&HirExpr> {
    match body {
        [HirStmt::Return(Some(expr))] => Some(expr),
        _ => None,
    }
}

fn if_parts(stmt: &HirStmt) -> Option<(&HirExpr, &[HirStmt], &[HirStmt])> {
    match stmt {
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => Some((cond, then_body, else_body)),
        _ => None,
    }
}

fn binary_comparison_parts(
    expr: &HirExpr,
) -> Option<(HirBinaryOp, &Box<HirExpr>, &Box<HirExpr>, &NirType)> {
    match expr {
        HirExpr::Binary {
            op:
                op @ (HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::Gt
                | HirBinaryOp::Ge
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe
                | HirBinaryOp::SGt
                | HirBinaryOp::SGe),
            lhs,
            rhs,
            ty,
        } => Some((*op, lhs, rhs, ty)),
        _ => None,
    }
}

fn minmax_branch_swap_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    match op {
        HirBinaryOp::Lt | HirBinaryOp::Le => Some(HirBinaryOp::Gt),
        HirBinaryOp::Gt | HirBinaryOp::Ge => Some(HirBinaryOp::Lt),
        HirBinaryOp::SLt | HirBinaryOp::SLe => Some(HirBinaryOp::SGt),
        HirBinaryOp::SGt | HirBinaryOp::SGe => Some(HirBinaryOp::SLt),
        _ => None,
    }
}

pub(crate) fn canonicalize_minmax_conditional_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx + 1 < stmts.len() {
        let Some((cond, then_body, else_body)) = if_parts(&stmts[idx]) else {
            idx += 1;
            continue;
        };
        if !else_body.is_empty() {
            idx += 1;
            continue;
        }
        let Some(then_expr) = single_return_expr(then_body) else {
            idx += 1;
            continue;
        };
        let Some(next_expr) = return_expr(&stmts[idx + 1]) else {
            idx += 1;
            continue;
        };
        let Some((op, lhs, rhs, ty)) = binary_comparison_parts(cond) else {
            idx += 1;
            continue;
        };
        if expr_has_side_effects(lhs) || expr_has_side_effects(rhs) {
            idx += 1;
            continue;
        }

        let Some(new_op) = minmax_branch_swap_op(op) else {
            idx += 1;
            continue;
        };
        if then_expr != rhs.as_ref() || next_expr != lhs.as_ref() {
            idx += 1;
            continue;
        }
        let lhs_expr = (**lhs).clone();
        let rhs_expr = (**rhs).clone();
        let cond_ty = ty.clone();

        stmts[idx] = HirStmt::If {
            cond: HirExpr::Binary {
                op: new_op,
                lhs: Box::new(lhs_expr.clone()),
                rhs: Box::new(rhs_expr.clone()),
                ty: cond_ty,
            },
            then_body: vec![HirStmt::Return(Some(lhs_expr))],
            else_body: Vec::new(),
        };
        stmts[idx + 1] = HirStmt::Return(Some(rhs_expr));
        changed = true;
        idx += 2;
    }

    changed
}
