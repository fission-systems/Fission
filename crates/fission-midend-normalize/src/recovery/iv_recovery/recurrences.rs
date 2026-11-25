use super::loop_expressions::{expr_vars, strip_casts};
use crate::HashSet;
use crate::prelude::*;

/// Check that an expression only contains constants or variables NOT in
/// `loop_variant` — i.e., the expression is loop-invariant.
pub(super) fn is_loop_invariant(expr: &PreHirExpr, loop_variant: &HashSet<String>) -> bool {
    let mut vars = HashSet::default();
    expr_vars(expr, &mut vars);
    vars.is_disjoint(loop_variant)
}

/// Detect a single `v = v ± k` or `v = k ± v` update for variable `v` in
/// the loop body.  Returns the update statement index and whether it is the
/// LAST statement.
pub(super) fn find_iv_update(
    body: &[PreHirStmt],
    var: &str,
    loop_variant: &HashSet<String>,
) -> Option<(usize, bool)> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } = stmt
        {
            if lhs_name != var {
                continue;
            }
            // Expect rhs = Var(v) ± k (linear) or affine v*k'+k'' (see below).
            if is_iv_update(rhs, var, loop_variant) {
                if found.is_some() {
                    return None; // multiple updates → bail
                }
                found = Some(i);
            }
        }
    }
    let idx = found?;
    Some((idx, idx == body.len() - 1))
}

/// Linear or affine induction update: `v = v±k`, or `v = v*C+k` with `C`,`k`
/// loop-invariant (integer affine recurrence on a single variable).
fn is_iv_update(expr: &PreHirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    is_linear_update_of(expr, var, loop_variant)
        || is_affine_mul_add_update(expr, var, loop_variant)
}

/// Like `is_iv_update`, but resolves through intermediate variable definitions
/// in the loop body.  For example, given `i = t` where `t = i + 1`, the direct
/// check fails (the RHS is just `Var("t")`), but this function resolves `t`
/// through its unique definition and finds the `i + 1` recurrence.
pub(super) fn is_iv_update_dataflow(
    expr: &PreHirExpr,
    var: &str,
    loop_variant: &HashSet<String>,
    body: &[PreHirStmt],
    depth: usize,
) -> bool {
    if depth >= 4 {
        return false;
    }
    // Direct check first.
    if is_iv_update(expr, var, loop_variant) {
        return true;
    }
    // If expr is a single variable (possibly wrapped in casts), resolve through
    // its unique definition in the loop body.
    match strip_casts(expr) {
        PreHirExpr::Var(name) if name != var => {
            if let Some(def_expr) = find_unique_definition_in_body(body, name) {
                return is_iv_update_dataflow(def_expr, var, loop_variant, body, depth + 1);
            }
            false
        }
        _ => false,
    }
}

/// Return true if `expr` is of the form `Var(v) op k` or `k op Var(v)` where
/// `op ∈ {Add, Sub}` and `k` is loop-invariant.
fn is_linear_update_of(expr: &PreHirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, .. }
            if matches!(op, PreHirBinaryOp::Add | PreHirBinaryOp::Sub) =>
        {
            let lhs_is_var = matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == var);
            let rhs_is_var = matches!(rhs.as_ref(), PreHirExpr::Var(n) if n == var);
            if lhs_is_var && is_loop_invariant(rhs, loop_variant) {
                return true;
            }
            if rhs_is_var && is_loop_invariant(lhs, loop_variant) {
                return true;
            }
            false
        }
        // Allow a Cast wrapping a linear update (sign extension on IV).
        PreHirExpr::Cast { expr: inner, .. } => is_linear_update_of(inner, var, loop_variant),
        _ => false,
    }
}

/// `v = v * C + k` or `v = k + v * C` (and commutative mul operand order), with
/// `C` and `k` loop-invariant scalars.
fn is_affine_mul_add_update(expr: &PreHirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => is_affine_mul_add_update(inner, var, loop_variant),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let mul_on_v = |m: &PreHirExpr| mul_var_times_invariant(m, var, loop_variant);
            let inv = |e: &PreHirExpr| is_loop_invariant_scalar(e, loop_variant);
            (mul_on_v(lhs) && inv(rhs)) || (mul_on_v(rhs) && inv(lhs))
        }
        _ => false,
    }
}

/// `v * e` or `e * v` where `e` has no loop-variant variables.
fn mul_var_times_invariant(expr: &PreHirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => mul_var_times_invariant(inner, var, loop_variant),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let lv = matches!(lhs.as_ref(), PreHirExpr::Var(n) if n == var);
            let rv = matches!(rhs.as_ref(), PreHirExpr::Var(n) if n == var);
            (lv && is_loop_invariant(rhs, loop_variant))
                || (rv && is_loop_invariant(lhs, loop_variant))
        }
        _ => false,
    }
}

/// Constants or expressions with no loop-variant variables (same as loop-invariant).
fn is_loop_invariant_scalar(expr: &PreHirExpr, loop_variant: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr: inner, .. } => is_loop_invariant_scalar(inner, loop_variant),
        _ => is_loop_invariant(expr, loop_variant),
    }
}

/// Collect the set of variables modified inside the loop body (excluding
/// variables modified only in nested loops, which are their own scope).
pub(super) fn loop_variant_vars(body: &[PreHirStmt]) -> HashSet<String> {
    let mut vars = HashSet::default();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    vars
}

fn loop_variant_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                loop_variant_stmt(s, out);
            }
            for s in else_body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default.iter() {
                loop_variant_stmt(s, out);
            }
        }
        // Nested loops are their own scope for variant purposes.
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {}
        _ => {}
    }
}

