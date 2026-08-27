use super::super::analysis::defuse::DefUseMap;
use super::utils::*;
use crate::prelude::*;
use crate::{HashMap, HashSet};

pub fn collapse_loop_exit_alias_returns(stmts: &mut Vec<PreHirStmt>) -> bool {
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
        let source_expr = PreHirExpr::Var(source.clone());
        if remove_loop_exit_alias_assignment(&mut stmts[idx], &alias, &source) {
            stmts[idx + 1] = PreHirStmt::Return(Some(source_expr));
            changed = true;
        }
        idx += 1;
    }

    changed
}

pub fn recover_guarded_loop_tail_accumulator_returns(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx + 3 < stmts.len() {
        let Some(replacement) = guarded_loop_tail_replacement(stmts, idx) else {
            changed |= recover_guarded_loop_tail_accumulator_returns_in_stmt(&mut stmts[idx]);
            idx += 1;
            continue;
        };
        let end = replacement.end;
        stmts.splice(
            idx..end,
            [
                PreHirStmt::While {
                    cond: replacement.cond,
                    body: replacement.body.into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var(replacement.accumulator))),
            ],
        );
        changed = true;
        idx += 2;
    }
    while idx < stmts.len() {
        changed |= recover_guarded_loop_tail_accumulator_returns_in_stmt(&mut stmts[idx]);
        idx += 1;
    }
    changed
}

struct GuardedLoopTailReplacement {
    cond: PreHirExpr,
    body: Vec<PreHirStmt>,
    accumulator: String,
    end: usize,
}

fn guarded_loop_tail_replacement(
    stmts: &[PreHirStmt],
    idx: usize,
) -> Option<GuardedLoopTailReplacement> {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmts.get(idx)?
    else {
        return None;
    };
    if !else_body.is_empty() {
        return None;
    }
    let [PreHirStmt::Goto(label)] = then_body.as_slice() else {
        return None;
    };
    let stale_temp = return_var_name(stmts.get(idx + 1)?)?.to_string();
    let PreHirStmt::Label(body_label) = stmts.get(idx + 2)? else {
        return None;
    };
    if body_label != label {
        return None;
    }
    let end = next_label_or_end(stmts, idx + 3);
    let body = stmts[idx + 3..end].to_vec();
    if body.is_empty() {
        return None;
    }
    let accumulator = accumulator_updated_from_temp(&body, &stale_temp)?;
    if !stmts[..idx]
        .iter()
        .any(|stmt| stmt_assigns_var(stmt, &accumulator))
    {
        return None;
    }
    let cond_vars = vars_in_expr(cond);
    if cond_vars.is_empty()
        || !cond_vars
            .iter()
            .any(|name| body.iter().any(|stmt| stmt_assigns_var(stmt, name)))
    {
        return None;
    }
    let stale_def_idx = body
        .iter()
        .position(|stmt| matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(lhs), .. } if lhs == &stale_temp))?;
    let acc_update_idx = body
        .iter()
        .position(|stmt| accumulator_update_stmt(stmt, &stale_temp).is_some())?;
    if stale_def_idx >= acc_update_idx {
        return None;
    }
    Some(GuardedLoopTailReplacement {
        cond: cond.clone(),
        body,
        accumulator,
        end,
    })
}

fn recover_guarded_loop_tail_accumulator_returns_in_stmt(stmt: &mut PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => recover_guarded_loop_tail_accumulator_returns(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
        ),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(init) = init
                && let PreHirStmt::Block(stmts) = init.as_mut()
            {
                changed |=
                    recover_guarded_loop_tail_accumulator_returns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                    );
            }
            if let Some(update) = update
                && let PreHirStmt::Block(stmts) = update.as_mut()
            {
                changed |=
                    recover_guarded_loop_tail_accumulator_returns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                    );
            }
            changed
                | recover_guarded_loop_tail_accumulator_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                )
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            recover_guarded_loop_tail_accumulator_returns(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                then_body,
            )) | recover_guarded_loop_tail_accumulator_returns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
            )
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |=
                    recover_guarded_loop_tail_accumulator_returns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    );
            }
            changed
                | recover_guarded_loop_tail_accumulator_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                )
        }
        _ => false,
    }
}

fn next_label_or_end(stmts: &[PreHirStmt], start: usize) -> usize {
    stmts[start..]
        .iter()
        .position(|stmt| matches!(stmt, PreHirStmt::Label(_)))
        .map(|offset| start + offset)
        .unwrap_or(stmts.len())
}

fn accumulator_updated_from_temp(body: &[PreHirStmt], temp: &str) -> Option<String> {
    let mut acc = None;
    for stmt in body {
        let Some(candidate) = accumulator_update_stmt(stmt, temp) else {
            continue;
        };
        if acc.is_some() {
            return None;
        }
        acc = Some(candidate);
    }
    acc
}

