/// HIR-level copy propagation and join-variable coalescing.
///
/// These passes improve the HIR after structuring by eliminating unnecessary
/// temporary variables and unifying variables that represent the same value
/// at control-flow join points.
///
/// ## Copy Propagation (`copy_propagation_pass`)
///
/// For every assignment `x = y` where `y` is a named variable and `x` is a
/// pure temporary with a single definition, substitutes `y` for every use of
/// `x` throughout the function and removes the assignment.
///
/// This is safe when:
/// - `x` has exactly one definition in the entire function body
/// - `y` is never re-assigned between the definition of `x` and any use of `x`
///   (conservatively approximated by requiring `y` to have no assignment at all
///   in the subtree between the definition and the last use — for the linear
///   case we simply require that `y` is not a pure temp that gets redefined)
///
/// ## Join Variable Coalescing (`join_coalescing_pass`)
///
/// Detects if-else structures where both branches end by assigning to the
/// *same* set of variables and renames join-point uses to the shared variable.
/// This models the classical SSA out-of-SSA transformation for 2-way joins.
use super::super::cleanup::{expr_has_side_effects, prune_unused_temp_bindings};
use super::super::analysis::defuse::DefUseMap;
use super::super::*;
use std::collections::{HashMap, HashSet};

// ── Copy Propagation ─────────────────────────────────────────────────────────

/// Propagate copies `x = y` (where both x and y are named variables and x is a
/// pure temporary with exactly one definition) by replacing every rvalue use of
/// `x` with `y` and removing the assignment.
///
/// Returns `true` if any substitution was made.
pub(crate) fn copy_propagation_pass(func: &mut HirFunction) -> bool {
    // Step 1: collect names of pure temporaries.
    let temp_names: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| matches!(b.origin, Some(NirBindingOrigin::Temp)))
        .map(|b| b.name.clone())
        .collect();
    if temp_names.is_empty() {
        return false;
    }

    // Step 2: build a map of single-definition copies: temp_name → rhs_name.
    //   Only consider assignments `x = Var(y)` where x is a pure temp.
    let def_count = count_definitions_in_stmts(&func.body, &temp_names);
    let mut copy_map: HashMap<String, String> = HashMap::new();
    collect_copies(&func.body, &temp_names, &def_count, &mut copy_map);

    if copy_map.is_empty() {
        return false;
    }

    // Step 3: validate that the source variable `y` is not re-assigned
    // between the copy definition and any of its uses (conservative guard:
    // reject `y` if it is itself a pure temp with >1 definition, since that
    // means it could be re-defined on some paths).
    copy_map.retain(|_x, y| {
        let y_def_count = def_count.get(y.as_str()).copied().unwrap_or(0);
        // Allow if y is never re-defined (0 or 1 definition and y is not a
        // pure temp with multiple writes that could create a hazard).
        y_def_count <= 1
    });

    if copy_map.is_empty() {
        return false;
    }

    // Step 4: remove the copy assignments from the body and substitute y for x.
    let mut changed = false;
    remove_copy_assigns(&mut func.body, &copy_map, &mut changed);
    substitute_copies_in_stmts(&mut func.body, &copy_map, &mut changed);

    if changed {
        prune_unused_temp_bindings(func);
    }
    changed
}

/// Count definition sites (assignments to LHS Var(name)) for each name in
/// `temp_names` across the entire body.
fn count_definitions_in_stmts(
    stmts: &[HirStmt],
    temp_names: &HashSet<String>,
) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for stmt in stmts {
        count_defs_stmt(stmt, temp_names, &mut counts);
    }
    counts
}

fn count_defs_stmt(
    stmt: &HirStmt,
    temps: &HashSet<String>,
    counts: &mut HashMap<String, usize>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            // Count definitions for ALL variables (not just temps) so we can
            // validate the source variable y.
            *counts.entry(name.clone()).or_default() += 1;
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                count_defs_stmt(s, temps, counts);
            }
            for s in else_body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for s in body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::For { init, update, body, .. } => {
            if let Some(i) = init {
                count_defs_stmt(i, temps, counts);
            }
            if let Some(u) = update {
                count_defs_stmt(u, temps, counts);
            }
            for s in body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    count_defs_stmt(s, temps, counts);
                }
            }
            for s in default {
                count_defs_stmt(s, temps, counts);
            }
        }
        _ => {}
    }
}