/// Scan `stmts[0..loop_idx]` backwards for a single assignment to `var` that
/// is not separated by a label, goto, or another modification of `var`.
pub(super) fn find_init_before(stmts: &[PreHirStmt], loop_idx: usize, var: &str) -> Option<usize> {
    let mut scan = loop_idx;
    // scan backwards, limited to the immediately preceding statement
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } if name == var => {
                return Some(scan);
            }
            // Any control flow or side-effecting statement stops the search.
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::If { .. }
            | PreHirStmt::While { .. }
            | PreHirStmt::DoWhile { .. }
            | PreHirStmt::For { .. }
            | PreHirStmt::Switch { .. }
            | PreHirStmt::Expr(_) => break,
            // Pure assignments to other variables are fine to skip.
            PreHirStmt::Assign { .. } => break,
            _ => {}
        }
    }
    None
}

/// Simple check to find the single top-level assignment to a variable in the loop body.
fn find_iv_update_simple(body: &[PreHirStmt], var: &str) -> Option<usize> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } = stmt
        {
            if lhs_name == var {
                if found.is_some() {
                    return None; // multiple updates → bail
                }
                found = Some(i);
            }
        }
    }
    found
}

/// Find a unique definition of a variable inside the loop body,
/// recursively checking top-level statements, nested blocks, and If statement branches.
fn find_unique_definition_in_body<'a>(body: &'a [PreHirStmt], var: &str) -> Option<&'a PreHirExpr> {
    let mut found: Option<&'a PreHirExpr> = None;
    for stmt in body {
        if let Some(rhs) = find_assignment_in_stmt(stmt, var) {
            if found.is_some() {
                return None; // multiple definitions → not unique
            }
            found = Some(rhs);
        }
    }
    found
}

fn find_assignment_in_stmt<'a>(stmt: &'a PreHirStmt, var: &str) -> Option<&'a PreHirExpr> {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == var => Some(rhs),
        PreHirStmt::Block(body) => {
            let mut found: Option<&'a PreHirExpr> = None;
            for s in body.iter() {
                if let Some(rhs) = find_assignment_in_stmt(s, var) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(rhs);
                }
            }
            found
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let mut found: Option<&'a PreHirExpr> = None;
            for s in then_body.iter().chain(else_body.iter()) {
                if let Some(rhs) = find_assignment_in_stmt(s, var) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(rhs);
                }
            }
            found
        }
        _ => None,
    }
}

/// Recursively checks if `loop_var` feeds the `expr` (iterator statement RHS) using dataflow path-walking.
fn test_iterate_form(body: &[PreHirStmt], update_idx: usize, loop_var: &str) -> bool {
    let update_stmt = &body[update_idx];
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs_name),
        rhs,
    } = update_stmt
    else {
        return false;
    };
    if lhs_name != loop_var {
        return false;
    }

    let mut visited = HashSet::default();
    test_iterate_form_expr(body, rhs, loop_var, &mut visited, 0)
}

fn test_iterate_form_expr(
    body: &[PreHirStmt],
    expr: &PreHirExpr,
    loop_var: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    if depth >= 4 {
        return false;
    }

    let mut vars = HashSet::default();
    expr_vars(expr, &mut vars);

    if vars.contains(loop_var) {
        return true;
    }

    for var in vars {
        if visited.insert(var.clone()) {
            if let Some(def_expr) = find_unique_definition_in_body(body, &var) {
                if test_iterate_form_expr(body, def_expr, loop_var, visited, depth + 1) {
                    return true;
                }
            }
            visited.remove(&var);
        }
    }

    false
}

/// Robust dataflow path-walking starting from loop condition variables
/// to identify the actual controlling loop induction variable.
pub(super) fn find_loop_variable_dataflow(
    stmts: &[PreHirStmt],
    loop_idx: usize,
    body: &[PreHirStmt],
    cond: &PreHirExpr,
    loop_variant: &HashSet<String>,
) -> Option<(String, usize)> {
    let mut cond_vars = HashSet::default();
    expr_vars(cond, &mut cond_vars);

    for start_var in cond_vars {
        let mut visited = HashSet::default();
        if let Some(res) = path_walk_var(
            stmts,
            loop_idx,
            body,
            &start_var,
            loop_variant,
            &mut visited,
            0,
        ) {
            return Some(res);
        }
    }
    None
}

fn path_walk_var(
    stmts: &[PreHirStmt],
    loop_idx: usize,
    body: &[PreHirStmt],
    curr_var: &str,
    loop_variant: &HashSet<String>,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Option<(String, usize)> {
    if depth >= 4 {
        return None;
    }
    if !visited.insert(curr_var.to_string()) {
        return None;
    }

    let has_init = find_init_before(stmts, loop_idx, curr_var).is_some();
    let has_update = find_iv_update_simple(body, curr_var).is_some();

    if has_init && has_update {
        let update_idx = find_iv_update_simple(body, curr_var).unwrap();
        let update_stmt = &body[update_idx];
        if let PreHirStmt::Assign { rhs, .. } = update_stmt {
            if is_iv_update_dataflow(rhs, curr_var, loop_variant, body, 0)
                && test_iterate_form(body, update_idx, curr_var)
            {
                return Some((curr_var.to_string(), update_idx));
            }
        }
    }

    // Otherwise, walk the definitions of curr_var to find its inputs.
    if let Some(def_expr) = find_unique_definition_in_body(body, curr_var) {
        let mut next_vars = HashSet::default();
        expr_vars(def_expr, &mut next_vars);
        for next_var in next_vars {
            if let Some(res) = path_walk_var(
                stmts,
                loop_idx,
                body,
                &next_var,
                loop_variant,
                visited,
                depth + 1,
            ) {
                return Some(res);
            }
        }
    }

    visited.remove(curr_var);
    None
}