fn accumulator_update_stmt(stmt: &PreHirStmt, temp: &str) -> Option<String> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs),
        rhs:
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
                lhs: bin_lhs,
                rhs: bin_rhs,
                ..
            },
    } = stmt
    else {
        return None;
    };
    let left = expr_as_var_ignoring_casts(bin_lhs)?;
    let right = expr_as_var_ignoring_casts(bin_rhs)?;
    if left == lhs && right == temp {
        Some(lhs.clone())
    } else if right == lhs && left == temp {
        Some(lhs.clone())
    } else {
        None
    }
}

fn vars_in_expr(expr: &PreHirExpr) -> HashSet<String> {
    let mut vars = HashSet::default();
    collect_vars_in_expr(expr, &mut vars);
    vars
}

fn collect_vars_in_expr(expr: &PreHirExpr, vars: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            vars.insert(name.clone());
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => collect_vars_in_expr(expr, vars),
        PreHirExpr::Binary { lhs, rhs, .. }
        | PreHirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
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
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, vars);
            }
        }
    }
}

fn return_var_name(stmt: &PreHirStmt) -> Option<&str> {
    match stmt {
        PreHirStmt::Return(Some(PreHirExpr::Var(name))) => Some(name.as_str()),
        _ => None,
    }
}

fn loop_executes_before_exit_return(stmts: &[PreHirStmt], loop_idx: usize) -> bool {
    match stmts.get(loop_idx) {
        Some(PreHirStmt::DoWhile { .. }) => true,
        Some(PreHirStmt::For { init, cond, .. }) => {
            for_loop_guard_proves_first_iteration(stmts, loop_idx, init.as_deref(), cond.as_ref())
        }
        _ => false,
    }
}

fn loop_exit_alias_source(stmt: &PreHirStmt, alias: &str) -> Option<String> {
    match stmt {
        PreHirStmt::DoWhile { body, cond } => loop_body_exit_alias_source(body, alias)
            .filter(|source| !expr_mentions_var(cond, alias) && !expr_mentions_var(cond, source)),
        PreHirStmt::For {
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

fn loop_body_exit_alias_source(body: &[PreHirStmt], alias: &str) -> Option<String> {
    let mut match_idx = None;
    let mut match_source = None;

    for (idx, stmt) in body.iter().enumerate() {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            rhs: PreHirExpr::Var(source),
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

fn remove_loop_exit_alias_assignment(stmt: &mut PreHirStmt, alias: &str, source: &str) -> bool {
    let body = match stmt {
        PreHirStmt::DoWhile { body, .. } | PreHirStmt::For { body, .. } => body,
        _ => return false,
    };
    let Some(idx) = body.iter().position(|stmt| {
        matches!(
            stmt,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs),
                rhs: PreHirExpr::Var(rhs),
            } if lhs == alias && rhs == source
        )
    }) else {
        return false;
    };
    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).remove(idx);
    true
}

fn for_loop_guard_proves_first_iteration(
    stmts: &[PreHirStmt],
    loop_idx: usize,
    init: Option<&PreHirStmt>,
    cond: Option<&PreHirExpr>,
) -> bool {
    let Some(exit_label) = stmts.get(loop_idx + 2).and_then(|stmt| match stmt {
        PreHirStmt::Label(label) => Some(label.as_str()),
        _ => None,
    }) else {
        return false;
    };
    let Some((_iv, bound)) = zero_based_less_than_bound(init, cond) else {
        return false;
    };

    stmts[..loop_idx].iter().any(|stmt| {
        let PreHirStmt::If {
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
    init: Option<&PreHirStmt>,
    cond: Option<&PreHirExpr>,
) -> Option<(String, String)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(init_var),
        rhs,
    } = init?
    else {
        return None;
    };
    if expr_as_const_ignoring_casts(rhs) != Some(0) {
        return None;
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Lt | PreHirBinaryOp::SLt,
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

fn guard_excludes_zero_iteration(cond: &PreHirExpr, bound: &str) -> bool {
    let PreHirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return false;
    };
    let lhs_var = expr_as_var_ignoring_casts(lhs);
    let rhs_var = expr_as_var_ignoring_casts(rhs);
    let lhs_const = expr_as_const_ignoring_casts(lhs);
    let rhs_const = expr_as_const_ignoring_casts(rhs);

    matches!(
        (op, lhs_var, rhs_const),
        (PreHirBinaryOp::Le | PreHirBinaryOp::SLe, Some(var), Some(0)) if var == bound
    ) || matches!(
        (op, lhs_const, rhs_var),
        (PreHirBinaryOp::Ge | PreHirBinaryOp::SGe, Some(0), Some(var)) if var == bound
    )
}

fn expr_as_var_ignoring_casts(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name) => Some(name.as_str()),
        PreHirExpr::Cast { expr, .. } => expr_as_var_ignoring_casts(expr),
        _ => None,
    }
}

fn expr_as_const_ignoring_casts(expr: &PreHirExpr) -> Option<i64> {
    match expr {
        PreHirExpr::Const(value, _) => Some(*value),
        PreHirExpr::Cast { expr, .. } => expr_as_const_ignoring_casts(expr),
        _ => None,
    }
}

fn matches_single_goto(body: &[PreHirStmt], label: &str) -> bool {
    matches!(body, [PreHirStmt::Goto(target)] if target == label)
}

pub fn inline_loop_condition_trailing_temps(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    for _ in 0..8 {
        let use_count = DefUseMap::build(&func.body).use_count;
        let leading_changed = inline_loop_header_break_temps_in_stmts(&mut func.body, &use_count);
        let trailing_changed =
            inline_loop_condition_trailing_temps_in_stmts(&mut func.body, &use_count);
        if !leading_changed && !trailing_changed {
            break;
        }
        changed = true;
    }
    changed
}

/// Promote a one-use loop-header temporary followed by a break guard into the
/// loop condition. The RHS may contain a load because it is moved, not copied;
/// its evaluation count and order therefore remain unchanged.
fn inline_loop_header_break_temps_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::While { cond, body } => {
                changed |= try_inline_loop_header_break_temp(cond, body, read_counts);
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::DoWhile { body, .. } | PreHirStmt::Block(body) => {
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init
                    && let PreHirStmt::Block(init_body) = init.as_mut()
                {
                    changed |= inline_loop_header_break_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(init_body),
                        read_counts,
                    );
                }
                if let Some(update) = update
                    && let PreHirStmt::Block(update_body) = update.as_mut()
                {
                    changed |= inline_loop_header_break_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(update_body),
                        read_counts,
                    );
                }
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    read_counts,
                );
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    read_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_loop_header_break_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        read_counts,
                    );
                }
                changed |= inline_loop_header_break_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    read_counts,
                );
            }
            _ => {}
        }
    }
    changed
}

