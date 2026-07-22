use super::super::analysis::defuse::DefUseMap;
use super::super::analysis::preservation::{
    should_block_trivial_return_collapse, should_keep_unused_temp_binding,
    should_skip_inline_for_preserved_temp,
};
use super::utils::*;
use crate::prelude::*;
use fission_midend_core::wave_stats;
use crate::{HashMap, HashSet};

pub fn collapse_trivial_assign_returns(
    stmts: &mut Vec<DirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    let mut changed = false;
    let mut blocked = 0usize;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let replacement = match (&stmts[idx], &stmts[idx + 1]) {
            (
                DirStmt::Assign {
                    lhs: DirLValue::Var(name),
                    rhs,
                },
                DirStmt::Return(Some(DirExpr::Var(ret_name))),
            ) if name == ret_name => {
                // Collapse candidates:
                // - ABI return regs with pure RHS (`rax = param+5; return rax`)
                // - trivial temps (subject to preservation)
                // - any local with pure RHS, especially const (`w8 = 3; return w8`)
                let pure_rhs = is_pure_return_collapse_rhs(rhs);
                let is_temp = is_trivial_temp_name(name);
                let is_abi = is_abi_return_register_name(name);
                if is_abi {
                    if pure_rhs { Some(rhs.clone()) } else { None }
                } else if is_temp || pure_rhs {
                    if should_block_trivial_return_collapse(name, preserved_temps)
                        && !matches!(rhs, DirExpr::Const(_, _))
                    {
                        blocked += 1;
                        None
                    } else {
                        Some(rhs.clone())
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[idx + 1] = DirStmt::Return(Some(expr));
            to_remove[idx] = true;
            changed = true;
        }
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    // Recurse into nested structured regions so Block/if arms also fold.
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                if collapse_trivial_assign_returns(body, preserved_temps) {
                    changed = true;
                }
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if collapse_trivial_assign_returns(then_body, preserved_temps) {
                    changed = true;
                }
                if collapse_trivial_assign_returns(else_body, preserved_temps) {
                    changed = true;
                }
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    if collapse_trivial_assign_returns(&mut case.body, preserved_temps) {
                        changed = true;
                    }
                }
                if collapse_trivial_assign_returns(default, preserved_temps) {
                    changed = true;
                }
            }
            _ => {}
        }
    }

    wave_stats::add_preserved_temp_prune_blocked(blocked);
    changed
}

pub fn inline_single_use_temps(stmts: &mut Vec<DirStmt>, preserved_temps: &HashSet<&str>) -> bool {
    // Whole-function use counts: nested scopes must see post-loop uses so
    // loop-carried temps (def in loop, use after) are not falsely single-use.
    let use_counts = DefUseMap::build(stmts).use_count;
    inline_single_use_temps_recursive(stmts, preserved_temps, &use_counts)
}

/// Adjacent pure copy into the next if: `t = a; if (… t …)` → `if (… a …)`
/// when `t` is a trivial temp, `a` is pure, and every use of `t` is on that if
/// (whole-function and sequential budgets match).
///
/// Measured on power exit tests after folding `t == 0 || t < 0` → `t <= 0`.
pub fn collapse_adjacent_pure_copy_into_if(stmts: &mut Vec<DirStmt>) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    collapse_adjacent_pure_copy_into_if_with_counts(stmts, &use_counts)
}

fn collapse_adjacent_pure_copy_into_if_with_counts(
    stmts: &mut Vec<DirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = collapse_adjacent_pure_copy_into_if_linear(stmts, use_counts);
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(body, use_counts);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let DirStmt::Block(b) = &mut **i {
                        changed |= collapse_adjacent_pure_copy_into_if_with_counts(b, use_counts);
                    }
                }
                if let Some(u) = update {
                    if let DirStmt::Block(b) = &mut **u {
                        changed |= collapse_adjacent_pure_copy_into_if_with_counts(b, use_counts);
                    }
                }
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(body, use_counts);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(then_body, use_counts);
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(else_body, use_counts);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        collapse_adjacent_pure_copy_into_if_with_counts(&mut case.body, use_counts);
                }
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(default, use_counts);
            }
            _ => {}
        }
    }
    changed
}

