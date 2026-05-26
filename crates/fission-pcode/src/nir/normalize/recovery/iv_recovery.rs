/// Loop induction variable (IV) recovery + break/continue accuracy pass.
///
/// ## Part A — SCEV-lite: Linear Recurrence → `for` upgrade
///
/// After `apply_for_loop_folding` has already converted simple `while` loops
/// to `for` loops based on *syntactic* patterns (init/update adjacent in the
/// AST), some loops are still `While { cond, body }` because the initial
/// `for_loop_folding` could not identify a clear preceding assignment.
///
/// This pass looks deeper using a data-flow criterion:
///
/// 1. Identify all variables that appear in the loop condition (`cond_vars`).
/// 2. For each cond_var, check if:
///    (a) There is exactly one assignment to it *before* the loop in the same
///        flat statement list (the "init" assignment).
///    (b) The loop body contains exactly one assignment of the form
///        `v = v ± k` where `k` does not mention any loop-variant variable
///        (the "update").
/// 3. If both are found and there is no `Continue` in the loop (updating
///    semantics are preserved), convert the `While` to `For`.
///
/// This is a conservative subset of full SCEV: only *linear* recurrences with
/// loop-invariant steps, no irreducible or multi-update cases.  The algorithm
/// is entirely syntax-driven on the HIR and has no binary-specific thresholds.
/// ## Part B — Break/Continue recovery
///
/// Inside every loop body (While/DoWhile/For), scan for:
///
/// ```text
/// If { cond, then_body: [Goto(label)], else_body: [] }
/// ```
///
/// where:
/// - `label` is a label that appears *after* the loop (exit) → replace with
///   `If { cond, then_body: [Break], else_body: [] }` and remove the now-dead
///   label if it has no other predecessors.
/// - `label` is a label that appears immediately before the loop (loop head)
///   → replace with `If { cond, then_body: [Continue], else_body: [] }`.
///
/// The detection is structural and label-based; no CFG reachability analysis
/// is required.  The replacement is only performed when the label has exactly
/// one incoming `Goto` (the one being replaced), so the label can also be
/// removed afterwards.
///
/// ## Part C — Tail label loops → break-guarded `for (;;)`
///
/// Structuring can leave a reducible tail loop as:
///
/// ```text
/// L:
///   body
///   if (continue_cond) goto L
/// ```
///
/// when interleaved labels inside the body prevent direct loop emission.  When
/// `L` has exactly one incoming goto and every remaining body goto is local to
/// the loop body, this pass lowers the shape to:
///
/// ```text
/// for (;;) {
///   body
///   if (!continue_cond) break;
/// }
/// ```
///
/// This preserves do-while entry semantics without requiring a separate
/// preheader proof, while still removing the outer label/goto recurrence.
use super::super::*;
use crate::nir::support::expr_type;
use std::collections::{HashMap, HashSet};

// ── Part B — Break/Continue recovery ─────────────────────────────────────────

/// Count occurrences of each label name as a Goto target in a statement list.
fn count_goto_targets(stmts: &[HirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_goto_targets_stmt(stmt, counts);
    }
}

fn count_goto_targets_stmt(stmt: &HirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        HirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            count_goto_targets(then_body, counts);
            count_goto_targets(else_body, counts);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            count_goto_targets(body, counts);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_goto_targets(&case.body, counts);
            }
            count_goto_targets(default, counts);
        }
        _ => {}
    }
}

/// Collect all label *definitions* in a statement list.
fn collect_labels(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_labels_stmt(stmt, out);
    }
}

fn collect_labels_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Label(label) => {
            out.insert(label.clone());
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_labels(then_body, out);
            collect_labels(else_body, out);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            collect_labels(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_labels(&case.body, out);
            }
            collect_labels(default, out);
        }
        _ => {}
    }
}