fn try_inline_loop_header_break_temp(
    loop_cond: &mut PreHirExpr,
    body: &mut std::rc::Rc<Vec<PreHirStmt>>,
    read_counts: &HashMap<String, usize>,
) -> bool {
    if !matches!(loop_cond, PreHirExpr::Const(value, _) if *value != 0) {
        return false;
    }
    let body_ref = body.as_ref();
    let guard_idx = body_ref
        .iter()
        .position(|stmt| !matches!(stmt, PreHirStmt::Assign { .. }))
        .unwrap_or(body_ref.len());
    if guard_idx == 0 || guard_idx > 8 {
        return false;
    }
    let Some(PreHirStmt::If {
        cond: break_cond,
        then_body,
        else_body,
    }) = body_ref.get(guard_idx)
    else {
        return false;
    };
    if !matches!(then_body.as_slice(), [PreHirStmt::Break]) || !else_body.is_empty() {
        return false;
    }

    let prefix = &body_ref[..guard_idx];
    let mut assigned_names = HashSet::default();
    let mut resolved_values = HashMap::default();
    let mut original_memory_reads = expr_memory_read_count(break_cond);
    for stmt in prefix {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = stmt
        else {
            return false;
        };
        if !is_trivial_temp_name(name) || expr_has_side_effects(rhs) {
            return false;
        }
        let resolved_rhs = resolve_loop_header_expr(rhs, &resolved_values);
        if !expr_is_loop_header_inline_candidate(&resolved_rhs, 16) {
            return false;
        }
        original_memory_reads += expr_memory_read_count(rhs);
        assigned_names.insert(name.clone());
        resolved_values.insert(name.clone(), resolved_rhs);
    }

    for name in &assigned_names {
        let prefix_reads = prefix
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum::<usize>();
        let owned_reads = prefix_reads + count_var_uses(break_cond, name);
        if read_counts.get(name).copied().unwrap_or(0) != owned_reads {
            return false;
        }
    }

    // More than one memory read would lose the explicit sequencing supplied
    // by the prefix assignments. One read can be moved to the condition at
    // exactly the same control-flow point without duplication or reordering.
    if original_memory_reads > 1 {
        return false;
    }

    let promoted_cond = resolve_loop_header_expr(break_cond, &resolved_values);
    if promoted_cond == *break_cond
        || expr_memory_read_count(&promoted_cond) != original_memory_reads
        || !expr_is_loop_header_inline_candidate(&promoted_cond, 16)
    {
        return false;
    }
    *loop_cond = negate_expr(promoted_cond);
    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body).drain(..=guard_idx);
    true
}