fn collapse_adjacent_pure_copy_into_if_linear(
    stmts: &mut Vec<DirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    if stmts.len() < 2 {
        return false;
    }
    let mut changed = false;
    let mut i = 0usize;
    while i + 1 < stmts.len() {
        let (t_name, rhs) = match &stmts[i] {
            DirStmt::Assign {
                lhs: DirLValue::Var(t),
                rhs,
            } if is_trivial_temp_name(t) && expr_is_pure_copy_rhs(rhs) => (t.clone(), rhs.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        let DirStmt::If { .. } = &stmts[i + 1] else {
            i += 1;
            continue;
        };
        let if_uses = count_var_uses_in_stmt(&stmts[i + 1], &t_name);
        if if_uses == 0 {
            i += 1;
            continue;
        }
        let sequential = count_uses_until_redef(stmts, i, &t_name);
        let total = use_counts.get(t_name.as_str()).copied().unwrap_or(0);
        // All uses of t are on this if (no post-loop / multi-def residual uses).
        if sequential != if_uses || total != sequential {
            i += 1;
            continue;
        }
        // Free vars of rhs must not be redefined between def and if (adjacent: none).
        replace_var_in_stmt(&mut stmts[i + 1], &t_name, &rhs);
        stmts.remove(i);
        changed = true;
        // Do not advance i — new stmt at i may also match.
    }
    changed
}

/// Collapse pure temp self-square chains:
/// `t = a; t = t * t; a = t`  →  `a = a * a`
/// when `t` is a trivial temp and has no other live uses in between.
///
/// Measured on x86 `imul` squaring for power-class loops (`base *= base`).
pub fn collapse_temp_self_square_assigns(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    changed |= collapse_temp_self_square_linear(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= collapse_temp_self_square_assigns(body);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let DirStmt::Block(b) = &mut **i {
                        changed |= collapse_temp_self_square_assigns(b);
                    }
                }
                if let Some(u) = update {
                    if let DirStmt::Block(b) = &mut **u {
                        changed |= collapse_temp_self_square_assigns(b);
                    }
                }
                changed |= collapse_temp_self_square_assigns(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_temp_self_square_assigns(then_body);
                changed |= collapse_temp_self_square_assigns(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_temp_self_square_assigns(&mut case.body);
                }
                changed |= collapse_temp_self_square_assigns(default);
            }
            _ => {}
        }
    }
    changed
}

fn collapse_temp_self_square_linear(stmts: &mut Vec<DirStmt>) -> bool {
    if stmts.len() < 3 {
        return false;
    }
    let mut changed = false;
    let mut i = 0usize;
    while i + 2 < stmts.len() {
        let Some((t_name, a_name, ty)) = match_temp_self_square_window(stmts, i) else {
            i += 1;
            continue;
        };
        // Refuse if `t` is used outside this three-stmt window in this linear list.
        let other_t_uses: usize = stmts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx < i || *idx > i + 2)
            .map(|(_, s)| count_var_uses_in_stmt(s, &t_name))
            .sum();
        if other_t_uses > 0 {
            i += 1;
            continue;
        }
        stmts[i] = DirStmt::Assign {
            lhs: DirLValue::Var(a_name.clone()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Mul,
                lhs: Box::new(DirExpr::Var(a_name.clone())),
                rhs: Box::new(DirExpr::Var(a_name)),
                ty,
            },
        };
        stmts.remove(i + 2);
        stmts.remove(i + 1);
        changed = true;
    }
    changed
}

fn match_temp_self_square_window(stmts: &[DirStmt], i: usize) -> Option<(String, String, NirType)> {
    let DirStmt::Assign {
        lhs: DirLValue::Var(t1),
        rhs: DirExpr::Var(a1),
    } = &stmts[i]
    else {
        return None;
    };
    let DirStmt::Assign {
        lhs: DirLValue::Var(t2),
        rhs:
            DirExpr::Binary {
                op: DirBinaryOp::Mul,
                lhs,
                rhs,
                ty,
            },
    } = &stmts[i + 1]
    else {
        return None;
    };
    let DirStmt::Assign {
        lhs: DirLValue::Var(a2),
        rhs: DirExpr::Var(t3),
    } = &stmts[i + 2]
    else {
        return None;
    };
    let (DirExpr::Var(tl), DirExpr::Var(tr)) = (lhs.as_ref(), rhs.as_ref()) else {
        return None;
    };
    if t1 != t2 || t1 != t3 || t1 != tl || t1 != tr || a1 != a2 {
        return None;
    }
    if !is_trivial_temp_name(t1) || t1 == a1 {
        return None;
    }
    Some((t1.clone(), a1.clone(), ty.clone()))
}

fn inline_single_use_temps_recursive(
    stmts: &mut Vec<DirStmt>,
    preserved_temps: &HashSet<&str>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let (name, rhs) = match &stmts[idx] {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if is_trivial_temp_name(name) => (name.clone(), rhs.clone()),
            _ => {
                idx += 1;
                continue;
            }
        };
        if should_skip_inline_for_preserved_temp(&name, preserved_temps) {
            idx += 1;
            continue;
        }

        let prefers_stable_materialization = expr_prefers_stable_materialization(&rhs);
        let Some(target_idx) =
            find_inline_forward_target(stmts, idx, &name, prefers_stable_materialization)
        else {
            idx += 1;
            continue;
        };
        let target_uses = count_var_uses_in_stmt(&stmts[target_idx], &name);
        let total_uses = use_counts.get(name.as_str()).copied().unwrap_or(0);
        // Pure Var/Const/Cast-of-Var copies: allow when all whole-function uses
        // are covered by the sequential live range up to the forward target.
        // This inlines `t = a; if (t <= 0)` inside loops without treating
        // multi-def or post-loop uses as single-use (loop-carried / CDQ-safe).
        let pure_copy = expr_is_pure_copy_rhs(&rhs);
        let sequential_uses = if pure_copy {
            count_uses_until_redef(stmts, idx, &name)
        } else {
            0
        };
        let use_budget_ok = if pure_copy && sequential_uses > 0 {
            sequential_uses == target_uses && total_uses == sequential_uses
        } else {
            total_uses == target_uses
        };
        if !use_budget_ok {
            idx += 1;
            continue;
        }
        let predicate_sensitive = stmt_uses_var_in_predicate_position(&stmts[target_idx], &name);
        let low_cost_inline = expr_is_low_cost_inline_candidate(&rhs);
        if target_uses > 1 && prefers_stable_materialization {
            idx += 1;
            continue;
        }
        if predicate_sensitive && !low_cost_inline {
            idx += 1;
            continue;
        }
        if target_uses > 1 && !low_cost_inline {
            idx += 1;
            continue;
        }
        replace_var_in_stmt(&mut stmts[target_idx], &name, &rhs);
        to_remove[idx] = true;
        changed = true;
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= inline_single_use_temps_recursive(body, preserved_temps, use_counts);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let DirStmt::Block(b) = &mut **i {
                        changed |=
                            inline_single_use_temps_recursive(b, preserved_temps, use_counts);
                    }
                }
                if let Some(u) = update {
                    if let DirStmt::Block(b) = &mut **u {
                        changed |=
                            inline_single_use_temps_recursive(b, preserved_temps, use_counts);
                    }
                }
                changed |= inline_single_use_temps_recursive(body, preserved_temps, use_counts);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    inline_single_use_temps_recursive(then_body, preserved_temps, use_counts);
                changed |=
                    inline_single_use_temps_recursive(else_body, preserved_temps, use_counts);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_single_use_temps_recursive(
                        &mut case.body,
                        preserved_temps,
                        use_counts,
                    );
                }
                changed |= inline_single_use_temps_recursive(default, preserved_temps, use_counts);
            }
            _ => {}
        }
    }

    changed
}

