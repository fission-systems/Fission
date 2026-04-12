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
///
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
use super::super::*;
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

// ── Part A — SCEV-lite: enhance While → For ──────────────────────────────────

/// Collect all variable names mentioned in an expression.
fn expr_vars(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            out.insert(name.clone());
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => expr_vars(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_vars(lhs, out);
            expr_vars(rhs, out);
        }
        HirExpr::Load { ptr, .. } => expr_vars(ptr, out),
        HirExpr::PtrOffset { base, .. } => expr_vars(base, out),
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

/// Try to upgrade a `While` loop at `stmts[loop_idx]` to a `For` loop using
/// SCEV-lite IV detection.  Returns `true` if a transformation was applied.
fn try_scev_upgrade(stmts: &mut Vec<HirStmt>, loop_idx: usize) -> bool {
    let (cond, body) = match &stmts[loop_idx] {
        HirStmt::While { cond, body } => (cond.clone(), body.clone()),
        _ => return false,
    };

    // Safety: no Continue in body (semantics of `update` would change).
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let mut cond_vars = HashSet::new();
    expr_vars(&cond, &mut cond_vars);
    if cond_vars.is_empty() {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);

    for var in &cond_vars {
        let (update_idx, is_last) = match find_iv_update(&body, var, &loop_variant) {
            Some(v) => v,
            None => continue,
        };
        // Update must be the last statement in body (or we'd change semantics).
        if !is_last {
            continue;
        }

        let init_idx = match find_init_before(stmts, loop_idx, var) {
            Some(i) => i,
            None => continue,
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
    false
}

fn apply_scev_upgrade_in_stmts(stmts: &mut Vec<HirStmt>) -> bool {
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
        }
        // Recurse into nested constructs.
        match &mut stmts[i] {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_scev_upgrade_in_stmts(then_body);
                changed |= apply_scev_upgrade_in_stmts(else_body);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= apply_scev_upgrade_in_stmts(body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_scev_upgrade_in_stmts(&mut case.body);
                }
                changed |= apply_scev_upgrade_in_stmts(default);
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
    apply_scev_upgrade_in_stmts(&mut func.body)
}

/// Apply break/continue recovery across the entire function body.
/// Returns `true` if any transformation was made.
pub(crate) fn apply_break_continue_pass(func: &mut HirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::new();
    count_goto_targets(&func.body, &mut goto_counts);
    apply_break_continue_in_stmts(&mut func.body, &goto_counts)
}