fn resolve_loop_header_expr(expr: &PreHirExpr, values: &HashMap<String, PreHirExpr>) -> PreHirExpr {
    match expr {
        PreHirExpr::Var(name) => values.get(name).cloned().unwrap_or_else(|| expr.clone()),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => expr.clone(),
        PreHirExpr::Cast { ty, expr } => PreHirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(resolve_loop_header_expr(expr, values)),
        },
        PreHirExpr::Unary { op, expr, ty } => PreHirExpr::Unary {
            op: *op,
            expr: Box::new(resolve_loop_header_expr(expr, values)),
            ty: ty.clone(),
        },
        PreHirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: *op,
            lhs: Box::new(resolve_loop_header_expr(lhs, values)),
            rhs: Box::new(resolve_loop_header_expr(rhs, values)),
            ty: ty.clone(),
        },
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => PreHirExpr::Select {
            cond: Box::new(resolve_loop_header_expr(cond, values)),
            then_expr: Box::new(resolve_loop_header_expr(then_expr, values)),
            else_expr: Box::new(resolve_loop_header_expr(else_expr, values)),
            ty: ty.clone(),
        },
        PreHirExpr::Call { target, args, ty } => PreHirExpr::Call {
            target: target.clone(),
            args: args
                .iter()
                .map(|arg| resolve_loop_header_expr(arg, values))
                .collect(),
            ty: ty.clone(),
        },
        PreHirExpr::Load { ptr, ty } => PreHirExpr::Load {
            ptr: Box::new(resolve_loop_header_expr(ptr, values)),
            ty: ty.clone(),
        },
        PreHirExpr::PtrOffset { base, offset } => PreHirExpr::PtrOffset {
            base: Box::new(resolve_loop_header_expr(base, values)),
            offset: *offset,
        },
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => PreHirExpr::Index {
            base: Box::new(resolve_loop_header_expr(base, values)),
            index: Box::new(resolve_loop_header_expr(index, values)),
            elem_ty: elem_ty.clone(),
        },
        PreHirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => PreHirExpr::FieldAccess {
            base: Box::new(resolve_loop_header_expr(base, values)),
            field_name: field_name.clone(),
            offset: *offset,
            ty: ty.clone(),
        },
        PreHirExpr::AggregateCopy { src, size } => PreHirExpr::AggregateCopy {
            src: Box::new(resolve_loop_header_expr(src, values)),
            size: *size,
        },
    }
}

fn expr_memory_read_count(expr: &PreHirExpr) -> usize {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_memory_read_count(expr),
        PreHirExpr::Load { ptr, .. } => 1 + expr_memory_read_count(ptr),
        PreHirExpr::FieldAccess { base, .. } => 1 + expr_memory_read_count(base),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_memory_read_count(lhs) + expr_memory_read_count(rhs)
        }
        PreHirExpr::Index { base, index, .. } => {
            1 + expr_memory_read_count(base) + expr_memory_read_count(index)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(expr_memory_read_count).sum(),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_memory_read_count(cond)
                + expr_memory_read_count(then_expr)
                + expr_memory_read_count(else_expr)
        }
    }
}

fn expr_is_loop_header_inline_candidate(expr: &PreHirExpr, depth_budget: usize) -> bool {
    if depth_budget == 0 {
        return false;
    }
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            expr_is_loop_header_inline_candidate(expr, depth_budget - 1)
        }
        PreHirExpr::Binary { lhs, rhs, .. }
        | PreHirExpr::Index {
            base: lhs,
            index: rhs,
            ..
        } => {
            expr_is_loop_header_inline_candidate(lhs, depth_budget - 1)
                && expr_is_loop_header_inline_candidate(rhs, depth_budget - 1)
        }
        PreHirExpr::Call { .. } | PreHirExpr::Select { .. } | PreHirExpr::AggregateCopy { .. } => {
            false
        }
    }
}

/// Repair the `do { ... v = v - c; } while (v != c)` → `do { ... v = v - c; } while (v != 0)`
/// pattern that arises when `canonicalize_condition_expr` folds `(v - c) != 0` → `v != c`
/// before the decrement has been recognized as a loop counter update.
///
/// The invariant: if the do-while body's last statement is `v = v - c` (linear decrement by
/// loop-invariant constant `c > 0`) and the condition is `v != c`, the semantically correct
/// condition is `v != 0` (check the post-decrement value against zero).
pub fn normalize_dowhile_decrement_condition(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::DoWhile { body, cond } => {
                if try_repair_dowhile_dec_cond(body, cond) {
                    changed = true;
                }
                // Recurse into nested statements.
                changed |= normalize_dowhile_decrement_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::While { body, .. }
            | PreHirStmt::For { body, .. }
            | PreHirStmt::Block(body) => {
                changed |= normalize_dowhile_decrement_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= normalize_dowhile_decrement_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= normalize_dowhile_decrement_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= normalize_dowhile_decrement_condition(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    );
                }
                changed |= normalize_dowhile_decrement_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }
    changed
}