fn count_uses_until_redef(stmts: &[DirStmt], def_idx: usize, name: &str) -> usize {
    let mut total = 0usize;
    for stmt in stmts.iter().skip(def_idx + 1) {
        if stmt_redefines_temp(stmt, name) {
            break;
        }
        total = total.saturating_add(count_var_uses_in_stmt(stmt, name));
    }
    total
}

fn expr_is_pure_copy_rhs(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(_) | DirExpr::Const(_, _) => true,
        DirExpr::Cast { expr, .. } => {
            matches!(expr.as_ref(), DirExpr::Var(_) | DirExpr::Const(_, _))
        }
        _ => false,
    }
}

fn find_inline_forward_target(
    stmts: &[DirStmt],
    def_idx: usize,
    name: &str,
    stable_materialization: bool,
) -> Option<usize> {
    let mut scan_idx = def_idx + 1;
    while scan_idx < stmts.len() {
        let stmt = &stmts[scan_idx];
        let uses = count_var_uses_in_stmt(stmt, name);
        let redefines = stmt_redefines_temp(stmt, name);
        if redefines {
            return None;
        }
        if uses > 0 && stmt_allows_inline_target(stmt) {
            return Some(scan_idx);
        }
        if uses == 0 {
            if stmt_redefines_expr_dependency(stmt, &stmts[def_idx]) {
                return None;
            }
            if stmt_blocks_linear_inline_scan(stmt) {
                return None;
            }
            if stable_materialization && stmt_blocks_stable_inline_scan(stmt) {
                return None;
            }
            scan_idx += 1;
            continue;
        }
        if !stmt_allows_forward_scan(stmt) {
            return None;
        }
        return None;
    }
    None
}

fn stmt_redefines_expr_dependency(stmt: &DirStmt, defining_stmt: &DirStmt) -> bool {
    let DirStmt::Assign {
        lhs: DirLValue::Var(defined_name),
        ..
    } = stmt
    else {
        return false;
    };
    let DirStmt::Assign { rhs, .. } = defining_stmt else {
        return false;
    };
    expr_mentions_var(rhs, defined_name)
}

fn stmt_blocks_linear_inline_scan(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, DirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        DirStmt::Expr(expr) => expr_has_side_effects(expr),
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(_)
        | DirStmt::VaStart { .. }
        | DirStmt::Block(_)
        | DirStmt::Switch { .. }
        | DirStmt::If { .. }
        | DirStmt::While { .. }
        | DirStmt::DoWhile { .. }
        | DirStmt::For { .. }
        | DirStmt::Break
        | DirStmt::Continue => true,
    }
}

fn stmt_blocks_stable_inline_scan(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, DirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        DirStmt::Label(_) => false,
        DirStmt::Return(None)
        | DirStmt::VaStart { .. }
        | DirStmt::Block(_)
        | DirStmt::Switch { .. }
        | DirStmt::If { .. }
        | DirStmt::While { .. }
        | DirStmt::DoWhile { .. }
        | DirStmt::For { .. }
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => true,
    }
}

fn stmt_allows_forward_scan(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(_),
            rhs,
        } => !expr_has_side_effects(rhs),
        DirStmt::Return(Some(expr)) => !expr_has_side_effects(expr),
        DirStmt::If { cond, .. } => !expr_has_side_effects(cond),
        DirStmt::Expr(expr) => !expr_has_side_effects(expr),
        _ => false,
    }
}

fn stmt_allows_inline_target(stmt: &DirStmt) -> bool {
    matches!(
        stmt,
        DirStmt::Assign { .. } | DirStmt::Expr(_) | DirStmt::Return(_) | DirStmt::If { .. }
    )
}

fn stmt_redefines_temp(stmt: &DirStmt, name: &str) -> bool {
    matches!(
        stmt,
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            ..
        } if lhs_name == name
    )
}