/// Collect copy assignments `x = Var(y)` where x is a pure temp with exactly
/// one definition.
fn collect_copies(
    stmts: &[HirStmt],
    temp_names: &HashSet<String>,
    def_count: &HashMap<String, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        collect_copies_stmt(stmt, temp_names, def_count, copy_map);
    }
}

fn collect_copies_stmt(
    stmt: &HirStmt,
    temp_names: &HashSet<String>,
    def_count: &HashMap<String, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Var(src),
        } if temp_names.contains(name.as_str())
            && def_count.get(name.as_str()).copied().unwrap_or(0) == 1
            && name != src =>
        {
            copy_map.insert(name.clone(), src.clone());
        }
        HirStmt::Block(stmts) => collect_copies(stmts, temp_names, def_count, copy_map),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_copies(then_body, temp_names, def_count, copy_map);
            collect_copies(else_body, temp_names, def_count, copy_map);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_copies(body, temp_names, def_count, copy_map);
        }
        HirStmt::For { init, update, body, .. } => {
            if let Some(i) = init {
                collect_copies_stmt(i, temp_names, def_count, copy_map);
            }
            if let Some(u) = update {
                collect_copies_stmt(u, temp_names, def_count, copy_map);
            }
            collect_copies(body, temp_names, def_count, copy_map);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_copies(&case.body, temp_names, def_count, copy_map);
            }
            collect_copies(default, temp_names, def_count, copy_map);
        }
        _ => {}
    }
}

/// Remove copy assignments `x = y` from the body where x is in copy_map.
fn remove_copy_assigns(
    stmts: &mut Vec<HirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_copy_assigns_nested(stmt, copy_map, changed);
    }
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Var(_),
        } = stmt
        {
            if copy_map.contains_key(name.as_str()) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn remove_copy_assigns_nested(
    stmt: &mut HirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Block(stmts) => remove_copy_assigns(stmts, copy_map, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_copy_assigns(then_body, copy_map, changed);
            remove_copy_assigns(else_body, copy_map, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_copy_assigns(body, copy_map, changed);
        }
        HirStmt::For { init, update, body, .. } => {
            if let Some(i) = init {
                remove_copy_assigns_nested(i, copy_map, changed);
            }
            if let Some(u) = update {
                remove_copy_assigns_nested(u, copy_map, changed);
            }
            remove_copy_assigns(body, copy_map, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_copy_assigns(&mut case.body, copy_map, changed);
            }
            remove_copy_assigns(default, copy_map, changed);
        }
        _ => {}
    }
}

/// Substitute every rvalue occurrence of `x` (keys of copy_map) with its
/// source `y` (values of copy_map) throughout all expressions.
fn substitute_copies_in_stmts(
    stmts: &mut Vec<HirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        substitute_copies_in_stmt(stmt, copy_map, changed);
    }
}

fn substitute_copies_in_stmt(
    stmt: &mut HirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            substitute_copies_lvalue(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            substitute_copies_expr(va_list, copy_map, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            substitute_copies_expr(expr, copy_map, changed);
        }
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
        HirStmt::Block(stmts) => substitute_copies_in_stmts(stmts, copy_map, changed),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(then_body, copy_map, changed);
            substitute_copies_in_stmts(else_body, copy_map, changed);
        }
        HirStmt::While { cond, body } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(body, copy_map, changed);
        }
        HirStmt::DoWhile { body, cond } => {
            substitute_copies_in_stmts(body, copy_map, changed);
            substitute_copies_expr(cond, copy_map, changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                substitute_copies_in_stmt(i, copy_map, changed);
            }
            if let Some(c) = cond {
                substitute_copies_expr(c, copy_map, changed);
            }
            if let Some(u) = update {
                substitute_copies_in_stmt(u, copy_map, changed);
            }
            substitute_copies_in_stmts(body, copy_map, changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            substitute_copies_expr(expr, copy_map, changed);
            for case in cases.iter_mut() {
                substitute_copies_in_stmts(&mut case.body, copy_map, changed);
            }
            substitute_copies_in_stmts(default, copy_map, changed);
        }
    }
}

fn substitute_copies_lvalue(
    lhs: &mut HirLValue,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => substitute_copies_expr(ptr, copy_map, changed),
        HirLValue::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
    }
}