/// Check whether `body` ends with `v = v - c` (or `v = v + (-c)`) and `cond` is `v != c`.
/// If so, rewrite cond to `v != 0`.
fn try_repair_dowhile_dec_cond(body: &[PreHirStmt], cond: &mut PreHirExpr) -> bool {
    // Condition must be `v != c` where c is a positive constant.
    let (cond_var, cond_const) = match cond {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (PreHirExpr::Var(name), PreHirExpr::Const(c, _)) if *c > 0 => {
                // Clone to owned so we can mutably borrow `cond` later.
                (name.clone(), *c)
            }
            _ => return false,
        },
        _ => return false,
    };

    // Body must end with `cond_var = cond_var - cond_const`.
    let Some(last) = body.last() else {
        return false;
    };
    let decrement_matches = match last {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == &cond_var => match rhs {
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: inner_lhs,
                rhs: inner_rhs,
                ..
            } => {
                matches!(inner_lhs.as_ref(), PreHirExpr::Var(n) if n == &cond_var)
                    && matches!(inner_rhs.as_ref(), PreHirExpr::Const(c, _) if *c == cond_const)
            }
            _ => false,
        },
        _ => false,
    };

    if !decrement_matches {
        return false;
    }

    // Rewrite: condition `v != c` → `v != 0`
    // Extract the type from the existing rhs (an integer Const), then zero it.
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Ne,
        rhs,
        ..
    } = cond
    {
        let zero_ty = match rhs.as_ref() {
            PreHirExpr::Const(_, ty) => ty.clone(),
            _ => NirType::Unknown,
        };
        *rhs = Box::new(PreHirExpr::Const(0, zero_ty));
        return true;
    }
    false
}

fn inline_loop_condition_trailing_temps_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::DoWhile { body, cond } => {
                changed |= inline_trailing_temps_into_condition(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    cond,
                    read_counts,
                );
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::While { body, .. } | PreHirStmt::Block(body) => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init
                    && let PreHirStmt::Block(body) = init.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                        read_counts,
                    );
                }
                if let Some(update) = update
                    && let PreHirStmt::Block(body) = update.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                        read_counts,
                    );
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    read_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    read_counts,
                );
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    read_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        read_counts,
                    );
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    read_counts,
                );
            }
            _ => {}
        }
    }
    changed
}