fn stmt_uses_var_in_predicate_position(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::If { cond, .. } => expr_contains_var(cond, name),
        DirStmt::While { cond, .. } | DirStmt::DoWhile { cond, .. } => {
            expr_contains_var(cond, name)
        }
        DirStmt::For {
            init, cond, update, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_contains_var(expr, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
        }
        DirStmt::Switch { expr, .. } => expr_contains_var(expr, name),
        DirStmt::Block(stmts) => stmts
            .iter()
            .any(|inner| stmt_uses_var_in_predicate_position(inner, name)),
        _ => false,
    }
}

fn expr_is_low_cost_inline_candidate(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => true,
        DirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => {
            expr_is_low_cost_inline_candidate(expr)
        }
        DirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                DirBinaryOp::Eq
                    | DirBinaryOp::Ne
                    | DirBinaryOp::Lt
                    | DirBinaryOp::Le
                    | DirBinaryOp::SLt
                    | DirBinaryOp::SLe
                    | DirBinaryOp::And
                    | DirBinaryOp::Or
                    | DirBinaryOp::Xor
                    | DirBinaryOp::Add
                    | DirBinaryOp::Sub
                    | DirBinaryOp::Shl
                    | DirBinaryOp::Shr
                    | DirBinaryOp::Sar
                    | DirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

fn expr_prefers_stable_materialization(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => false,
        DirExpr::Cast { expr, .. } => expr_prefers_stable_materialization(expr),
        DirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().any(expr_prefers_stable_materialization)
        }
        DirExpr::Unary { .. }
        | DirExpr::Load { .. }
        | DirExpr::PtrOffset { .. }
        | DirExpr::Index { .. }
        | DirExpr::Select { .. }
        | DirExpr::AggregateCopy { .. }
        | DirExpr::FieldAccess { .. }
        | DirExpr::Call { .. } => true,
        DirExpr::Binary { op, .. } => matches!(
            op,
            DirBinaryOp::Add
                | DirBinaryOp::Sub
                | DirBinaryOp::Mul
                | DirBinaryOp::Div
                | DirBinaryOp::Mod
                | DirBinaryOp::And
                | DirBinaryOp::Or
                | DirBinaryOp::Xor
                | DirBinaryOp::Shl
                | DirBinaryOp::Shr
                | DirBinaryOp::Sar
                | DirBinaryOp::Eq
                | DirBinaryOp::Ne
                | DirBinaryOp::Lt
                | DirBinaryOp::Le
                | DirBinaryOp::SLt
                | DirBinaryOp::SLe
        ),
    }
}

pub fn eliminate_dead_temp_assigns(
    stmts: &mut Vec<DirStmt>,
    _preserved_temps: &HashSet<&str>,
) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    eliminate_dead_temp_assigns_recursive(stmts, &use_counts)
}

fn eliminate_dead_temp_assigns_recursive(
    stmts: &mut Vec<DirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for (idx, stmt) in stmts.iter().enumerate() {
        let (name, rhs) = match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if is_trivial_temp_name(name) => (name, rhs),
            _ => continue,
        };

        let uses = use_counts.get(name.as_str()).copied().unwrap_or(0);
        let side_effects = expr_has_side_effects(rhs);
        if uses == 0 && !side_effects {
            to_remove[idx] = true;
            changed = true;
        }
    }

    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= eliminate_dead_temp_assigns_recursive(body, use_counts);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let DirStmt::Block(b) = &mut **i {
                        changed |= eliminate_dead_temp_assigns_recursive(b, use_counts);
                    }
                }
                if let Some(u) = update {
                    if let DirStmt::Block(b) = &mut **u {
                        changed |= eliminate_dead_temp_assigns_recursive(b, use_counts);
                    }
                }
                changed |= eliminate_dead_temp_assigns_recursive(body, use_counts);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_dead_temp_assigns_recursive(then_body, use_counts);
                changed |= eliminate_dead_temp_assigns_recursive(else_body, use_counts);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_dead_temp_assigns_recursive(&mut case.body, use_counts);
                }
                changed |= eliminate_dead_temp_assigns_recursive(default, use_counts);
            }
            _ => {}
        }
    }

    changed
}

pub fn eliminate_redundant_var_assigns(stmts: &mut Vec<DirStmt>) -> bool {
    eliminate_redundant_var_assigns_recursive(stmts)
}

/// Drop pure `x = x` and adjacent duplicate assigns. Must recurse into nested
/// Block/If/loop bodies — structured O0 functions wrap the real body in a
/// single outer Block, so a top-level-only scan never sees the noise.
fn eliminate_redundant_var_assigns_recursive(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for idx in 0..stmts.len() {
        let DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } = &stmts[idx]
        else {
            continue;
        };

        if matches!(rhs, DirExpr::Var(rhs_name) if rhs_name == name) {
            to_remove[idx] = true;
            changed = true;
            continue;
        }

        if idx == 0
            || to_remove[idx - 1]
            || expr_has_side_effects(rhs)
            || expr_mentions_var(rhs, name)
        {
            continue;
        }

        let DirStmt::Assign {
            lhs: DirLValue::Var(prev_name),
            rhs: prev_rhs,
        } = &stmts[idx - 1]
        else {
            continue;
        };

        if prev_name == name && redundant_assign_rhs_equal(prev_rhs, rhs) {
            to_remove[idx - 1] = true;
            changed = true;
        }
    }

    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= eliminate_redundant_var_assigns_recursive(body);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let DirStmt::Block(b) = &mut **i {
                        changed |= eliminate_redundant_var_assigns_recursive(b);
                    }
                }
                if let Some(u) = update {
                    if let DirStmt::Block(b) = &mut **u {
                        changed |= eliminate_redundant_var_assigns_recursive(b);
                    }
                }
                changed |= eliminate_redundant_var_assigns_recursive(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_redundant_var_assigns_recursive(then_body);
                changed |= eliminate_redundant_var_assigns_recursive(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_redundant_var_assigns_recursive(&mut case.body);
                }
                changed |= eliminate_redundant_var_assigns_recursive(default);
            }
            _ => {}
        }
    }

    changed
}