fn substitute_copies_expr(
    expr: &mut HirExpr,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        HirExpr::Var(name) => {
            if let Some(src) = copy_map.get(name.as_str()) {
                *name = src.clone();
                *changed = true;
            }
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            substitute_copies_expr(inner, copy_map, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_copies_expr(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                substitute_copies_expr(a, copy_map, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
    }
}

// ── Join Variable Coalescing ──────────────────────────────────────────────────

/// Detect 2-way join patterns: an if-else structure where both the then-branch
/// and the else-branch end with an assignment to the same set of variables.
/// Rename subsequent uses of those variables to a single canonical name
/// (the one from the then-branch), eliminating redundant parallel assignments.
///
/// Classic pattern:
/// ```text
/// if (cond) { v_then = expr_a; } else { v_else = expr_b; }
/// use(v_then);    ← v_then is the canonical name
/// ```
///
/// After coalescing, the else-branch assignment is renamed so that code after
/// the if-else consistently uses the same variable name whether it came from
/// the then-branch or the else-branch.
///
/// Returns `true` if any renaming was made.
pub(crate) fn join_coalescing_pass(func: &mut HirFunction) -> bool {
    let temp_names: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| matches!(b.origin, Some(NirBindingOrigin::Temp)))
        .map(|b| b.name.clone())
        .collect();

    if temp_names.is_empty() {
        return false;
    }

    let map = DefUseMap::build(&func.body);
    let mut rename_map: HashMap<String, String> = HashMap::new();
    collect_join_renames(&func.body, &temp_names, &map, &mut rename_map);

    if rename_map.is_empty() {
        return false;
    }

    let mut changed = false;
    // Apply renames: wherever we see an assignment `else_var = rhs`, rename
    // `else_var` to `then_var` in the LHS (inside the else-branch).
    // Also substitute rvalue uses of `else_var` with `then_var` everywhere.
    apply_join_renames(&mut func.body, &rename_map, &mut changed);

    if changed {
        prune_unused_temp_bindings(func);
    }
    changed
}

/// Walk the statement list looking for If statements that have matching
/// last-assignments in both branches.
fn collect_join_renames(
    stmts: &[HirStmt],
    temp_names: &HashSet<String>,
    map: &DefUseMap,
    rename_map: &mut HashMap<String, String>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } if !then_body.is_empty() && !else_body.is_empty() => {
                // Find the last assignments in each branch.
                let then_assigns = last_assigns(then_body, temp_names);
                let else_assigns = last_assigns(else_body, temp_names);

                // For each (then_var, else_var) pair where both are pure temps
                // with the same type and else_var only ever appears in the
                // else-branch after this point (not used independently
                // elsewhere), we can coalesce: rename else_var → then_var.
                for (then_var, then_ty) in &then_assigns {
                    for (else_var, else_ty) in &else_assigns {
                        if then_var == else_var {
                            continue;
                        }
                        if then_ty != else_ty {
                            continue;
                        }
                        // Only coalesce if else_var is NOT used outside the
                        // else-branch (other than in subsequent stmts where
                        // then_var is also used — this is an approximation).
                        let else_uses_total =
                            map.use_count.get(else_var.as_str()).copied().unwrap_or(0);
                        let else_uses_after = count_uses_after(stmts, idx + 1, else_var);
                        // If all remaining uses of else_var are in the
                        // statements that follow this If (not inside the
                        // else-branch itself), they can be replaced by
                        // then_var.  The else-branch use is the definition
                        // site (assignment), which is not counted in use_count.
                        let else_uses_in_branch =
                            else_uses_total.saturating_sub(else_uses_after);
                        if else_uses_in_branch == 0 {
                            rename_map.insert(else_var.clone(), then_var.clone());
                        }
                    }
                }

                // Recurse into branches.
                collect_join_renames(then_body, temp_names, map, rename_map);
                collect_join_renames(else_body, temp_names, map, rename_map);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_join_renames(then_body, temp_names, map, rename_map);
                collect_join_renames(else_body, temp_names, map, rename_map);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            HirStmt::For { init: _, body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_join_renames(&case.body, temp_names, map, rename_map);
                }
                collect_join_renames(default, temp_names, map, rename_map);
            }
            _ => {}
        }
    }
}

/// Return (name, type_repr) pairs for the LAST assignment to each pure temp
/// within a flat statement list.
fn last_assigns(stmts: &[HirStmt], temp_names: &HashSet<String>) -> Vec<(String, String)> {
    let mut seen: HashMap<String, String> = HashMap::new();
    for stmt in stmts {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
        {
            if temp_names.contains(name.as_str()) {
                seen.insert(name.clone(), type_repr(rhs));
            }
        }
    }
    seen.into_iter().collect()
}