/// Within a loop body (already extracted from the parent statement list),
/// replace `If { then_body: [Goto(exit_label)], else_body: [] }` with
/// `If { then_body: [Break], ... }` when `exit_label` is an "after-loop" label.
///
/// `after_labels` = set of labels defined *after* the loop in the same flat
/// statement list.  `head_labels` = labels defined immediately before the loop.
/// `goto_counts` = number of times each label is reached by a Goto in the
/// *entire function* (so we know if the label becomes dead after replacement).
fn recover_break_continue_in_body(
    body: &mut Vec<HirStmt>,
    after_labels: &HashSet<String>,
    head_labels: &HashSet<String>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    // Recurse into nested loops first (inner loops are handled before outer).
    for stmt in body.iter_mut() {
        match stmt {
            HirStmt::While { body: inner, .. }
            | HirStmt::DoWhile { body: inner, .. }
            | HirStmt::For { body: inner, .. } => {
                // For nested loops we collect their own after/head labels.
                // We skip nested loops here — the outer pass will recurse on
                // the whole function again if changed.
                let _ = inner; // recurse handled at top level
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < body.len() {
        let do_break = if let HirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[i]
        {
            if else_body.is_empty() {
                if let [HirStmt::Goto(lbl)] = then_body.as_slice() {
                    if after_labels.contains(lbl) {
                        // Only replace if this is the only goto to this label.
                        goto_counts.get(lbl).copied().unwrap_or(0) == 1
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        let do_continue = if !do_break {
            if let HirStmt::If {
                then_body,
                else_body,
                ..
            } = &body[i]
            {
                if else_body.is_empty() {
                    if let [HirStmt::Goto(lbl)] = then_body.as_slice() {
                        head_labels.contains(lbl) && goto_counts.get(lbl).copied().unwrap_or(0) == 1
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if do_break {
            if let HirStmt::If { then_body, .. } = &mut body[i] {
                then_body.clear();
                then_body.push(HirStmt::Break);
                changed = true;
            }
        } else if do_continue {
            if let HirStmt::If { then_body, .. } = &mut body[i] {
                then_body.clear();
                then_body.push(HirStmt::Continue);
                changed = true;
            }
        }
        i += 1;
    }
    changed
}

/// Top-level break/continue recovery for a flat statement list.
/// Scans every While/DoWhile/For loop and replaces eligible Goto patterns
/// inside their bodies.
fn apply_break_continue_in_stmts(
    stmts: &mut Vec<HirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let n = stmts.len();
    for loop_idx in 0..n {
        // Determine labels that appear after this loop (exit targets).
        let mut after_labels = HashSet::new();
        for stmt in stmts.iter().skip(loop_idx + 1) {
            collect_labels_stmt(stmt, &mut after_labels);
        }
        // Head labels: labels immediately before the loop.
        let mut head_labels = HashSet::new();
        if loop_idx > 0 {
            collect_labels_stmt(&stmts[loop_idx - 1], &mut head_labels);
        }

        let is_loop = matches!(
            &stmts[loop_idx],
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. }
        );
        if !is_loop {
            continue;
        }

        let body = match &mut stmts[loop_idx] {
            HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => body,
            _ => unreachable!(),
        };
        changed |= recover_break_continue_in_body(body, &after_labels, &head_labels, goto_counts);
    }

    // Recurse into If/Block/Switch to catch loops nested there.
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_break_continue_in_stmts(then_body, goto_counts);
                changed |= apply_break_continue_in_stmts(else_body, goto_counts);
            }
            HirStmt::Block(body) => {
                changed |= apply_break_continue_in_stmts(body, goto_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_break_continue_in_stmts(&mut case.body, goto_counts);
                }
                changed |= apply_break_continue_in_stmts(default, goto_counts);
            }
            HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                // Recurse for nested loops.
                changed |= apply_break_continue_in_stmts(body, goto_counts);
            }
            _ => {}
        }
    }

    changed
}

// ── Part C — Tail label loops → break-guarded `for (;;)` ─────────────────────

fn invert_condition(expr: HirExpr) -> HirExpr {
    match expr {
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let inverted = match op {
                HirBinaryOp::Eq => Some(HirBinaryOp::Ne),
                HirBinaryOp::Ne => Some(HirBinaryOp::Eq),
                HirBinaryOp::Lt => Some(HirBinaryOp::Ge),
                HirBinaryOp::Le => Some(HirBinaryOp::Gt),
                HirBinaryOp::Gt => Some(HirBinaryOp::Le),
                HirBinaryOp::Ge => Some(HirBinaryOp::Lt),
                HirBinaryOp::SLt => Some(HirBinaryOp::SGe),
                HirBinaryOp::SLe => Some(HirBinaryOp::SGt),
                HirBinaryOp::SGt => Some(HirBinaryOp::SLe),
                HirBinaryOp::SGe => Some(HirBinaryOp::SLt),
                _ => None,
            };
            if let Some(op) = inverted {
                HirExpr::Binary { op, lhs, rhs, ty }
            } else {
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Binary { op, lhs, rhs, ty }),
                    ty: NirType::Bool,
                }
            }
        }
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

fn tail_goto_condition(stmt: &HirStmt, label: &str) -> Option<HirExpr> {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() {
        return None;
    }
    matches!(then_body.as_slice(), [HirStmt::Goto(target)] if target == label).then(|| cond.clone())
}

fn collect_loop_body_labels(stmts: &[HirStmt]) -> HashSet<String> {
    let mut labels = HashSet::new();
    collect_labels(stmts, &mut labels);
    labels
}

fn collect_goto_targets_in_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_goto_targets_in_stmt(stmt, out);
    }
}

fn collect_goto_targets_in_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Goto(label) => {
            out.insert(label.clone());
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_goto_targets_in_stmts(then_body, out);
            collect_goto_targets_in_stmts(else_body, out);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => collect_goto_targets_in_stmts(body, out),
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_goto_targets_in_stmts(&case.body, out);
            }
            collect_goto_targets_in_stmts(default, out);
        }
        _ => {}
    }
}

fn has_unscoped_break(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(has_unscoped_break_stmt)
}

fn has_unscoped_break_stmt(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Break => true,
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => has_unscoped_break(then_body) || has_unscoped_break(else_body),
        HirStmt::Block(body) => has_unscoped_break(body),
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| has_unscoped_break(&case.body)) || has_unscoped_break(default)
        }
        HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => false,
        _ => false,
    }
}

fn body_gotos_are_loop_local(body: &[HirStmt]) -> bool {
    let labels = collect_loop_body_labels(body);
    let mut gotos = HashSet::new();
    collect_goto_targets_in_stmts(body, &mut gotos);
    gotos.iter().all(|target| labels.contains(target))
}

fn try_tail_label_loop_to_for(
    stmts: &mut Vec<HirStmt>,
    label_idx: usize,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let HirStmt::Label(label) = &stmts[label_idx] else {
        return false;
    };
    let label = label.clone();
    if goto_counts.get(&label).copied().unwrap_or(0) != 1 {
        return false;
    }

    for tail_idx in label_idx + 1..stmts.len() {
        let Some(continue_cond) = tail_goto_condition(&stmts[tail_idx], &label) else {
            continue;
        };
        let body_slice = &stmts[label_idx + 1..tail_idx];
        if body_slice.is_empty()
            || super::for_loops::stmt_list_contains_continue_pub(body_slice)
            || has_unscoped_break(body_slice)
            || !body_gotos_are_loop_local(body_slice)
        {
            return false;
        }

        let mut body = body_slice.to_vec();
        body.push(HirStmt::If {
            cond: invert_condition(continue_cond),
            then_body: vec![HirStmt::Break],
            else_body: Vec::new(),
        });
        let replacement = HirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        };
        stmts.splice(label_idx..=tail_idx, [replacement]);
        return true;
    }

    false
}