fn redundant_assign_rhs_equal(lhs: &DirExpr, rhs: &DirExpr) -> bool {
    lhs == rhs
        || matches!(
            (lhs, rhs),
            (DirExpr::Const(lhs_value, _), DirExpr::Const(rhs_value, _)) if lhs_value == rhs_value
        )
}

/// Hoist pure `v = param_N` copies above their first use when a prior pass
/// left a use-before-def (observed after cmov/flag recovery on x86-32 clamp).
///
/// Only applies to top-level straight-line bodies: a single dominating
/// definition of `v` that is a param alias, with an earlier pure use.
pub fn hoist_param_alias_copies_before_first_use(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0usize;
    while i < stmts.len() {
        let (name, param) = match &stmts[i] {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs: DirExpr::Var(param),
            } if param.starts_with("param_") => (name.clone(), param.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        // Only hoist when this is the sole top-level def of `name`.
        let def_count = stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    DirStmt::Assign {
                        lhs: DirLValue::Var(n),
                        ..
                    } if n == &name
                )
            })
            .count();
        if def_count != 1 {
            i += 1;
            continue;
        }
        if let Some(first_use) = first_top_level_use_index_of_var(stmts, &name) {
            if first_use < i {
                let stmt = stmts.remove(i);
                stmts.insert(first_use, stmt);
                changed = true;
                // Restart so chained hoists are ordered correctly.
                i = 0;
                continue;
            }
        }
        let _ = param;
        i += 1;
    }
    // Recurse into structured bodies.
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= hoist_param_alias_copies_before_first_use(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= hoist_param_alias_copies_before_first_use(then_body);
                changed |= hoist_param_alias_copies_before_first_use(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= hoist_param_alias_copies_before_first_use(&mut case.body);
                }
                changed |= hoist_param_alias_copies_before_first_use(default);
            }
            _ => {}
        }
    }
    changed
}

fn first_top_level_use_index_of_var(stmts: &[DirStmt], name: &str) -> Option<usize> {
    for (idx, stmt) in stmts.iter().enumerate() {
        if stmt_uses_var(stmt, name) {
            return Some(idx);
        }
    }
    None
}

fn stmt_uses_var(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => lvalue_uses_var(lhs, name) || expr_mentions_var(rhs, name),
        DirStmt::Expr(expr)
        | DirStmt::Return(Some(expr))
        | DirStmt::VaStart { va_list: expr, .. } => expr_mentions_var(expr, name),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_mentions_var(cond, name)
                || then_body.iter().any(|s| stmt_uses_var(s, name))
                || else_body.iter().any(|s| stmt_uses_var(s, name))
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            body.iter().any(|s| stmt_uses_var(s, name))
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || cond.as_ref().is_some_and(|c| expr_mentions_var(c, name))
                || update.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || body.iter().any(|s| stmt_uses_var(s, name))
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_mentions_var(expr, name)
                || cases
                    .iter()
                    .any(|c| c.body.iter().any(|s| stmt_uses_var(s, name)))
                || default.iter().any(|s| stmt_uses_var(s, name))
        }
        DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

fn lvalue_uses_var(lhs: &DirLValue, name: &str) -> bool {
    match lhs {
        DirLValue::Var(_) => false, // definition, not a use
        DirLValue::Deref { ptr, .. } => expr_mentions_var(ptr, name),
        DirLValue::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
        DirLValue::FieldAccess { base, .. } => expr_mentions_var(base, name),
    }
}

pub fn eliminate_dead_local_clobber_assigns(func: &mut DirFunction) -> bool {
    // Build a whole-function use map so sibling branches / nested blocks are
    // correctly accounted for.  Using a scoped `count_uses_in_stmt_list` on
    // each nested slice risks counting only the local slice and incorrectly
    // classifying a variable as dead when it is live in a sibling scope.
    let use_map = DefUseMap::build(&func.body);
    let local_types: HashMap<&str, &NirType> = func
        .locals
        .iter()
        .map(|b| (b.name.as_str(), &b.ty))
        .collect();
    let param_names: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    // Stack-backed locals (StackOffset / DerivedFromStackOffset origin) must
    // NEVER be silently removed even when their name is never read, because the
    // write itself may be observable through aliased pointers.
    let stack_backed_names: HashSet<&str> = func
        .locals
        .iter()
        .filter(|b| {
            matches!(
                b.origin,
                Some(NirBindingOrigin::StackOffset(_))
                    | Some(NirBindingOrigin::DerivedFromStackOffset(_))
            )
        })
        .map(|b| b.name.as_str())
        .collect();
    eliminate_dead_local_clobber_assigns_in_stmts(
        &mut func.body,
        &param_names,
        &local_types,
        &stack_backed_names,
        &use_map,
    )
}