fn inline_trailing_temps_into_condition(
    body: &mut Vec<PreHirStmt>,
    cond: &mut PreHirExpr,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    loop {
        let Some(PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
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

fn expr_is_low_cost_inline_candidate(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_is_low_cost_inline_candidate(expr),
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::And
                    | PreHirBinaryOp::Or
                    | PreHirBinaryOp::Xor
                    | PreHirBinaryOp::Add
                    | PreHirBinaryOp::Sub
                    | PreHirBinaryOp::Shl
                    | PreHirBinaryOp::Shr
                    | PreHirBinaryOp::Sar
                    | PreHirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

pub fn collapse_redundant_conditional_returns(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some(PreHirStmt::If {
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
                rewritten.push(PreHirStmt::Expr(cond.clone()));
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
                    rewritten.push(PreHirStmt::Expr(cond.clone()));
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

fn as_return_stmt(stmt: &PreHirStmt) -> Option<&PreHirStmt> {
    matches!(stmt, PreHirStmt::Return(_)).then_some(stmt)
}

fn return_expr(stmt: &PreHirStmt) -> Option<&PreHirExpr> {
    match stmt {
        PreHirStmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

fn single_return_stmt(body: &[PreHirStmt]) -> Option<PreHirStmt> {
    match body {
        [PreHirStmt::Return(expr)] => Some(PreHirStmt::Return(expr.clone())),
        _ => None,
    }
}

fn single_return_expr(body: &[PreHirStmt]) -> Option<&PreHirExpr> {
    match body {
        [PreHirStmt::Return(Some(expr))] => Some(expr),
        _ => None,
    }
}

fn if_parts(stmt: &PreHirStmt) -> Option<(&PreHirExpr, &[PreHirStmt], &[PreHirStmt])> {
    match stmt {
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => Some((cond, then_body, else_body)),
        _ => None,
    }
}

fn binary_comparison_parts(
    expr: &PreHirExpr,
) -> Option<(PreHirBinaryOp, &Box<PreHirExpr>, &Box<PreHirExpr>, &NirType)> {
    match expr {
        PreHirExpr::Binary {
            op:
                op @ (PreHirBinaryOp::Lt
                | PreHirBinaryOp::Le
                | PreHirBinaryOp::Gt
                | PreHirBinaryOp::Ge
                | PreHirBinaryOp::SLt
                | PreHirBinaryOp::SLe
                | PreHirBinaryOp::SGt
                | PreHirBinaryOp::SGe),
            lhs,
            rhs,
            ty,
        } => Some((*op, lhs, rhs, ty)),
        _ => None,
    }
}

fn minmax_branch_swap_op(op: PreHirBinaryOp) -> Option<PreHirBinaryOp> {
    match op {
        PreHirBinaryOp::Lt | PreHirBinaryOp::Le => Some(PreHirBinaryOp::Gt),
        PreHirBinaryOp::Gt | PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Lt),
        PreHirBinaryOp::SLt | PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGt),
        PreHirBinaryOp::SGt | PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLt),
        _ => None,
    }
}

pub fn canonicalize_minmax_conditional_returns(stmts: &mut Vec<PreHirStmt>) -> bool {
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

        stmts[idx] = PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: new_op,
                lhs: Box::new(lhs_expr.clone()),
                rhs: Box::new(rhs_expr.clone()),
                ty: cond_ty,
            },
            then_body: vec![PreHirStmt::Return(Some(lhs_expr))].into(),
            else_body: Vec::new().into(),
        };
        stmts[idx + 1] = PreHirStmt::Return(Some(rhs_expr));
        changed = true;
        idx += 2;
    }

    changed
}

/// Ghidra `RuleConditionalMove` analog: convert
/// `if (cond) { x = A; } else { x = B; }` → `x = cond ? A : B`
/// when both branches are single, side-effect-free assignments to the
/// same variable.
///
/// This improves readability for x86/x86-64 CMOVcc patterns.
/// Returns true if any change was made.
pub fn conditional_select_pass(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0;
    while idx < stmts.len() {
        // Check for a convertible If, then decide.
        let is_convertible = matches!(&stmts[idx], PreHirStmt::If { .. });
        if is_convertible {
            // Temporarily take ownership to avoid borrow conflicts.
            let stmt = std::mem::replace(&mut stmts[idx], PreHirStmt::Break);
            if let PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } = &stmt
            {
                if let Some(replacement) = try_cmov_to_select(cond, then_body, else_body) {
                    stmts[idx] = replacement;
                    changed = true;
                } else {
                    // Restore and recurse.
                    stmts[idx] = stmt;
                    if let PreHirStmt::If {
                        then_body,
                        else_body,
                        ..
                    } = &mut stmts[idx]
                    {
                        changed |= conditional_select_pass(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                        );
                        changed |= conditional_select_pass(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                        );
                    }
                }
            } else {
                stmts[idx] = stmt;
            }
        } else {
            match &mut stmts[idx] {
                PreHirStmt::While { body, .. }
                | PreHirStmt::DoWhile { body, .. }
                | PreHirStmt::Block(body) => {
                    changed |=
                        conditional_select_pass(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
                PreHirStmt::For {
                    init, update, body, ..
                } => {
                    if let Some(init) = init {
                        if let PreHirStmt::Block(b) = init.as_mut() {
                            changed |= conditional_select_pass(
                                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            );
                        }
                    }
                    if let Some(update) = update {
                        if let PreHirStmt::Block(b) = update.as_mut() {
                            changed |= conditional_select_pass(
                                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            );
                        }
                    }
                    changed |=
                        conditional_select_pass(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        changed |= conditional_select_pass(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        );
                    }
                    changed |=
                        conditional_select_pass(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
                }
                _ => {}
            }
        }
        idx += 1;
    }
    changed
}

/// Try to recognize:
///   `if (cond) { lhs = then_rhs; } else { lhs = else_rhs; }`
/// where:
///   - `then_body` and `else_body` are each a single assignment to the same variable
///   - neither `then_rhs` nor `else_rhs` has side effects
///   - `then_rhs` and `else_rhs` are cheap to inline (no calls)
///
/// Returns `PreHirStmt::Assign { lhs, rhs: Select { cond, then_expr, else_expr } }` if matched.
fn try_cmov_to_select(
    cond: &PreHirExpr,
    then_body: &[PreHirStmt],
    else_body: &[PreHirStmt],
) -> Option<PreHirStmt> {
    // Both branches must be a single assignment.
    let (then_lhs, then_rhs) = single_assign(then_body)?;
    let (else_lhs, else_rhs) = single_assign(else_body)?;

    // Must assign to the same variable.
    if then_lhs != else_lhs {
        return None;
    }

    // Neither RHS may have side effects.
    if expr_has_side_effects(then_rhs) || expr_has_side_effects(else_rhs) {
        return None;
    }

    // The condition itself must not have side effects.
    if expr_has_side_effects(cond) {
        return None;
    }

    // Cheap inline candidates only (avoid large expressions in ternaries).
    if !expr_is_low_cost_inline_candidate(then_rhs) || !expr_is_low_cost_inline_candidate(else_rhs)
    {
        return None;
    }

    let result_ty = expr_nir_type(then_rhs).or_else(|| expr_nir_type(else_rhs))?;

    Some(PreHirStmt::Assign {
        lhs: PreHirLValue::Var(then_lhs.to_string()),
        rhs: PreHirExpr::Select {
            cond: Box::new(cond.clone()),
            then_expr: Box::new(then_rhs.clone()),
            else_expr: Box::new(else_rhs.clone()),
            ty: result_ty,
        },
    })
}

fn single_assign(body: &[PreHirStmt]) -> Option<(&str, &PreHirExpr)> {
    match body {
        [
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            },
        ] => Some((name.as_str(), rhs)),
        _ => None,
    }
}

fn expr_nir_type(expr: &PreHirExpr) -> Option<NirType> {
    match expr {
        PreHirExpr::Const(_, ty) => Some(ty.clone()),
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) => None,
        PreHirExpr::Cast { ty, .. } => Some(ty.clone()),
        PreHirExpr::Binary { ty, .. } => Some(ty.clone()),
        PreHirExpr::Unary { ty, .. } => Some(ty.clone()),
        PreHirExpr::Load { ty, .. } => Some(ty.clone()),
        PreHirExpr::Select { ty, .. } => Some(ty.clone()),
        PreHirExpr::Index { elem_ty, .. } => Some(elem_ty.clone()),
        PreHirExpr::PtrOffset { .. } | PreHirExpr::FieldAccess { .. } => None,
        PreHirExpr::Call { ty, .. } => Some(ty.clone()),
        PreHirExpr::AggregateCopy { .. } => None,
    }
}

/// Match-path recovery: when a loop is followed by `return SENTINEL` (const)
/// and the body has `result = value; break;`, rewrite the found-path break to
/// `return result` so search/lookup helpers don't fall through to the not-found
/// sentinel (`find_pair_value` / `kv_lookup`-class).
///
/// Only rewrites a break that is immediately preceded (same statement list)
/// by a non-induction assignment to a local. Exit breaks like
/// `if (i >= n) break;` are left alone.
pub fn rewrite_found_path_break_to_return(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        let has_sentinel = matches!(
            stmts.get(idx + 1),
            Some(PreHirStmt::Return(Some(PreHirExpr::Const(_, _))))
        );
        let is_loop = matches!(
            stmts.get(idx),
            Some(PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. })
        );
        if is_loop && has_sentinel {
            if rewrite_found_breaks_in_stmt(&mut stmts[idx]) {
                changed = true;
            }
        } else {
            changed |= rewrite_found_path_break_to_return_in_stmt(&mut stmts[idx]);
        }
        idx += 1;
    }
    changed
}

fn rewrite_found_path_break_to_return_in_stmt(stmt: &mut PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            rewrite_found_path_break_to_return(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body))
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            let mut changed = false;
            if let Some(init) = init {
                changed |= rewrite_found_path_break_to_return_in_stmt(init);
            }
            if let Some(update) = update {
                changed |= rewrite_found_path_break_to_return_in_stmt(update);
            }
            changed
                | rewrite_found_path_break_to_return(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body))
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            rewrite_found_path_break_to_return(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body))
                | rewrite_found_path_break_to_return(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                    else_body,
                ))
        }
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= rewrite_found_path_break_to_return(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                );
            }
            changed
                | rewrite_found_path_break_to_return(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                    default,
                ))
        }
        _ => false,
    }
}