fn apply_tail_label_loop_recovery_in_stmts(
    stmts: &mut Vec<HirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], HirStmt::Label(_))
            && try_tail_label_loop_to_for(stmts, i, goto_counts)
        {
            changed = true;
            continue;
        }

        match &mut stmts[i] {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(then_body, goto_counts);
                changed |= apply_tail_label_loop_recovery_in_stmts(else_body, goto_counts);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(body, goto_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |=
                        apply_tail_label_loop_recovery_in_stmts(&mut case.body, goto_counts);
                }
                changed |= apply_tail_label_loop_recovery_in_stmts(default, goto_counts);
            }
            _ => {}
        }
        i += 1;
    }
    changed
}

// ── Part A — SCEV-lite: enhance While/guarded DoWhile → For ─────────────────

/// Collect all variable names mentioned in an expression.
fn expr_vars(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            out.insert(name.clone());
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => expr_vars(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_vars(lhs, out);
            expr_vars(rhs, out);
        }
        HirExpr::Load { ptr, .. } => expr_vars(ptr, out),
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => expr_vars(base, out),
        HirExpr::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                expr_vars(a, out);
            }
        }
        HirExpr::AggregateCopy { src, .. } => expr_vars(src, out),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_vars(cond, out);
            expr_vars(then_expr, out);
            expr_vars(else_expr, out);
        }
        HirExpr::Const(_, _) => {}
    }
}

/// Check that an expression only contains constants or variables NOT in
/// `loop_variant` — i.e., the expression is loop-invariant.
fn is_loop_invariant(expr: &HirExpr, loop_variant: &HashSet<String>) -> bool {
    let mut vars = HashSet::new();
    expr_vars(expr, &mut vars);
    vars.is_disjoint(loop_variant)
}

/// Detect a single `v = v ± k` or `v = k ± v` update for variable `v` in
/// the loop body.  Returns the update statement index and whether it is the
/// LAST statement.
fn find_iv_update(
    body: &[HirStmt],
    var: &str,
    loop_variant: &HashSet<String>,
) -> Option<(usize, bool)> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
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
fn is_iv_update(expr: &HirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    is_linear_update_of(expr, var, loop_variant)
        || is_affine_mul_add_update(expr, var, loop_variant)
}

/// Like `is_iv_update`, but resolves through intermediate variable definitions
/// in the loop body.  For example, given `i = t` where `t = i + 1`, the direct
/// check fails (the RHS is just `Var("t")`), but this function resolves `t`
/// through its unique definition and finds the `i + 1` recurrence.
fn is_iv_update_dataflow(
    expr: &HirExpr,
    var: &str,
    loop_variant: &HashSet<String>,
    body: &[HirStmt],
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
        HirExpr::Var(name) if name != var => {
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
fn is_linear_update_of(expr: &HirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        HirExpr::Binary { op, lhs, rhs, .. }
            if matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub) =>
        {
            let lhs_is_var = matches!(lhs.as_ref(), HirExpr::Var(n) if n == var);
            let rhs_is_var = matches!(rhs.as_ref(), HirExpr::Var(n) if n == var);
            if lhs_is_var && is_loop_invariant(rhs, loop_variant) {
                return true;
            }
            if rhs_is_var && is_loop_invariant(lhs, loop_variant) {
                return true;
            }
            false
        }
        // Allow a Cast wrapping a linear update (sign extension on IV).
        HirExpr::Cast { expr: inner, .. } => is_linear_update_of(inner, var, loop_variant),
        _ => false,
    }
}

/// `v = v * C + k` or `v = k + v * C` (and commutative mul operand order), with
/// `C` and `k` loop-invariant scalars.
fn is_affine_mul_add_update(expr: &HirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        HirExpr::Cast { expr: inner, .. } => is_affine_mul_add_update(inner, var, loop_variant),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let mul_on_v = |m: &HirExpr| mul_var_times_invariant(m, var, loop_variant);
            let inv = |e: &HirExpr| is_loop_invariant_scalar(e, loop_variant);
            (mul_on_v(lhs) && inv(rhs)) || (mul_on_v(rhs) && inv(lhs))
        }
        _ => false,
    }
}

/// `v * e` or `e * v` where `e` has no loop-variant variables.
fn mul_var_times_invariant(expr: &HirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        HirExpr::Cast { expr: inner, .. } => mul_var_times_invariant(inner, var, loop_variant),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let lv = matches!(lhs.as_ref(), HirExpr::Var(n) if n == var);
            let rv = matches!(rhs.as_ref(), HirExpr::Var(n) if n == var);
            (lv && is_loop_invariant(rhs, loop_variant))
                || (rv && is_loop_invariant(lhs, loop_variant))
        }
        _ => false,
    }
}

/// Constants or expressions with no loop-variant variables (same as loop-invariant).
fn is_loop_invariant_scalar(expr: &HirExpr, loop_variant: &HashSet<String>) -> bool {
    match expr {
        HirExpr::Const(_, _) => true,
        HirExpr::Cast { expr: inner, .. } => is_loop_invariant_scalar(inner, loop_variant),
        _ => is_loop_invariant(expr, loop_variant),
    }
}

fn strip_casts(expr: &HirExpr) -> &HirExpr {
    match expr {
        HirExpr::Cast { expr, .. } => strip_casts(expr),
        _ => expr,
    }
}