fn eliminate_dead_local_clobber_assigns_in_stmts(
    stmts: &mut Vec<DirStmt>,
    param_names: &HashSet<&str>,
    local_types: &HashMap<&str, &NirType>,
    stack_backed_names: &HashSet<&str>,
    use_map: &DefUseMap,
) -> bool {
    // Recurse into nested bodies first (the use_map is already whole-function).
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    then_body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
                eliminate_dead_local_clobber_assigns_in_stmts(
                    else_body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    eliminate_dead_local_clobber_assigns_in_stmts(
                        &mut case.body,
                        param_names,
                        local_types,
                        stack_backed_names,
                        use_map,
                    );
                }
                eliminate_dead_local_clobber_assigns_in_stmts(
                    default,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            _ => {}
        }
    }

    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    for (idx, stmt) in stmts.iter().enumerate() {
        let (name, rhs) = match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } => (name.as_str(), rhs),
            _ => continue,
        };
        if !is_dead_local_clobber_name(name)
            || param_names.contains(name)
            || name.starts_with("slot_")
            || expr_has_side_effects(rhs)
        {
            continue;
        }
        // Stack-backed locals (StackOffset / DerivedFromStackOffset) must never
        // be removed even when unused: their writes may be observable through
        // aliased pointers.  This is the authoritative semantic guard that
        // replaces the old hex-offset cut-off.
        if stack_backed_names.contains(name) {
            continue;
        }
        if matches!(
            local_types.get(name).copied(),
            Some(NirType::Aggregate { .. } | NirType::Ptr(_))
        ) {
            continue;
        }
        // Use the whole-function use map — not a local slice — so sibling
        // branches that read this name are correctly counted.
        let uses = use_map.use_count.get(name).copied().unwrap_or(0);
        if uses == 0 {
            to_remove[idx] = true;
            changed = true;
        }
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }
    changed
}

pub fn prune_unused_temp_bindings(func: &mut DirFunction) -> bool {
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = count_uses_in_stmt_list(&func.body, &binding.name) > 0;
        let assigned_side_effect =
            stmt_list_assigns_var_from_side_effecting_expr(&func.body, &binding.name);
        let keep = should_keep_unused_temp_binding(
            is_prunable_unused_temp_binding(binding),
            used || assigned_side_effect,
            binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects),
        );
        changed |= !keep;
        keep
    });
    changed
}

fn is_prunable_unused_temp_binding(binding: &DirBinding) -> bool {
    is_trivial_temp_name(&binding.name) || binding.is_temp_like()
}

fn stmt_list_assigns_var_from_side_effecting_expr(stmts: &[DirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
}

fn stmt_assigns_var_from_side_effecting_expr(stmt: &DirStmt, name: &str) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            rhs,
        } => lhs_name == name && expr_has_side_effects(rhs),
        DirStmt::Block(stmts)
        | DirStmt::While { body: stmts, .. }
        | DirStmt::DoWhile { body: stmts, .. } => {
            stmt_list_assigns_var_from_side_effecting_expr(stmts, name)
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmt_list_assigns_var_from_side_effecting_expr(then_body, name)
                || stmt_list_assigns_var_from_side_effecting_expr(else_body, name)
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || stmt_list_assigns_var_from_side_effecting_expr(body, name)
        }
        DirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmt_list_assigns_var_from_side_effecting_expr(&case.body, name))
                || stmt_list_assigns_var_from_side_effecting_expr(default, name)
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

pub fn prune_unused_dead_local_bindings(func: &mut DirFunction) -> bool {
    let param_names = func
        .params
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<HashSet<_>>();
    let mut changed = false;
    func.locals.retain(|binding| {
        let keep = !is_dead_local_clobber_name(&binding.name)
            || param_names.contains(binding.name.as_str())
            || binding.name.starts_with("slot_")
            || matches!(binding.ty, NirType::Aggregate { .. })
            || count_uses_in_stmt_list(&func.body, &binding.name) > 0
            || binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects);
        changed |= !keep;
        keep
    });
    changed
}