fn rewrite_found_breaks_in_stmt(stmt: &mut PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            rewrite_found_breaks_in_body(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body))
        }
        _ => false,
    }
}

fn rewrite_found_breaks_in_body(body: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    // Nested regions first.
    for stmt in body.iter_mut() {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_found_breaks_in_body(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                    then_body,
                ));
                changed |= rewrite_found_breaks_in_body(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                    else_body,
                ));
            }
            PreHirStmt::Block(inner)
            | PreHirStmt::While { body: inner, .. }
            | PreHirStmt::DoWhile { body: inner, .. }
            | PreHirStmt::For { body: inner, .. } => {
                changed |=
                    rewrite_found_breaks_in_body(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(inner));
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= rewrite_found_breaks_in_body(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    );
                }
                changed |=
                    rewrite_found_breaks_in_body(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            }
            _ => {}
        }
    }

    let mut i = 0usize;
    while i + 1 < body.len() {
        if matches!(&body[i + 1], PreHirStmt::Break)
            && let Some(ret_name) = found_result_assign_name(&body[i])
        {
            body[i + 1] = PreHirStmt::Return(Some(PreHirExpr::Var(ret_name)));
            changed = true;
            i += 2;
            continue;
        }
        i += 1;
    }
    changed
}

/// `result = <expr>` that is not a loop-induction self-update (`i = i + 1`).
fn found_result_assign_name(stmt: &PreHirStmt) -> Option<String> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    if is_self_increment_assign(name, rhs) {
        return None;
    }
    // Avoid promoting pure param copies as "found" results.
    if matches!(rhs, PreHirExpr::Var(src) if src.starts_with("param_")) {
        return None;
    }
    Some(name.clone())
}

fn is_self_increment_assign(name: &str, rhs: &PreHirExpr) -> bool {
    match rhs {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add | PreHirBinaryOp::Sub,
            lhs,
            rhs: step,
            ..
        } => {
            let lhs_is_self = matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == name);
            let rhs_is_self = matches!(step.as_ref(), PreHirExpr::Var(n) if n == name);
            let lhs_is_const = matches!(lhs.as_ref(), PreHirExpr::Const(_, _));
            let rhs_is_const = matches!(step.as_ref(), PreHirExpr::Const(_, _));
            (lhs_is_self && rhs_is_const) || (rhs_is_self && lhs_is_const)
        }
        _ => false,
    }
}