fn stripped_var_name(expr: &HirExpr) -> Option<&str> {
    match strip_casts(expr) {
        HirExpr::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

fn vars_equivalent_after_casts(a: &HirExpr, b: &HirExpr) -> bool {
    matches!((stripped_var_name(a), stripped_var_name(b)), (Some(x), Some(y)) if x == y)
}

fn is_zero(expr: &HirExpr) -> bool {
    matches!(strip_casts(expr), HirExpr::Const(0, _))
}

/// Collect the set of variables modified inside the loop body (excluding
/// variables modified only in nested loops, which are their own scope).
fn loop_variant_vars(body: &[HirStmt]) -> HashSet<String> {
    let mut vars = HashSet::new();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    vars
}

fn loop_variant_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                loop_variant_stmt(s, out);
            }
            for s in else_body {
                loop_variant_stmt(s, out);
            }
        }
        HirStmt::Block(body) => {
            for s in body {
                loop_variant_stmt(s, out);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default {
                loop_variant_stmt(s, out);
            }
        }
        // Nested loops are their own scope for variant purposes.
        HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {}
        _ => {}
    }
}

/// Scan `stmts[0..loop_idx]` backwards for a single assignment to `var` that
/// is not separated by a label, goto, or another modification of `var`.
fn find_init_before(stmts: &[HirStmt], loop_idx: usize, var: &str) -> Option<usize> {
    let mut scan = loop_idx;
    // scan backwards, limited to the immediately preceding statement
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if name == var => {
                return Some(scan);
            }
            // Any control flow or side-effecting statement stops the search.
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::If { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Switch { .. }
            | HirStmt::Expr(_) => break,
            // Pure assignments to other variables are fine to skip.
            HirStmt::Assign { .. } => break,
            _ => {}
        }
    }
    None
}

fn find_pointer_end_assignment_before(
    stmts: &[HirStmt],
    loop_idx: usize,
    cursor: &str,
    end: &str,
) -> Option<(usize, HirExpr)> {
    let mut scan = loop_idx;
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs:
                    HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs,
                        rhs,
                        ..
                    },
            } if name == end => {
                if matches!(lhs.as_ref(), HirExpr::Var(name) if name == cursor) {
                    return Some((scan, rhs.as_ref().clone()));
                }
                if matches!(rhs.as_ref(), HirExpr::Var(name) if name == cursor) {
                    return Some((scan, lhs.as_ref().clone()));
                }
                return None;
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Switch { .. }
            | HirStmt::Expr(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => break,
            HirStmt::If { .. } | HirStmt::Assign { .. } | HirStmt::VaStart { .. } => {}
            HirStmt::Block(_) => break,
        }
    }
    None
}

fn single_goto_target(stmts: &[HirStmt]) -> Option<&str> {
    match stmts {
        [HirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

fn labels_after(stmts: &[HirStmt], idx: usize) -> HashSet<String> {
    let mut labels = HashSet::new();
    for stmt in stmts.iter().skip(idx + 1) {
        collect_labels_stmt(stmt, &mut labels);
    }
    labels
}

fn positive_count_loop_cmp(cond: &HirExpr, count: &HirExpr) -> Option<HirBinaryOp> {
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    match op {
        HirBinaryOp::Le | HirBinaryOp::SLe => (vars_equivalent_after_casts(lhs, count)
            && is_zero(rhs))
        .then_some(if matches!(op, HirBinaryOp::SLe) {
            HirBinaryOp::SLt
        } else {
            HirBinaryOp::Lt
        }),
        HirBinaryOp::Ge | HirBinaryOp::SGe => (is_zero(lhs)
            && vars_equivalent_after_casts(rhs, count))
        .then_some(if matches!(op, HirBinaryOp::SGe) {
            HirBinaryOp::SLt
        } else {
            HirBinaryOp::Lt
        }),
        _ => None,
    }
}

fn positive_count_entry_guard_cmp(
    stmts: &[HirStmt],
    loop_idx: usize,
    count: &HirExpr,
    after_labels: &HashSet<String>,
) -> Option<HirBinaryOp> {
    stmts[..loop_idx].iter().find_map(|stmt| {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if !else_body.is_empty() {
            return None;
        }
        let exits_before_loop = single_goto_target(then_body)
            .is_some_and(|label| after_labels.contains(label))
            || matches!(then_body.as_slice(), [HirStmt::Return(Some(expr))] if is_zero(expr));
        if exits_before_loop {
            positive_count_loop_cmp(cond, count)
        } else {
            None
        }
    })
}

fn pointer_cursor_condition(cond: &HirExpr) -> Option<(&str, &str)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (HirExpr::Var(cursor), HirExpr::Var(end)) => Some((cursor.as_str(), end.as_str())),
        _ => None,
    }
}

fn cursor_used_after_loop(stmts: &[HirStmt], loop_idx: usize, cursor: &str) -> bool {
    stmts
        .iter()
        .skip(loop_idx + 1)
        .any(|stmt| count_var_uses_in_stmt(stmt, cursor) > 0)
}

fn count_var_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => count_var_uses(va_list, name),
        HirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        HirStmt::Block(body) | HirStmt::While { body, .. } => body
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        HirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .map_or(0, |stmt| count_var_uses_in_stmt(stmt, name))
                + cond.as_ref().map_or(0, |expr| count_var_uses(expr, name))
                + update
                    .as_deref()
                    .map_or(0, |stmt| count_var_uses_in_stmt(stmt, name))
                + body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses(cond, name)
                + then_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses(expr, name)
                + cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_var_uses_in_stmt(stmt, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => 0,
    }
}

fn count_var_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(_) => 0,
        HirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        HirLValue::FieldAccess { base, .. } => {
            count_var_uses(base, name)
        }
    }
}

fn count_var_uses(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => count_var_uses(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        HirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        HirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_uses(cond, name)
                + count_var_uses(then_expr, name)
                + count_var_uses(else_expr, name)
        }
    }
}

fn collect_names_in_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        HirLValue::Var(name) => {
            out.insert(name.clone());
        }
        HirLValue::Deref { ptr, .. } => expr_vars(ptr, out),
        HirLValue::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        HirLValue::FieldAccess { base, .. } => {
            expr_vars(base, out);
        }
    }
}