fn is_rescue_candidate_name(name: &str) -> bool {
    if name.starts_with("iVar")
        || name.starts_with("uVar")
        || name.starts_with("bVar")
        || name.starts_with("xVar")
    {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
    } else if name.starts_with("tmp_") {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if let Some(suffix) = name.strip_prefix("local_") {
        // Stack-home surface names (`local_0`, `local_4`, `local_1c`) from
        // materialize. Used but undeclared → compile_error in semantic harness.
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if matches!(name, "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_") {
        // Named EFLAGS bits (SLA 0x200 layout). Prefer dead-flag cleanup; if a
        // live use remains, declare as Bool so the C harness compiles.
        true
    } else if name.starts_with('r') || name.starts_with('e') {
        name != "reg" && name != "rsp" && name != "rbp" && name != "esp" && name != "ebp"
    } else {
        false
    }
}

pub fn rescue_undeclared_bindings(func: &mut DirFunction) -> bool {
    use fission_midend_dir::util::expr_type;

    let mut declared: HashSet<String> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|b| b.name.clone())
        .collect();

    // Collect every variable name that appears anywhere in the body.
    let mut body_names: HashSet<String> = HashSet::default();
    collect_all_body_names_stmts(&func.body, &mut body_names);

    // Find undeclared names and try to infer their type from the first
    // assignment RHS in the body.
    let mut changed = false;
    for name in &body_names {
        if declared.contains(name.as_str()) {
            continue;
        }
        if !is_rescue_candidate_name(name.as_str()) {
            continue;
        }
        let inferred_ty = if matches!(
            name.as_str(),
            "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_"
        ) {
            NirType::Bool
        } else {
            infer_type_from_first_assign(&func.body, name)
        };
        func.locals.push(DirBinding {
            name: name.clone(),
            ty: inferred_ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        declared.insert(name.clone());
        changed = true;
    }
    changed
}

fn collect_all_body_names_expr(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            out.insert(name.clone());
        }
        DirExpr::Const(_, _) | DirExpr::AddressOfGlobal(_) => {}
        DirExpr::Unary { expr, .. } | DirExpr::Cast { expr, .. } => {
            collect_all_body_names_expr(expr, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_all_body_names_expr(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        DirExpr::Call { target, args, .. } => {
            // target is a function name String, not DirExpr.
            for arg in args {
                collect_all_body_names_expr(arg, out);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_expr(then_expr, out);
            collect_all_body_names_expr(else_expr, out);
        }
        DirExpr::Load { ptr, .. } => {
            collect_all_body_names_expr(ptr, out);
        }
        DirExpr::PtrOffset { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        DirExpr::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        DirExpr::AggregateCopy { src, .. } => {
            collect_all_body_names_expr(src, out);
        }
    }
}

fn collect_all_body_names_lvalue(lhs: &DirLValue, out: &mut HashSet<String>) {
    match lhs {
        DirLValue::Var(name) => {
            out.insert(name.clone());
        }
        DirLValue::Deref { ptr, .. } => collect_all_body_names_expr(ptr, out),
        DirLValue::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        DirLValue::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
    }
}

fn collect_all_body_names_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            collect_all_body_names_lvalue(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        DirStmt::VaStart { va_list, .. } | DirStmt::Expr(va_list) => {
            collect_all_body_names_expr(va_list, out);
        }
        DirStmt::Return(Some(expr)) => collect_all_body_names_expr(expr, out),
        DirStmt::Block(body) | DirStmt::While { body, .. } => {
            collect_all_body_names_stmts(body, out);
        }
        DirStmt::DoWhile { body, cond } => {
            collect_all_body_names_stmts(body, out);
            collect_all_body_names_expr(cond, out);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_all_body_names_stmt(init, out);
            }
            if let Some(cond) = cond {
                collect_all_body_names_expr(cond, out);
            }
            if let Some(update) = update {
                collect_all_body_names_stmt(update, out);
            }
            collect_all_body_names_stmts(body, out);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_stmts(then_body, out);
            collect_all_body_names_stmts(else_body, out);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_all_body_names_expr(expr, out);
            for case in cases {
                collect_all_body_names_stmts(&case.body, out);
            }
            collect_all_body_names_stmts(default, out);
        }
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn collect_all_body_names_stmts(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_all_body_names_stmt(stmt, out);
    }
}

/// Try to infer the type of a variable from its first assignment RHS in the body.
fn infer_type_from_first_assign(stmts: &[DirStmt], name: &str) -> NirType {
    use fission_midend_dir::util::expr_type;
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return ty;
        }
    }
    NirType::Unknown
}

fn infer_type_from_stmt(stmt: &DirStmt, name: &str) -> Option<NirType> {
    use fission_midend_dir::util::expr_type;
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == name => {
            let ty = expr_type(rhs);
            Some(if ty == NirType::Unknown {
                NirType::Int {
                    bits: 32,
                    signed: true,
                }
            } else {
                ty
            })
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } => {
            infer_type_from_first_assign_stmts(body, name)
        }
        DirStmt::DoWhile { body, .. } => infer_type_from_first_assign_stmts(body, name),
        DirStmt::For { body, .. } => infer_type_from_first_assign_stmts(body, name),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => infer_type_from_first_assign_stmts(then_body, name)
            .or_else(|| infer_type_from_first_assign_stmts(else_body, name)),
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                if let Some(ty) = infer_type_from_first_assign_stmts(&case.body, name) {
                    return Some(ty);
                }
            }
            infer_type_from_first_assign_stmts(default, name)
        }
        _ => None,
    }
}

fn infer_type_from_first_assign_stmts(stmts: &[DirStmt], name: &str) -> Option<NirType> {
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return Some(ty);
        }
    }
    None
}

pub fn elide_unused_popcount_assigns(func: &mut DirFunction) -> bool {
    if !func.body.iter().any(has_popcount) {
        return false;
    }
    let use_map = DefUseMap::build(&func.body);

    let mut changed = false;
    for _ in 0..8 {
        let round_changed = elide_popcount_round(func, &use_map);
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

fn elide_popcount_round(func: &mut DirFunction, use_map: &DefUseMap) -> bool {
    let mut changed = false;
    elide_popcount_in_stmts(&mut func.body, use_map, &mut changed);
    if changed {
        let remaining_names: HashSet<String> =
            func.body.iter().flat_map(collect_assigned_names).collect();
        func.locals.retain(|b| {
            remaining_names.contains(&b.name)
                || use_map.use_count.get(&b.name).copied().unwrap_or(0) > 0
        });
    }
    changed
}

fn collect_assigned_names(stmt: &DirStmt) -> Vec<String> {
    let mut names = Vec::new();
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } => {
            names.push(name.clone());
        }
        DirStmt::Block(body) => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter().chain(else_body.iter()) {
                names.extend(collect_assigned_names(s));
            }
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        DirStmt::For { body, .. } => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    names.extend(collect_assigned_names(s));
                }
            }
            for s in default {
                names.extend(collect_assigned_names(s));
            }
        }
        _ => {}
    }
    names
}