/// `goto L` whose label was dropped (common after match-path return rewrites
/// leave a continue-edge label unreferenced) → loop-continue when nested in a
/// loop. If the loop bound-check form `if (bound <= i) break;` is present,
/// inject `i++` before `continue` so the orphan edge matches the usual
/// search-loop continue (skip-to-increment) rather than spinning forever.
///
/// `defined` must be the set of labels defined anywhere in the *whole
/// function*, not just `stmts` -- this is called once per nesting level
/// during `cleanup_stmt_list_with_options_and_preserved`'s per-branch
/// recursion, so `stmts` is frequently just one arm of an `If` (e.g. a
/// `while` loop's `then_body`, with the label its one exit `goto` targets
/// sitting in the sibling `else_body`). Computing `defined` from `stmts`
/// alone would see that label as nonexistent ("orphaned") purely because
/// it's a sibling's label rather than genuinely unreferenced anywhere, and
/// blindly rewrite a real, meaningful jump into `continue` -- silently
/// dropping whatever code lived at that label (e.g. a loop's increment
/// step) and turning a finite loop into an infinite one at runtime.
pub fn rewrite_orphan_loop_gotos_to_continue(
    stmts: &mut Vec<PreHirStmt>,
    defined: Option<&HashSet<String>>,
) -> bool {
    // Without whole-function label visibility we cannot tell a genuinely
    // orphaned goto from one whose label just lives in a sibling scope --
    // skip rather than guess (see the doc comment above).
    let Some(defined) = defined else {
        return false;
    };
    rewrite_orphan_loop_gotos_to_continue_rec(stmts, defined, false, None)
}

fn rewrite_orphan_loop_gotos_to_continue_rec(
    stmts: &mut Vec<PreHirStmt>,
    defined: &HashSet<String>,
    in_loop: bool,
    loop_index: Option<&str>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Goto(label) if in_loop && !defined.contains(label) => {
                *stmt = orphan_loop_continue_stmt(loop_index);
                changed = true;
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                let idx = detect_bound_break_loop_index(body);
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    defined,
                    true,
                    idx.as_deref().or(loop_index),
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                let idx = detect_bound_break_loop_index(body);
                let idx_ref = idx.as_deref().or(loop_index);
                if let Some(init) = init {
                    changed |= rewrite_orphan_loop_gotos_to_continue_in_stmt(
                        init, defined, in_loop, loop_index,
                    );
                }
                if let Some(update) = update {
                    changed |= rewrite_orphan_loop_gotos_to_continue_in_stmt(
                        update, defined, true, idx_ref,
                    );
                }
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    defined,
                    true,
                    idx_ref,
                );
            }
            PreHirStmt::Block(body) => {
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    defined,
                    in_loop,
                    loop_index,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    defined,
                    in_loop,
                    loop_index,
                );
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    defined,
                    in_loop,
                    loop_index,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        defined,
                        in_loop,
                        loop_index,
                    );
                }
                changed |= rewrite_orphan_loop_gotos_to_continue_rec(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    defined,
                    in_loop,
                    loop_index,
                );
            }
            _ => {}
        }
    }
    changed
}

fn rewrite_orphan_loop_gotos_to_continue_in_stmt(
    stmt: &mut PreHirStmt,
    defined: &HashSet<String>,
    in_loop: bool,
    loop_index: Option<&str>,
) -> bool {
    match stmt {
        PreHirStmt::Block(body) => rewrite_orphan_loop_gotos_to_continue_rec(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            defined,
            in_loop,
            loop_index,
        ),
        PreHirStmt::Goto(label) if in_loop && !defined.contains(label) => {
            *stmt = orphan_loop_continue_stmt(loop_index);
            true
        }
        _ => false,
    }
}

fn orphan_loop_continue_stmt(loop_index: Option<&str>) -> PreHirStmt {
    if let Some(idx) = loop_index {
        PreHirStmt::Block(
            vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(idx.to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var(idx.to_string())),
                        rhs: Box::new(PreHirExpr::Const(
                            1,
                            NirType::Int {
                                bits: 64,
                                signed: true,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                    },
                },
                PreHirStmt::Continue,
            ]
            .into(),
        )
    } else {
        PreHirStmt::Continue
    }
}

/// Recognize `if (bound <= i) break;` / `if (i >= bound) break;` at the head
/// of a search-loop body and return the induction variable name.
fn detect_bound_break_loop_index(body: &[PreHirStmt]) -> Option<String> {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = body.first()?
    else {
        return None;
    };
    if !else_body.is_empty() || !matches!(then_body.as_slice(), [PreHirStmt::Break]) {
        return None;
    }
    let PreHirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    match op {
        PreHirBinaryOp::Le | PreHirBinaryOp::Lt => {
            // bound <= i  or  bound < i
            match (lhs.as_ref(), rhs.as_ref()) {
                (_, PreHirExpr::Var(idx)) => Some(idx.clone()),
                (PreHirExpr::Var(idx), _) => Some(idx.clone()),
                _ => None,
            }
        }
        PreHirBinaryOp::Ge | PreHirBinaryOp::Gt => {
            // i >= bound  or  i > bound
            match (lhs.as_ref(), rhs.as_ref()) {
                (PreHirExpr::Var(idx), _) => Some(idx.clone()),
                (_, PreHirExpr::Var(idx)) => Some(idx.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}