fn collect_names_in_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_names_in_lvalue(lhs, out);
            expr_vars(rhs, out);
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => expr_vars(va_list, out),
        HirStmt::Return(Some(expr)) => expr_vars(expr, out),
        HirStmt::Block(body) | HirStmt::While { body, .. } => {
            collect_names_in_stmts(body, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_names_in_stmts(body, out);
            expr_vars(cond, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_names_in_stmt(init, out);
            }
            if let Some(cond) = cond {
                expr_vars(cond, out);
            }
            if let Some(update) = update {
                collect_names_in_stmt(update, out);
            }
            collect_names_in_stmts(body, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_vars(cond, out);
            collect_names_in_stmts(then_body, out);
            collect_names_in_stmts(else_body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_vars(expr, out);
            for case in cases {
                collect_names_in_stmts(&case.body, out);
            }
            collect_names_in_stmts(default, out);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_names_in_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_names_in_stmt(stmt, out);
    }
}

fn fresh_index_name(locals: &[NirBinding], stmts: &[HirStmt]) -> String {
    let mut used = HashSet::new();
    for local in locals {
        used.insert(local.name.clone());
    }
    collect_names_in_stmts(stmts, &mut used);
    for id in 0.. {
        let name = format!("iVar{id}");
        if !used.contains(&name) {
            return name;
        }
    }
    unreachable!()
}

fn index_type_for_count(count_expr: &HirExpr) -> NirType {
    match expr_type(count_expr) {
        NirType::Int { bits, signed } if bits >= 32 => NirType::Int { bits, signed },
        _ => NirType::Int {
            bits: 64,
            signed: true,
        },
    }
}

fn direct_cursor_var(expr: &HirExpr, cursor: &str) -> bool {
    matches!(strip_casts(expr), HirExpr::Var(name) if name == cursor)
}

fn index_var_expr(index_name: &str) -> HirExpr {
    HirExpr::Var(index_name.to_string())
}

fn cursor_index_expr(cursor: &str, index_name: &str, elem_ty: NirType) -> HirExpr {
    HirExpr::Index {
        base: Box::new(HirExpr::Var(cursor.to_string())),
        index: Box::new(index_var_expr(index_name)),
        elem_ty,
    }
}

fn is_one(expr: &HirExpr) -> bool {
    matches!(strip_casts(expr), HirExpr::Const(1, _))
}

fn is_cursor_increment_by_one(stmt: &HirStmt, cursor: &str) -> bool {
    let HirStmt::Assign {
        lhs: HirLValue::Var(lhs),
        rhs,
    } = stmt
    else {
        return false;
    };
    if lhs != cursor {
        return false;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = strip_casts(rhs)
    else {
        return false;
    };
    (direct_cursor_var(lhs, cursor) && is_one(rhs))
        || (direct_cursor_var(rhs, cursor) && is_one(lhs))
}

fn rewrite_cursor_expr_to_index(expr: &mut HirExpr, cursor: &str, index_name: &str) -> bool {
    match expr {
        HirExpr::Var(name) => name != cursor,
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_cursor_expr_to_index(expr, cursor, index_name)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            rewrite_cursor_expr_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_expr_to_index(then_expr, cursor, index_name)
                && rewrite_cursor_expr_to_index(else_expr, cursor, index_name)
        }
        HirExpr::Call { args, .. } => args
            .iter_mut()
            .all(|arg| rewrite_cursor_expr_to_index(arg, cursor, index_name)),
        HirExpr::Load { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *expr = cursor_index_expr(cursor, index_name, ty.clone());
            true
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. } => {
            rewrite_cursor_expr_to_index(ptr, cursor, index_name)
        }
        HirExpr::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
    }
}

fn rewrite_cursor_lvalue_to_index(lhs: &mut HirLValue, cursor: &str, index_name: &str) -> bool {
    match lhs {
        HirLValue::Var(name) => name != cursor,
        HirLValue::Deref { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *lhs = HirLValue::Index {
                base: Box::new(HirExpr::Var(cursor.to_string())),
                index: Box::new(index_var_expr(index_name)),
                elem_ty: ty.clone(),
            };
            true
        }
        HirLValue::Deref { ptr, .. } => rewrite_cursor_expr_to_index(ptr, cursor, index_name),
        HirLValue::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
        HirLValue::FieldAccess { base, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
        }
    }
}

fn rewrite_cursor_stmt_to_index(stmt: &mut HirStmt, cursor: &str, index_name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            rewrite_cursor_lvalue_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => {
            rewrite_cursor_expr_to_index(va_list, cursor, index_name)
        }
        HirStmt::Return(Some(expr)) => rewrite_cursor_expr_to_index(expr, cursor, index_name),
        HirStmt::Block(body) | HirStmt::While { body, .. } => {
            rewrite_cursor_body_to_index(body, cursor, index_name)
        }
        HirStmt::DoWhile { body, cond } => {
            rewrite_cursor_body_to_index(body, cursor, index_name)
                && rewrite_cursor_expr_to_index(cond, cursor, index_name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref_mut()
                .is_none_or(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
                && cond
                    .as_mut()
                    .is_none_or(|expr| rewrite_cursor_expr_to_index(expr, cursor, index_name))
                && update
                    .as_deref_mut()
                    .is_none_or(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
                && rewrite_cursor_body_to_index(body, cursor, index_name)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_body_to_index(then_body, cursor, index_name)
                && rewrite_cursor_body_to_index(else_body, cursor, index_name)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rewrite_cursor_expr_to_index(expr, cursor, index_name)
                && cases
                    .iter_mut()
                    .all(|case| rewrite_cursor_body_to_index(&mut case.body, cursor, index_name))
                && rewrite_cursor_body_to_index(default, cursor, index_name)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => true,
    }
}

fn rewrite_cursor_body_to_index(body: &mut [HirStmt], cursor: &str, index_name: &str) -> bool {
    body.iter_mut()
        .all(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
}

/// Simple check to find the single top-level assignment to a variable in the loop body.
fn find_iv_update_simple(body: &[HirStmt], var: &str) -> Option<usize> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
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
fn find_unique_definition_in_body<'a>(
    body: &'a [HirStmt],
    var: &str,
) -> Option<&'a HirExpr> {
    let mut found: Option<&'a HirExpr> = None;
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

fn find_assignment_in_stmt<'a>(stmt: &'a HirStmt, var: &str) -> Option<&'a HirExpr> {
    match stmt {
        HirStmt::Assign { lhs: HirLValue::Var(lhs_name), rhs } if lhs_name == var => Some(rhs),
        HirStmt::Block(body) => {
            let mut found: Option<&'a HirExpr> = None;
            for s in body {
                if let Some(rhs) = find_assignment_in_stmt(s, var) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(rhs);
                }
            }
            found
        }
        HirStmt::If { then_body, else_body, .. } => {
            let mut found: Option<&'a HirExpr> = None;
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
fn test_iterate_form(
    body: &[HirStmt],
    update_idx: usize,
    loop_var: &str,
) -> bool {
    let update_stmt = &body[update_idx];
    let HirStmt::Assign { lhs: HirLValue::Var(lhs_name), rhs } = update_stmt else {
        return false;
    };
    if lhs_name != loop_var {
        return false;
    }

    let mut visited = HashSet::new();
    test_iterate_form_expr(body, rhs, loop_var, &mut visited, 0)
}

fn test_iterate_form_expr(
    body: &[HirStmt],
    expr: &HirExpr,
    loop_var: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    if depth >= 4 {
        return false;
    }

    let mut vars = HashSet::new();
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
fn find_loop_variable_dataflow(
    stmts: &[HirStmt],
    loop_idx: usize,
    body: &[HirStmt],
    cond: &HirExpr,
    loop_variant: &HashSet<String>,
) -> Option<(String, usize)> {
    let mut cond_vars = HashSet::new();
    expr_vars(cond, &mut cond_vars);

    for start_var in cond_vars {
        let mut visited = HashSet::new();
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
    stmts: &[HirStmt],
    loop_idx: usize,
    body: &[HirStmt],
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
        if let HirStmt::Assign { rhs, .. } = update_stmt {
            if is_iv_update_dataflow(rhs, curr_var, loop_variant, body, 0) && test_iterate_form(body, update_idx, curr_var) {
                return Some((curr_var.to_string(), update_idx));
            }
        }
    }

    // Otherwise, walk the definitions of curr_var to find its inputs.
    if let Some(def_expr) = find_unique_definition_in_body(body, curr_var) {
        let mut next_vars = HashSet::new();
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

/// Try to upgrade a `While` loop at `stmts[loop_idx]` to a `For` loop using
/// SCEV-lite IV detection.  Returns `true` if a transformation was applied.
fn try_scev_upgrade(stmts: &mut Vec<HirStmt>, loop_idx: usize) -> bool {
    let (is_for, init, cond, body) = match &stmts[loop_idx] {
        HirStmt::While { cond, body } => (false, None, cond.clone(), body.clone()),
        HirStmt::For { init, cond, update, body } if update.is_none() => {
            (true, init.clone(), cond.as_ref().cloned().unwrap(), body.clone())
        }
        _ => return false,
    };

    // Safety: no Continue in body (semantics of `update` would change).
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);

    let (var, update_idx) = match find_loop_variable_dataflow(stmts, loop_idx, &body, &cond, &loop_variant) {
        Some(res) => res,
        None => return false,
    };

    // Update must be the last statement in body (or we'd change semantics).
    let is_last = update_idx == body.len() - 1;
    if !is_last {
        let has_subsequent_uses = body[update_idx + 1..]
            .iter()
            .any(|stmt| count_var_uses_in_stmt(stmt, &var) > 0);
        if has_subsequent_uses {
            return false;
        }
    }

    if is_for {
        let mut new_body = body.clone();
        new_body.remove(update_idx);
        let update_stmt = body[update_idx].clone();

        stmts[loop_idx] = HirStmt::For {
            init,
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: new_body,
        };
        return true;
    } else {
        let init_idx = match find_init_before(stmts, loop_idx, &var) {
            Some(i) => i,
            None => return false,
        };

        // Apply transformation.
        let init_stmt = stmts[init_idx].clone();
        let mut new_body = body.clone();
        new_body.remove(update_idx);
        let update_stmt = body[update_idx].clone();

        // Remove the init statement *before* the loop.
        stmts.remove(init_idx);
        // loop_idx shifts down by 1.
        let loop_idx = loop_idx - 1;

        stmts[loop_idx] = HirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: new_body,
        };
        return true;
    }
}

fn try_guarded_dowhile_pointer_iv_upgrade(
    stmts: &mut [HirStmt],
    locals: &mut Vec<NirBinding>,
    loop_idx: usize,
) -> bool {
    let (cond, body) = match &stmts[loop_idx] {
        HirStmt::DoWhile { cond, body } => (cond.clone(), body.clone()),
        _ => return false,
    };
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }
    let Some((cursor, end)) = pointer_cursor_condition(&cond) else {
        return false;
    };
    let loop_variant = loop_variant_vars(&body);
    let Some((update_idx, true)) = find_iv_update(&body, cursor, &loop_variant) else {
        return false;
    };
    let Some((_end_idx, count_expr)) =
        find_pointer_end_assignment_before(stmts, loop_idx, cursor, end)
    else {
        return false;
    };
    let after_labels = labels_after(stmts, loop_idx);
    let Some(count_cmp) =
        positive_count_entry_guard_cmp(stmts, loop_idx, &count_expr, &after_labels)
    else {
        return false;
    };
    if cursor_used_after_loop(stmts, loop_idx, cursor) {
        return false;
    }

    let mut new_body = body;
    let update_stmt = new_body.remove(update_idx);
    let cursor_body_uses = new_body
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, cursor))
        .sum::<usize>();
    let index_name = fresh_index_name(locals, stmts);
    let mut indexed_body = new_body.clone();
    if cursor_body_uses > 0
        && is_cursor_increment_by_one(&update_stmt, cursor)
        && rewrite_cursor_body_to_index(&mut indexed_body, cursor, &index_name)
    {
        let index_ty = index_type_for_count(&count_expr);
        locals.push(NirBinding {
            name: index_name.clone(),
            ty: index_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        let init_stmt = HirStmt::Assign {
            lhs: HirLValue::Var(index_name.clone()),
            rhs: HirExpr::Const(0, index_ty.clone()),
        };
        let cond = HirExpr::Binary {
            op: count_cmp,
            lhs: Box::new(HirExpr::Var(index_name.clone())),
            rhs: Box::new(count_expr),
            ty: NirType::Bool,
        };
        let update_stmt = HirStmt::Assign {
            lhs: HirLValue::Var(index_name.clone()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var(index_name.clone())),
                rhs: Box::new(HirExpr::Const(1, index_ty.clone())),
                ty: index_ty,
            },
        };
        stmts[loop_idx] = HirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: indexed_body,
        };
        return true;
    }

    stmts[loop_idx] = HirStmt::For {
        init: None,
        cond: Some(cond),
        update: Some(Box::new(update_stmt)),
        body: new_body,
    };
    true
}

fn apply_scev_upgrade_in_stmts(stmts: &mut Vec<HirStmt>, locals: &mut Vec<NirBinding>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], HirStmt::While { .. }) {
            if try_scev_upgrade(stmts, i) {
                changed = true;
                // Don't advance i — re-check this position (the For may enable
                // another pass, but more likely just continue).
                continue;
            }
        } else if matches!(&stmts[i], HirStmt::DoWhile { .. })
            && try_guarded_dowhile_pointer_iv_upgrade(stmts, locals, i)
        {
            changed = true;
            continue;
        }
        // Recurse into nested constructs.
        match &mut stmts[i] {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_scev_upgrade_in_stmts(then_body, locals);
                changed |= apply_scev_upgrade_in_stmts(else_body, locals);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= apply_scev_upgrade_in_stmts(body, locals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_scev_upgrade_in_stmts(&mut case.body, locals);
                }
                changed |= apply_scev_upgrade_in_stmts(default, locals);
            }
            _ => {}
        }
        i += 1;
    }
    changed
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Apply IV-to-For upgrade (SCEV-lite) across the entire function body.
/// Returns `true` if any transformation was made.
pub(crate) fn apply_iv_recovery_pass(func: &mut HirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::new();
    count_goto_targets(&func.body, &mut goto_counts);
    apply_tail_label_loop_recovery_in_stmts(&mut func.body, &goto_counts)
        | apply_scev_upgrade_in_stmts(&mut func.body, &mut func.locals)
}