fn elide_popcount_in_stmts(stmts: &mut Vec<DirStmt>, use_map: &DefUseMap, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) => elide_popcount_in_stmts(body, use_map, changed),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                elide_popcount_in_stmts(then_body, use_map, changed);
                elide_popcount_in_stmts(else_body, use_map, changed);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                elide_popcount_in_stmts(body, use_map, changed);
            }
            DirStmt::For { body, .. } => {
                elide_popcount_in_stmts(body, use_map, changed);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    elide_popcount_in_stmts(&mut case.body, use_map, changed);
                }
                elide_popcount_in_stmts(default, use_map, changed);
            }
            _ => {}
        }
    }
    stmts.retain(|stmt| {
        if let DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } = stmt
        {
            let uses = use_map.use_count.get(name.as_str()).copied().unwrap_or(0);
            if uses == 0 && rhs_contains_popcount(rhs) && !expr_has_side_effects(rhs) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn rhs_contains_popcount(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Call { target, .. } if target == "__popcount" => true,
        DirExpr::Cast { expr: inner, .. } | DirExpr::Unary { expr: inner, .. } => {
            rhs_contains_popcount(inner)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            rhs_contains_popcount(lhs) || rhs_contains_popcount(rhs)
        }
        DirExpr::Call { args, .. } => args.iter().any(rhs_contains_popcount),
        _ => false,
    }
}

fn has_popcount(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign { rhs, .. } => rhs_contains_popcount(rhs),
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => rhs_contains_popcount(expr),
        DirStmt::Block(body) => body.iter().any(has_popcount),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rhs_contains_popcount(cond)
                || then_body.iter().any(has_popcount)
                || else_body.iter().any(has_popcount)
        }
        DirStmt::While { cond, body } | DirStmt::DoWhile { cond, body } => {
            rhs_contains_popcount(cond) || body.iter().any(has_popcount)
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(has_popcount)
                || cond.as_ref().is_some_and(rhs_contains_popcount)
                || update.as_deref().is_some_and(has_popcount)
                || body.iter().any(has_popcount)
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rhs_contains_popcount(expr)
                || cases.iter().any(|c| c.body.iter().any(has_popcount))
                || default.iter().any(has_popcount)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Coerce pointer-typed variables used in integer-only bit operations
// ---------------------------------------------------------------------------

/// Collect variable names that appear as the LHS of an assignment where the RHS
/// is a bitwise-integer-only binary operation (And, Or, Xor, Shl, Shr, Sar).
/// These variables must have an integer (not pointer) type to compile as valid C.
fn collect_bitop_lhs_vars_stmts(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_bitop_lhs_vars_stmt(stmt, out);
    }
}

fn collect_bitop_lhs_vars_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            rhs,
        } => {
            if rhs_is_integer_bitop(rhs) {
                out.insert(name.clone());
            }
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            collect_bitop_lhs_vars_stmts(body, out);
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_bitop_lhs_vars_stmts(then_body, out);
            collect_bitop_lhs_vars_stmts(else_body, out);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_bitop_lhs_vars_stmts(&case.body, out);
            }
            collect_bitop_lhs_vars_stmts(default, out);
        }
        _ => {}
    }
}

fn rhs_is_integer_bitop(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Binary { op, .. } => matches!(
            op,
            DirBinaryOp::And
                | DirBinaryOp::Or
                | DirBinaryOp::Xor
                | DirBinaryOp::Shl
                | DirBinaryOp::Shr
                | DirBinaryOp::Sar
        ),
        DirExpr::Cast { expr: inner, .. } => rhs_is_integer_bitop(inner),
        _ => false,
    }
}

/// Safety-net pass: if a local binding has `NirType::Ptr(_)` but is used as the
/// destination of a bitwise-integer-only operation, coerce its type to `ulonglong`
/// so that the generated C compiles cleanly.
///
/// This handles x86-64 idioms where a pointer difference is computed, stored in
/// a pointer-typed slot, and then bit-masked (e.g. `ptr_diff &= 4`).
pub fn coerce_ptr_typed_bitop_vars(func: &mut DirFunction) -> bool {
    // Collect all LHS names that receive a bitwise-integer RHS.
    let mut bitop_lhs: HashSet<String> = HashSet::default();
    collect_bitop_lhs_vars_stmts(&func.body, &mut bitop_lhs);
    if bitop_lhs.is_empty() {
        return false;
    }

    let int64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };

    let mut changed = false;
    for binding in &mut func.locals {
        if bitop_lhs.contains(&binding.name) && matches!(binding.ty, NirType::Ptr(_)) {
            binding.ty = int64_ty.clone();
            // Drop any pointer initializer so it doesn't conflict with the new integer type.
            if binding.initializer.is_some() {
                binding.initializer = None;
            }
            changed = true;
        }
    }
    changed
}