/// Quick structural type fingerprint for an expression (used to gate
/// coalescing by compatible assignment shapes).
fn type_repr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Const(_, ty) | HirExpr::Cast { ty, .. } => format!("{ty:?}"),
        HirExpr::Var(_) => "var".to_string(),
        HirExpr::Binary { ty, .. } | HirExpr::Unary { ty, .. } => format!("{ty:?}"),
        HirExpr::Load { ty, .. } => format!("load_{ty:?}"),
        _ => "other".to_string(),
    }
}

/// Count uses of `name` in stmts[start_idx..].
fn count_uses_after(stmts: &[HirStmt], start_idx: usize, name: &str) -> usize {
    stmts[start_idx.min(stmts.len())..]
        .iter()
        .map(|s| count_uses_in_stmt_flat(s, name))
        .sum()
}

fn count_uses_in_stmt_flat(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs: _, rhs } => count_var_in_expr(rhs, name),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => count_var_in_expr(expr, name),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_in_expr(cond, name)
                + then_body.iter().map(|s| count_uses_in_stmt_flat(s, name)).sum::<usize>()
                + else_body.iter().map(|s| count_uses_in_stmt_flat(s, name)).sum::<usize>()
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            count_var_in_expr(cond, name)
                + body.iter().map(|s| count_uses_in_stmt_flat(s, name)).sum::<usize>()
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().map_or(0, |s| count_uses_in_stmt_flat(s, name))
                + cond.as_ref().map_or(0, |e| count_var_in_expr(e, name))
                + update.as_deref().map_or(0, |s| count_uses_in_stmt_flat(s, name))
                + body.iter().map(|s| count_uses_in_stmt_flat(s, name)).sum::<usize>()
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_in_expr(expr, name)
                + cases
                    .iter()
                    .flat_map(|c| &c.body)
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
                + default.iter().map(|s| count_uses_in_stmt_flat(s, name)).sum::<usize>()
        }
        _ => 0,
    }
}

fn count_var_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) => usize::from(n.as_str() == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_var_in_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_in_expr(lhs, name) + count_var_in_expr(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_var_in_expr(a, name)).sum(),
        HirExpr::Index { base, index, .. } => {
            count_var_in_expr(base, name) + count_var_in_expr(index, name)
        }
    }
}

/// Apply renames: in the else-branch of each If, rename LHS `else_var` to
/// `then_var`. Also rename rvalue uses of `else_var` to `then_var` everywhere
/// outside the else-branch (and inside other branches).
fn apply_join_renames(
    stmts: &mut Vec<HirStmt>,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        apply_join_renames_stmt(stmt, rename_map, changed);
    }
}

fn apply_join_renames_stmt(
    stmt: &mut HirStmt,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        HirStmt::Assign { lhs, rhs } => {
            apply_join_renames_expr(rhs, rename_map, changed);
            // Also rename inside index/deref lvalues.
            apply_join_renames_lvalue(lhs, rename_map, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            apply_join_renames_expr(va_list, rename_map, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            apply_join_renames_expr(expr, rename_map, changed);
        }
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
        HirStmt::Block(stmts) => apply_join_renames(stmts, rename_map, changed),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(then_body, rename_map, changed);
            apply_join_renames(else_body, rename_map, changed);
        }
        HirStmt::While { cond, body } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(body, rename_map, changed);
        }
        HirStmt::DoWhile { body, cond } => {
            apply_join_renames(body, rename_map, changed);
            apply_join_renames_expr(cond, rename_map, changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                apply_join_renames_stmt(i, rename_map, changed);
            }
            if let Some(c) = cond {
                apply_join_renames_expr(c, rename_map, changed);
            }
            if let Some(u) = update {
                apply_join_renames_stmt(u, rename_map, changed);
            }
            apply_join_renames(body, rename_map, changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            apply_join_renames_expr(expr, rename_map, changed);
            for case in cases.iter_mut() {
                apply_join_renames(&mut case.body, rename_map, changed);
            }
            apply_join_renames(default, rename_map, changed);
        }
    }
}

fn apply_join_renames_lvalue(
    lhs: &mut HirLValue,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => apply_join_renames_expr(ptr, rename_map, changed),
        HirLValue::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
    }
}

fn apply_join_renames_expr(
    expr: &mut HirExpr,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        HirExpr::Var(name) => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            apply_join_renames_expr(inner, rename_map, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            apply_join_renames_expr(lhs, rename_map, changed);
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                apply_join_renames_expr(a, rename_map, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
    }
}