/// Apply break/continue recovery across the entire function body.
/// Returns `true` if any transformation was made.
pub(crate) fn apply_break_continue_pass(func: &mut HirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::new();
    count_goto_targets(&func.body, &mut goto_counts);
    apply_break_continue_in_stmts(&mut func.body, &goto_counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn ptr_u32() -> NirType {
        NirType::Ptr(Box::new(int(32, false)))
    }

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    fn const_i(value: i64) -> HirExpr {
        HirExpr::Const(value, int(64, true))
    }

    fn add(lhs: HirExpr, rhs: HirExpr, ty: NirType) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        }
    }

    fn ne(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn guarded_pointer_dowhile_upgrades_to_for() {
        let mut func = HirFunction {
            name: "guarded_pointer_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLe,
                        lhs: Box::new(var("len")),
                        rhs: Box::new(const_i(0)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Goto("exit".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                HirStmt::DoWhile {
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("value".to_string()),
                            rhs: HirExpr::Load {
                                ptr: Box::new(var("ptr")),
                                ty: int(32, false),
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("ptr".to_string()),
                            rhs: add(var("ptr"), const_i(1), ptr_u32()),
                        },
                    ],
                    cond: ne(var("ptr"), var("end")),
                },
                HirStmt::Label("exit".to_string()),
                HirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let HirStmt::For {
            init,
            cond,
            update,
            body,
        } = &func.body[2]
        else {
            panic!("expected guarded do-while to become for");
        };
        assert!(init.is_some());
        assert!(matches!(
            cond,
            Some(HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs,
                rhs,
                ..
            }) if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "iVar0")
                && matches!(rhs.as_ref(), HirExpr::Var(name) if name == "len")
        ));
        assert!(update.is_some());
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs:
                    HirExpr::Index {
                        base,
                        index,
                        elem_ty
                    },
            } if name == "value"
                && matches!(base.as_ref(), HirExpr::Var(name) if name == "ptr")
                && matches!(index.as_ref(), HirExpr::Var(name) if name == "iVar0")
                && *elem_ty == int(32, false)
        ));
        assert!(func.locals.iter().any(|local| local.name == "iVar0"
            && local.ty
                == (NirType::Int {
                    bits: 64,
                    signed: true,
                })));
    }

    #[test]
    fn unguarded_pointer_dowhile_stays_dowhile() {
        let mut func = HirFunction {
            name: "unguarded_pointer_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                HirStmt::DoWhile {
                    body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("ptr".to_string()),
                        rhs: add(var("ptr"), const_i(1), ptr_u32()),
                    }],
                    cond: ne(var("ptr"), var("end")),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[1], HirStmt::DoWhile { .. }));
    }

    #[test]
    fn early_return_guarded_pointer_dowhile_upgrades_to_indexed_for() {
        let mut func = HirFunction {
            name: "early_return_guarded_pointer_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: int(32, false),
            surface_return_type_name: None,
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLe,
                        lhs: Box::new(var("len")),
                        rhs: Box::new(const_i(0)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32, false))))],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                HirStmt::DoWhile {
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("sum".to_string()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Add,
                                lhs: Box::new(var("sum")),
                                rhs: Box::new(HirExpr::Load {
                                    ptr: Box::new(var("ptr")),
                                    ty: int(32, false),
                                }),
                                ty: int(32, false),
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("ptr".to_string()),
                            rhs: add(var("ptr"), const_i(1), ptr_u32()),
                        },
                    ],
                    cond: ne(var("ptr"), var("end")),
                },
                HirStmt::Return(Some(var("sum"))),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let HirStmt::For { body, cond, .. } = &func.body[2] else {
            panic!("expected early-return guarded do-while to become for");
        };
        assert!(matches!(
            cond,
            Some(HirExpr::Binary {
                op: HirBinaryOp::SLt,
                lhs,
                rhs,
                ..
            }) if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "iVar0")
                && matches!(rhs.as_ref(), HirExpr::Var(name) if name == "len")
        ));
        assert!(matches!(
            &body[0],
            HirStmt::Assign {
                rhs:
                    HirExpr::Binary {
                        rhs,
                        ..
                    },
                ..
            } if matches!(
                rhs.as_ref(),
                HirExpr::Index { base, index, .. }
                    if matches!(base.as_ref(), HirExpr::Var(name) if name == "ptr")
                        && matches!(index.as_ref(), HirExpr::Var(name) if name == "iVar0")
            )
        ));
    }

    #[test]
    fn tail_label_counted_loop_becomes_break_guarded_for() {
        let mut func = HirFunction {
            name: "tail_label_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                HirStmt::Label("head".to_string()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: add(var("sum"), var("i"), int(64, true)),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                HirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![HirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], HirStmt::Assign { .. }));
        let HirStmt::For {
            init,
            cond,
            update,
            body,
        } = &func.body[1]
        else {
            panic!("expected tail label loop to become for");
        };
        assert!(init.is_none());
        assert!(cond.is_none());
        assert!(update.is_none());
        assert_eq!(body.len(), 3);
        assert!(matches!(
            body.last(),
            Some(HirStmt::If {
                cond:
                    HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs,
                        rhs,
                        ..
                    },
                then_body,
                else_body,
            }) if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "i")
                && matches!(rhs.as_ref(), HirExpr::Var(name) if name == "n")
                && matches!(then_body.as_slice(), [HirStmt::Break])
                && else_body.is_empty()
        ));
        assert!(matches!(func.body[2], HirStmt::Return(None)));
        assert!(
            !func
                .body
                .iter()
                .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == "head"))
        );
    }

    #[test]
    fn tail_label_loop_allows_body_local_goto() {
        let mut func = HirFunction {
            name: "tail_label_loop_with_local_goto".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Label("head".to_string()),
                HirStmt::If {
                    cond: var("flag"),
                    then_body: vec![HirStmt::Goto("inside".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: add(var("sum"), const_i(1), int(64, true)),
                },
                HirStmt::Label("inside".to_string()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                HirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![HirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let HirStmt::For { body, .. } = &func.body[0] else {
            panic!("expected local-goto tail loop to become for");
        };
        assert!(body
            .iter()
            .any(|stmt| matches!(stmt, HirStmt::Label(label) if label == "inside")));
    }

    #[test]
    fn tail_label_loop_rejects_nonlocal_body_goto() {
        let mut func = HirFunction {
            name: "tail_label_loop_with_external_goto".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Label("head".to_string()),
                HirStmt::If {
                    cond: var("flag"),
                    then_body: vec![HirStmt::Goto("exit".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                HirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![HirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Label("exit".to_string()),
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], HirStmt::Label(_)));
    }

    #[test]
    fn tail_label_loop_rejects_multiple_backedges_to_head() {
        let mut func = HirFunction {
            name: "tail_label_loop_with_multiple_backedges".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Label("head".to_string()),
                HirStmt::If {
                    cond: var("retry"),
                    then_body: vec![HirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                HirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![HirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], HirStmt::Label(_)));
    }

    #[test]
    fn for_loop_dataflow_simple() {
        // init: i = 0
        // while (i < n) {
        //   t = i + 1;
        //   i = t;
        // }
        // The dataflow walk should trace i through t back to i + 1, verifying that i is loop-carried
        // and has a valid linear iteration update!
        let mut func = HirFunction {
            name: "dataflow_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                HirStmt::While {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("t".to_string()),
                            rhs: add(var("i"), const_i(1), int(64, true)),
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("i".to_string()),
                            rhs: var("t"),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let HirStmt::For { init, cond, update, body } = &func.body[0] else {
            panic!("Expected loop to become a For loop!");
        };
        assert!(init.is_some());
        assert!(cond.is_some());
        assert!(update.is_some());
        // The body should have had the update statement (i = t) removed, and only contain t = i + 1.
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            HirStmt::Assign {
                lhs: HirLValue::Var(lhs),
                ..
            } if lhs == "t"
        ));
    }

    #[test]
    fn for_loop_dataflow_invalid_no_loop_var() {
        // init: i = 0
        // while (i < n) {
        //   t = 42;
        //   i = t;
        // }
        // This is not a linear/affine recurrence of i, so it should fail to upgrade!
        let mut func = HirFunction {
            name: "invalid_dataflow_loop".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                HirStmt::While {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("t".to_string()),
                            rhs: const_i(42),
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("i".to_string()),
                            rhs: var("t"),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
    }
}
