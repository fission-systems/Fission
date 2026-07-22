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
use crate::prelude::*;
use fission_midend_dir::util::expr_type;
use crate::{HashMap, HashSet};

// ── Part B — Break/Continue recovery ─────────────────────────────────────────

/// Count occurrences of each label name as a Goto target in a statement list.
fn count_goto_targets(stmts: &[DirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_goto_targets_stmt(stmt, counts);
    }
}

fn count_goto_targets_stmt(stmt: &DirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        DirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        DirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            count_goto_targets(then_body, counts);
            count_goto_targets(else_body, counts);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            count_goto_targets(body, counts);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_goto_targets(&case.body, counts);
            }
            count_goto_targets(default, counts);
        }
        _ => {}
    }
}

/// Collect all label *definitions* in a statement list.
fn collect_labels(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_labels_stmt(stmt, out);
    }
}

fn collect_labels_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Label(label) => {
            out.insert(label.clone());
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_labels(then_body, out);
            collect_labels(else_body, out);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            collect_labels(body, out);
        }
        DirStmt::Switch { cases, default, .. } => {
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
    body: &mut Vec<DirStmt>,
    after_labels: &HashSet<String>,
    head_labels: &HashSet<String>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    // Recurse into nested loops first (inner loops are handled before outer).
    for stmt in body.iter_mut() {
        match stmt {
            DirStmt::While { body: inner, .. }
            | DirStmt::DoWhile { body: inner, .. }
            | DirStmt::For { body: inner, .. } => {
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
        let do_break = if let DirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[i]
        {
            if else_body.is_empty() {
                if let [DirStmt::Goto(lbl)] = then_body.as_slice() {
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
            if let DirStmt::If {
                then_body,
                else_body,
                ..
            } = &body[i]
            {
                if else_body.is_empty() {
                    if let [DirStmt::Goto(lbl)] = then_body.as_slice() {
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
            if let DirStmt::If { then_body, .. } = &mut body[i] {
                then_body.clear();
                then_body.push(DirStmt::Break);
                changed = true;
            }
        } else if do_continue {
            if let DirStmt::If { then_body, .. } = &mut body[i] {
                then_body.clear();
                then_body.push(DirStmt::Continue);
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
    stmts: &mut Vec<DirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let n = stmts.len();
    for loop_idx in 0..n {
        // Determine labels that appear after this loop (exit targets).
        let mut after_labels = HashSet::default();
        for stmt in stmts.iter().skip(loop_idx + 1) {
            collect_labels_stmt(stmt, &mut after_labels);
        }
        // Head labels: labels immediately before the loop.
        let mut head_labels = HashSet::default();
        if loop_idx > 0 {
            collect_labels_stmt(&stmts[loop_idx - 1], &mut head_labels);
        }

        let is_loop = matches!(
            &stmts[loop_idx],
            DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. }
        );
        if !is_loop {
            continue;
        }

        let body = match &mut stmts[loop_idx] {
            DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => body,
            _ => unreachable!(),
        };
        changed |= recover_break_continue_in_body(body, &after_labels, &head_labels, goto_counts);
    }

    // Recurse into If/Block/Switch to catch loops nested there.
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_break_continue_in_stmts(then_body, goto_counts);
                changed |= apply_break_continue_in_stmts(else_body, goto_counts);
            }
            DirStmt::Block(body) => {
                changed |= apply_break_continue_in_stmts(body, goto_counts);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_break_continue_in_stmts(&mut case.body, goto_counts);
                }
                changed |= apply_break_continue_in_stmts(default, goto_counts);
            }
            DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                // Recurse for nested loops.
                changed |= apply_break_continue_in_stmts(body, goto_counts);
            }
            _ => {}
        }
    }

    changed
}

// ── Part C — Tail label loops → break-guarded `for (;;)` ─────────────────────

fn invert_condition(expr: DirExpr) -> DirExpr {
    match expr {
        DirExpr::Binary { op, lhs, rhs, ty } => {
            let inverted = match op {
                DirBinaryOp::Eq => Some(DirBinaryOp::Ne),
                DirBinaryOp::Ne => Some(DirBinaryOp::Eq),
                DirBinaryOp::Lt => Some(DirBinaryOp::Ge),
                DirBinaryOp::Le => Some(DirBinaryOp::Gt),
                DirBinaryOp::Gt => Some(DirBinaryOp::Le),
                DirBinaryOp::Ge => Some(DirBinaryOp::Lt),
                DirBinaryOp::SLt => Some(DirBinaryOp::SGe),
                DirBinaryOp::SLe => Some(DirBinaryOp::SGt),
                DirBinaryOp::SGt => Some(DirBinaryOp::SLe),
                DirBinaryOp::SGe => Some(DirBinaryOp::SLt),
                _ => None,
            };
            if let Some(op) = inverted {
                DirExpr::Binary { op, lhs, rhs, ty }
            } else {
                DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Binary { op, lhs, rhs, ty }),
                    ty: NirType::Bool,
                }
            }
        }
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

fn tail_goto_condition(stmt: &DirStmt, label: &str) -> Option<DirExpr> {
    let DirStmt::If {
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
    matches!(then_body.as_slice(), [DirStmt::Goto(target)] if target == label).then(|| cond.clone())
}

fn collect_loop_body_labels(stmts: &[DirStmt]) -> HashSet<String> {
    let mut labels = HashSet::default();
    collect_labels(stmts, &mut labels);
    labels
}

fn collect_goto_targets_in_stmts(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_goto_targets_in_stmt(stmt, out);
    }
}

fn collect_goto_targets_in_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Goto(label) => {
            out.insert(label.clone());
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_goto_targets_in_stmts(then_body, out);
            collect_goto_targets_in_stmts(else_body, out);
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => collect_goto_targets_in_stmts(body, out),
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_goto_targets_in_stmts(&case.body, out);
            }
            collect_goto_targets_in_stmts(default, out);
        }
        _ => {}
    }
}

fn has_unscoped_break(stmts: &[DirStmt]) -> bool {
    stmts.iter().any(has_unscoped_break_stmt)
}

fn has_unscoped_break_stmt(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Break => true,
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => has_unscoped_break(then_body) || has_unscoped_break(else_body),
        DirStmt::Block(body) => has_unscoped_break(body),
        DirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| has_unscoped_break(&case.body)) || has_unscoped_break(default)
        }
        DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. } => false,
        _ => false,
    }
}

fn body_gotos_are_loop_local(body: &[DirStmt]) -> bool {
    let labels = collect_loop_body_labels(body);
    let mut gotos = HashSet::default();
    collect_goto_targets_in_stmts(body, &mut gotos);
    gotos.iter().all(|target| labels.contains(target))
}

fn try_tail_label_loop_to_for(
    stmts: &mut Vec<DirStmt>,
    label_idx: usize,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let DirStmt::Label(label) = &stmts[label_idx] else {
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

        // Side-entry check: Reject loop recovery if there are jumps from outside into the body.
        let body_labels = collect_loop_body_labels(body_slice);
        let mut internal_goto_counts = HashMap::default();
        count_goto_targets(body_slice, &mut internal_goto_counts);
        for label_in_body in &body_labels {
            let global_gotos = goto_counts.get(label_in_body).copied().unwrap_or(0);
            let internal_gotos = internal_goto_counts
                .get(label_in_body)
                .copied()
                .unwrap_or(0);
            if global_gotos > internal_gotos {
                return false;
            }
        }

        let mut body = body_slice.to_vec();
        body.push(DirStmt::If {
            cond: invert_condition(continue_cond),
            then_body: vec![DirStmt::Break],
            else_body: Vec::new(),
        });
        let replacement = DirStmt::For {
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

/// MSVC `/O` row-fill inner loops sometimes peel vectorized prolog/epilog into
/// label/goto pointer walks with `(end - start) & align` guards.  When the outer
/// latch is already an infinite `for` with a break tail, recover:
///   `for (j = 0; j < cols; j++) base[row_offset + j] = value`.
fn try_recover_row_stride_fill_inner_loop(
    stmts: &mut Vec<DirStmt>,
    loop_idx: usize,
    locals: &mut Vec<DirBinding>,
) -> bool {
    let body = match &stmts[loop_idx] {
        DirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        } => body.clone(),
        _ => return false,
    };
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }
    let tail_start = match find_outer_infinite_for_tail_start(&body) {
        Some(i) => i,
        None => return false,
    };
    let loop_variant = loop_variant_vars(&body);
    let matched = match try_parse_row_stride_fill(&body, tail_start, &loop_variant) {
        Some(m) => m,
        None => return false,
    };

    let j_name = fresh_index_name(locals, stmts);
    let j_ty = index_type_for_count(&matched.stride);
    locals.push(DirBinding {
        name: j_name.clone(),
        ty: j_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });

    let index_expr = DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(matched.row_offset),
        rhs: Box::new(DirExpr::Var(j_name.clone())),
        ty: j_ty.clone(),
    };
    let store_stmt = DirStmt::Assign {
        lhs: DirLValue::Index {
            base: Box::new(matched.base),
            index: Box::new(index_expr),
            elem_ty: matched.elem_ty.clone(),
        },
        rhs: matched.fill_value,
    };
    let inner_for = DirStmt::For {
        init: Some(Box::new(DirStmt::Assign {
            lhs: DirLValue::Var(j_name.clone()),
            rhs: DirExpr::Const(0, j_ty.clone()),
        })),
        cond: Some(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(DirExpr::Var(j_name.clone())),
            rhs: Box::new(matched.stride),
            ty: NirType::Bool,
        }),
        update: Some(Box::new(DirStmt::Assign {
            lhs: DirLValue::Var(j_name.clone()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var(j_name.clone())),
                rhs: Box::new(DirExpr::Const(1, j_ty.clone())),
                ty: j_ty,
            },
        })),
        body: vec![store_stmt],
    };

    let mut new_body = vec![inner_for];
    new_body.extend_from_slice(&body[matched.tail_start..]);
    stmts[loop_idx] = DirStmt::For {
        init: None,
        cond: None,
        update: None,
        body: new_body,
    };
    true
}

struct RowStrideFillMatch {
    base: DirExpr,
    row_offset: DirExpr,
    stride: DirExpr,
    fill_value: DirExpr,
    elem_ty: NirType,
    tail_start: usize,
}

fn find_outer_infinite_for_tail_start(body: &[DirStmt]) -> Option<usize> {
    let tail = tail_meaningful_stmts(body, 3)?;
    parse_break_eq_var(&tail[2].1)?;
    Some(tail[0].0)
}

fn parse_ptr_base_offset(expr: &DirExpr) -> Option<(DirExpr, DirExpr)> {
    match strip_casts(expr) {
        DirExpr::PtrOffset { base, offset } => Some((
            (*base).as_ref().clone(),
            DirExpr::Const(
                *offset,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        )),
        DirExpr::Index { base, index, .. } => {
            Some(((*base).as_ref().clone(), (*index).as_ref().clone()))
        }
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => Some(((*lhs).as_ref().clone(), (*rhs).as_ref().clone())),
        _ => None,
    }
}

fn assign_rhs_ptr_base_offset(stmt: &DirStmt) -> Option<(String, DirExpr, DirExpr)> {
    let DirStmt::Assign {
        lhs: DirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let (base, offset) = parse_ptr_base_offset(rhs)?;
    Some((name.clone(), base, offset))
}

fn is_peel_corruption_pair(sub_stmt: &DirStmt, mask_stmt: &DirStmt) -> bool {
    let Some((_, _sub_rhs)) = parse_self_sub_assign(sub_stmt) else {
        return false;
    };
    let DirStmt::Assign {
        lhs: DirLValue::Var(_),
        rhs,
    } = mask_stmt
    else {
        return false;
    };
    matches!(
        strip_casts(rhs),
        DirExpr::Binary {
            op: DirBinaryOp::And,
            rhs: mask,
            ..
        } if matches!(strip_casts(mask.as_ref()), DirExpr::Const(v, _) if (1..=16).contains(v))
    )
}

fn parse_self_sub_assign(stmt: &DirStmt) -> Option<(String, DirExpr)> {
    let DirStmt::Assign {
        lhs: DirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs,
        rhs: subtrahend,
        ..
    } = strip_casts(rhs)
    else {
        return None;
    };
    if matches!(strip_casts(lhs.as_ref()), DirExpr::Var(var) if var == name) {
        Some((name.clone(), (*subtrahend).as_ref().clone()))
    } else {
        None
    }
}

fn deref_targets_var(ptr: &DirExpr, cursor: &str) -> bool {
    matches!(strip_casts(ptr), DirExpr::Var(name) if name == cursor)
}

fn find_fill_value_in_region(stmts: &[DirStmt], cursor: &str) -> Option<DirExpr> {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Deref { ptr, .. },
                rhs,
            } if deref_targets_var(ptr, cursor) => return Some(rhs.clone()),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if let Some(v) = find_fill_value_in_region(then_body, cursor) {
                    return Some(v);
                }
                if let Some(v) = find_fill_value_in_region(else_body, cursor) {
                    return Some(v);
                }
            }
            DirStmt::Block(body) => {
                if let Some(v) = find_fill_value_in_region(body, cursor) {
                    return Some(v);
                }
            }
            _ => {}
        }
    }
    None
}

fn inner_region_has_cursor_stores(stmts: &[DirStmt], cursor: &str) -> bool {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Deref { ptr, .. },
                ..
            } if deref_targets_var(ptr, cursor) => return true,
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if inner_region_has_cursor_stores(then_body, cursor)
                    || inner_region_has_cursor_stores(else_body, cursor)
                {
                    return true;
                }
            }
            DirStmt::Block(body) => {
                if inner_region_has_cursor_stores(body, cursor) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn try_parse_row_stride_fill(
    body: &[DirStmt],
    tail_start: usize,
    loop_variant: &HashSet<String>,
) -> Option<RowStrideFillMatch> {
    let mut meaningful = Vec::new();
    for (idx, stmt) in body.iter().enumerate().take(tail_start) {
        if !matches!(stmt, DirStmt::Label(_)) {
            meaningful.push((idx, stmt));
        }
    }
    if meaningful.len() < 4 {
        return None;
    }

    let mut idx = 0;
    let mut row_offset: Option<DirExpr> = None;
    let mut base: Option<DirExpr> = None;
    let mut cursor: Option<String> = None;
    let mut stride: Option<DirExpr> = None;
    let mut prefix_end = 0;

    if let DirStmt::Assign {
        lhs: DirLValue::Var(_),
        rhs,
    } = meaningful[idx].1
    {
        if let Some(var) = stripped_var_name(strip_casts(rhs)) {
            row_offset = Some(DirExpr::Var(var.to_string()));
            prefix_end = meaningful[idx].0 + 1;
            idx += 1;
        } else if let DirExpr::Cast { expr, .. } = strip_casts(rhs) {
            if let Some(var) = stripped_var_name(expr) {
                row_offset = Some(DirExpr::Var(var.to_string()));
                prefix_end = meaningful[idx].0 + 1;
                idx += 1;
            }
        }
    }

    if idx >= meaningful.len() {
        return None;
    }
    let (cursor_name, base_expr, off_expr) = assign_rhs_ptr_base_offset(meaningful[idx].1)?;
    base = Some(base_expr);
    cursor = Some(cursor_name);
    if row_offset.is_none() {
        row_offset = Some(off_expr);
    }
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx >= meaningful.len() {
        return None;
    }
    let (_, addend) = parse_self_add_assign(meaningful[idx].1)?;
    if !is_loop_invariant(&addend, loop_variant) {
        return None;
    }
    stride = Some(addend);
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx >= meaningful.len() {
        return None;
    }
    let (_, base2, _) = assign_rhs_ptr_base_offset(meaningful[idx].1)?;
    if !vars_equivalent_after_casts(base.as_ref()?, &base2) {
        return None;
    }
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx + 1 < meaningful.len()
        && is_peel_corruption_pair(meaningful[idx].1, meaningful[idx + 1].1)
    {
        prefix_end = meaningful[idx + 1].0 + 1;
    }

    let inner_region = &body[prefix_end..tail_start];
    let fill_value = find_fill_value_in_region(inner_region, cursor.as_ref()?)?;
    if !inner_region_has_cursor_stores(inner_region, cursor.as_ref()?) {
        return None;
    }

    Some(RowStrideFillMatch {
        base: base?,
        row_offset: row_offset?,
        stride: stride?,
        fill_value,
        elem_ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        tail_start,
    })
}

/// MSVC-style nested-loop outer latch sometimes merges two induction variables
/// (row counter `+= 1` and row offset `+= cols`) onto one temp after copy chains.
/// Pattern at infinite-`for` tail:
///   `v = v + 1; v = v + K; if (limit == v) break;`
/// Recover by splitting the `+1` update and exit compare onto a fresh row counter.
fn try_split_merged_dual_iv_tail(
    stmts: &mut Vec<DirStmt>,
    loop_idx: usize,
    locals: &mut Vec<DirBinding>,
) -> bool {
    let body = match &stmts[loop_idx] {
        DirStmt::For {
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
    if !matches!(strip_casts(&add_one), DirExpr::Const(1, _)) {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);
    if !is_loop_invariant(&add_k, &loop_variant) {
        return false;
    }
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let row_name = fresh_index_name(locals, stmts);
    let row_ty = index_type_for_count(&limit_expr);
    locals.push(DirBinding {
        name: row_name.clone(),
        ty: row_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });

    let mut new_body = body;
    substitute_var_in_stmt(&mut new_body[tail[0].0], &var_one, &row_name, false);
    substitute_var_in_stmt(&mut new_body[tail[2].0], &var_one, &row_name, false);

    stmts[loop_idx] = DirStmt::For {
        init: None,
        cond: None,
        update: None,
        body: new_body,
    };

    let init_stmt = DirStmt::Assign {
        lhs: DirLValue::Var(row_name),
        rhs: DirExpr::Const(0, row_ty),
    };
    stmts.insert(loop_idx, init_stmt);
    true
}

fn tail_meaningful_stmts(body: &[DirStmt], count: usize) -> Option<Vec<(usize, DirStmt)>> {
    let mut tail = Vec::new();
    for (idx, stmt) in body.iter().enumerate().rev() {
        if matches!(stmt, DirStmt::Label(_)) {
            continue;
        }
        tail.push((idx, stmt.clone()));
        if tail.len() == count {
            tail.reverse();
            return Some(tail);
        }
    }
    None
}

fn parse_self_add_assign(stmt: &DirStmt) -> Option<(String, DirExpr)> {
    let DirStmt::Assign {
        lhs: DirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs,
        rhs: addend,
        ..
    } = strip_casts(rhs)
    else {
        return None;
    };
    if matches!(strip_casts(lhs.as_ref()), DirExpr::Var(var) if var == name) {
        Some((name.clone(), (*addend).as_ref().clone()))
    } else if matches!(strip_casts(addend.as_ref()), DirExpr::Var(var) if var == name) {
        Some((name.clone(), (*lhs).as_ref().clone()))
    } else {
        None
    }
}

fn parse_break_eq_var(stmt: &DirStmt) -> Option<(DirExpr, String)> {
    let DirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() || !matches!(then_body.as_slice(), [DirStmt::Break]) {
        return None;
    }
    let DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = strip_casts(cond)
    else {
        return None;
    };
    if let DirExpr::Var(var) = strip_casts(rhs.as_ref()) {
        Some(((*lhs).as_ref().clone(), var.clone()))
    } else if let DirExpr::Var(var) = strip_casts(lhs.as_ref()) {
        Some(((*rhs).as_ref().clone(), var.clone()))
    } else {
        None
    }
}

/// Replace `var` with `replacement` in `stmt`. When `assign_lhs_only`, only rewrite
/// the assignment target (for splitting a shared temp across dual IV updates).
fn substitute_var_in_stmt(stmt: &mut DirStmt, var: &str, replacement: &str, assign_lhs_only: bool) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            if let DirLValue::Var(name) = lhs {
                if name == var {
                    *name = replacement.to_string();
                }
            }
            if !assign_lhs_only {
                substitute_var_in_expr(rhs, var, &DirExpr::Var(replacement.to_string()));
            }
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            substitute_var_in_expr(cond, var, &DirExpr::Var(replacement.to_string()));
            for s in then_body.iter_mut().chain(else_body.iter_mut()) {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for s in body.iter_mut() {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                for s in case.body.iter_mut() {
                    substitute_var_in_stmt(s, var, replacement, false);
                }
            }
            for s in default.iter_mut() {
                substitute_var_in_stmt(s, var, replacement, false);
            }
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            substitute_var_in_expr(expr, var, &DirExpr::Var(replacement.to_string()));
        }
        _ => {}
    }
}

fn apply_tail_label_loop_recovery_in_stmts(
    stmts: &mut Vec<DirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], DirStmt::Label(_))
            && try_tail_label_loop_to_for(stmts, i, goto_counts)
        {
            changed = true;
            continue;
        }

        match &mut stmts[i] {
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(then_body, goto_counts);
                changed |= apply_tail_label_loop_recovery_in_stmts(else_body, goto_counts);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(body, goto_counts);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_tail_label_loop_recovery_in_stmts(&mut case.body, goto_counts);
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
fn expr_vars(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            out.insert(name.clone());
        }
        DirExpr::Cast { expr, .. } | DirExpr::Unary { expr, .. } => expr_vars(expr, out),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_vars(lhs, out);
            expr_vars(rhs, out);
        }
        DirExpr::Load { ptr, .. } => expr_vars(ptr, out),
        DirExpr::PtrOffset { base, .. } | DirExpr::FieldAccess { base, .. } => expr_vars(base, out),
        DirExpr::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        DirExpr::Call { args, .. } => {
            for a in args {
                expr_vars(a, out);
            }
        }
        DirExpr::AggregateCopy { src, .. } => expr_vars(src, out),
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_vars(cond, out);
            expr_vars(then_expr, out);
            expr_vars(else_expr, out);
        }
        DirExpr::Const(_, _) => {}
    }
}

/// Check that an expression only contains constants or variables NOT in
/// `loop_variant` — i.e., the expression is loop-invariant.
fn is_loop_invariant(expr: &DirExpr, loop_variant: &HashSet<String>) -> bool {
    let mut vars = HashSet::default();
    expr_vars(expr, &mut vars);
    vars.is_disjoint(loop_variant)
}

/// Detect a single `v = v ± k` or `v = k ± v` update for variable `v` in
/// the loop body.  Returns the update statement index and whether it is the
/// LAST statement.
fn find_iv_update(
    body: &[DirStmt],
    var: &str,
    loop_variant: &HashSet<String>,
) -> Option<(usize, bool)> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
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
fn is_iv_update(expr: &DirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    is_linear_update_of(expr, var, loop_variant)
        || is_affine_mul_add_update(expr, var, loop_variant)
}

/// Like `is_iv_update`, but resolves through intermediate variable definitions
/// in the loop body.  For example, given `i = t` where `t = i + 1`, the direct
/// check fails (the RHS is just `Var("t")`), but this function resolves `t`
/// through its unique definition and finds the `i + 1` recurrence.
fn is_iv_update_dataflow(
    expr: &DirExpr,
    var: &str,
    loop_variant: &HashSet<String>,
    body: &[DirStmt],
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
        DirExpr::Var(name) if name != var => {
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
fn is_linear_update_of(expr: &DirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        DirExpr::Binary { op, lhs, rhs, .. }
            if matches!(op, DirBinaryOp::Add | DirBinaryOp::Sub) =>
        {
            let lhs_is_var = matches!(lhs.as_ref(), DirExpr::Var(n) if n == var);
            let rhs_is_var = matches!(rhs.as_ref(), DirExpr::Var(n) if n == var);
            if lhs_is_var && is_loop_invariant(rhs, loop_variant) {
                return true;
            }
            if rhs_is_var && is_loop_invariant(lhs, loop_variant) {
                return true;
            }
            false
        }
        // Allow a Cast wrapping a linear update (sign extension on IV).
        DirExpr::Cast { expr: inner, .. } => is_linear_update_of(inner, var, loop_variant),
        _ => false,
    }
}

/// `v = v * C + k` or `v = k + v * C` (and commutative mul operand order), with
/// `C` and `k` loop-invariant scalars.
fn is_affine_mul_add_update(expr: &DirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        DirExpr::Cast { expr: inner, .. } => is_affine_mul_add_update(inner, var, loop_variant),
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let mul_on_v = |m: &DirExpr| mul_var_times_invariant(m, var, loop_variant);
            let inv = |e: &DirExpr| is_loop_invariant_scalar(e, loop_variant);
            (mul_on_v(lhs) && inv(rhs)) || (mul_on_v(rhs) && inv(lhs))
        }
        _ => false,
    }
}

/// `v * e` or `e * v` where `e` has no loop-variant variables.
fn mul_var_times_invariant(expr: &DirExpr, var: &str, loop_variant: &HashSet<String>) -> bool {
    match expr {
        DirExpr::Cast { expr: inner, .. } => mul_var_times_invariant(inner, var, loop_variant),
        DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let lv = matches!(lhs.as_ref(), DirExpr::Var(n) if n == var);
            let rv = matches!(rhs.as_ref(), DirExpr::Var(n) if n == var);
            (lv && is_loop_invariant(rhs, loop_variant))
                || (rv && is_loop_invariant(lhs, loop_variant))
        }
        _ => false,
    }
}

/// Constants or expressions with no loop-variant variables (same as loop-invariant).
fn is_loop_invariant_scalar(expr: &DirExpr, loop_variant: &HashSet<String>) -> bool {
    match expr {
        DirExpr::Const(_, _) => true,
        DirExpr::Cast { expr: inner, .. } => is_loop_invariant_scalar(inner, loop_variant),
        _ => is_loop_invariant(expr, loop_variant),
    }
}

fn strip_casts(expr: &DirExpr) -> &DirExpr {
    match expr {
        DirExpr::Cast { expr, .. } => strip_casts(expr),
        _ => expr,
    }
}

fn stripped_var_name(expr: &DirExpr) -> Option<&str> {
    match strip_casts(expr) {
        DirExpr::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

fn vars_equivalent_after_casts(a: &DirExpr, b: &DirExpr) -> bool {
    matches!((stripped_var_name(a), stripped_var_name(b)), (Some(x), Some(y)) if x == y)
}

fn is_zero(expr: &DirExpr) -> bool {
    matches!(strip_casts(expr), DirExpr::Const(0, _))
}

/// Collect the set of variables modified inside the loop body (excluding
/// variables modified only in nested loops, which are their own scope).
fn loop_variant_vars(body: &[DirStmt]) -> HashSet<String> {
    let mut vars = HashSet::default();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    vars
}

fn loop_variant_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        DirStmt::If {
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
        DirStmt::Block(body) => {
            for s in body {
                loop_variant_stmt(s, out);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
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
        DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. } => {}
        _ => {}
    }
}

/// Scan `stmts[0..loop_idx]` backwards for a single assignment to `var` that
/// is not separated by a label, goto, or another modification of `var`.
fn find_init_before(stmts: &[DirStmt], loop_idx: usize, var: &str) -> Option<usize> {
    let mut scan = loop_idx;
    // scan backwards, limited to the immediately preceding statement
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                ..
            } if name == var => {
                return Some(scan);
            }
            // Any control flow or side-effecting statement stops the search.
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::If { .. }
            | DirStmt::While { .. }
            | DirStmt::DoWhile { .. }
            | DirStmt::For { .. }
            | DirStmt::Switch { .. }
            | DirStmt::Expr(_) => break,
            // Pure assignments to other variables are fine to skip.
            DirStmt::Assign { .. } => break,
            _ => {}
        }
    }
    None
}

fn find_pointer_end_assignment_before(
    stmts: &[DirStmt],
    loop_idx: usize,
    cursor: &str,
    end: &str,
) -> Option<(usize, DirExpr)> {
    let mut scan = loop_idx;
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if name == end => {
                let mut found_count_expr = None;
                if let DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs,
                    rhs: add_rhs,
                    ..
                } = strip_casts(rhs)
                {
                    if matches!(strip_casts(lhs), DirExpr::Var(name) if name == cursor) {
                        found_count_expr = Some(add_rhs.as_ref().clone());
                    } else if matches!(strip_casts(add_rhs), DirExpr::Var(name) if name == cursor) {
                        found_count_expr = Some(lhs.as_ref().clone());
                    }
                }

                if let Some(mut expr) = found_count_expr {
                    let mut backtrack = scan;
                    while backtrack > 0 {
                        backtrack -= 1;
                        if let DirStmt::Assign {
                            lhs: DirLValue::Var(v),
                            rhs: val,
                        } = &stmts[backtrack]
                        {
                            if count_var_uses(&expr, v) > 0 {
                                substitute_var_in_expr(&mut expr, v, val);
                            }
                        } else if matches!(
                            &stmts[backtrack],
                            DirStmt::Label(_)
                                | DirStmt::Goto(_)
                                | DirStmt::While { .. }
                                | DirStmt::DoWhile { .. }
                                | DirStmt::For { .. }
                                | DirStmt::Switch { .. }
                                | DirStmt::Block(_)
                                | DirStmt::Return(_)
                                | DirStmt::Break
                                | DirStmt::Continue
                        ) {
                            break;
                        }
                    }
                    return Some((scan, expr));
                }
                return None;
            }
            DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::While { .. }
            | DirStmt::DoWhile { .. }
            | DirStmt::For { .. }
            | DirStmt::Switch { .. }
            | DirStmt::Expr(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => break,
            DirStmt::If { .. } | DirStmt::Assign { .. } | DirStmt::VaStart { .. } => {}
            DirStmt::Block(_) => break,
        }
    }
    None
}

fn single_goto_target(stmts: &[DirStmt]) -> Option<&str> {
    match stmts {
        [DirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

fn labels_after(stmts: &[DirStmt], idx: usize) -> HashSet<String> {
    let mut labels = HashSet::default();
    for stmt in stmts.iter().skip(idx + 1) {
        collect_labels_stmt(stmt, &mut labels);
    }
    labels
}

fn positive_count_loop_cmp(cond: &DirExpr, count: &DirExpr) -> Option<DirBinaryOp> {
    let DirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    match op {
        DirBinaryOp::Le | DirBinaryOp::SLe => (vars_equivalent_after_casts(lhs, count)
            && is_zero(rhs))
        .then_some(if matches!(op, DirBinaryOp::SLe) {
            DirBinaryOp::SLt
        } else {
            DirBinaryOp::Lt
        }),
        DirBinaryOp::Ge | DirBinaryOp::SGe => (is_zero(lhs)
            && vars_equivalent_after_casts(rhs, count))
        .then_some(if matches!(op, DirBinaryOp::SGe) {
            DirBinaryOp::SLt
        } else {
            DirBinaryOp::Lt
        }),
        _ => None,
    }
}

fn positive_count_entry_guard_cmp(
    stmts: &[DirStmt],
    loop_idx: usize,
    count: &DirExpr,
    after_labels: &HashSet<String>,
) -> Option<DirBinaryOp> {
    stmts[..loop_idx].iter().find_map(|stmt| {
        let DirStmt::If {
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
            || matches!(then_body.as_slice(), [DirStmt::Return(Some(expr))] if is_zero(expr));
        if exits_before_loop {
            positive_count_loop_cmp(cond, count)
        } else {
            None
        }
    })
}

fn pointer_cursor_condition(cond: &DirExpr) -> Option<(&str, &str)> {
    let DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (DirExpr::Var(cursor), DirExpr::Var(end)) => Some((cursor.as_str(), end.as_str())),
        _ => None,
    }
}

fn cursor_used_after_loop(stmts: &[DirStmt], loop_idx: usize, cursor: &str) -> bool {
    stmts
        .iter()
        .skip(loop_idx + 1)
        .any(|stmt| count_var_uses_in_stmt(stmt, cursor) > 0)
}

fn count_var_uses_in_stmt(stmt: &DirStmt, name: &str) -> usize {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        DirStmt::VaStart { va_list, .. } | DirStmt::Expr(va_list) => count_var_uses(va_list, name),
        DirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        DirStmt::Block(body) | DirStmt::While { body, .. } => body
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        DirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        DirStmt::For {
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
        DirStmt::If {
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
        DirStmt::Switch {
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
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => 0,
    }
}

fn count_var_uses_in_lvalue(lhs: &DirLValue, name: &str) -> usize {
    match lhs {
        DirLValue::Var(_) => 0,
        DirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        DirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        DirLValue::FieldAccess { base, .. } => count_var_uses(base, name),
    }
}

fn count_var_uses(expr: &DirExpr, name: &str) -> usize {
    match expr {
        DirExpr::Var(var) | DirExpr::AddressOfGlobal(var) => usize::from(var == name),
        DirExpr::Const(_, _) => 0,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::PtrOffset { base: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => count_var_uses(expr, name),
        DirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        DirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        DirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        DirExpr::Select {
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

fn collect_names_in_lvalue(lhs: &DirLValue, out: &mut HashSet<String>) {
    match lhs {
        DirLValue::Var(name) => {
            out.insert(name.clone());
        }
        DirLValue::Deref { ptr, .. } => expr_vars(ptr, out),
        DirLValue::Index { base, index, .. } => {
            expr_vars(base, out);
            expr_vars(index, out);
        }
        DirLValue::FieldAccess { base, .. } => {
            expr_vars(base, out);
        }
    }
}

fn collect_names_in_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            collect_names_in_lvalue(lhs, out);
            expr_vars(rhs, out);
        }
        DirStmt::VaStart { va_list, .. } | DirStmt::Expr(va_list) => expr_vars(va_list, out),
        DirStmt::Return(Some(expr)) => expr_vars(expr, out),
        DirStmt::Block(body) | DirStmt::While { body, .. } => {
            collect_names_in_stmts(body, out);
        }
        DirStmt::DoWhile { body, cond } => {
            collect_names_in_stmts(body, out);
            expr_vars(cond, out);
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_vars(cond, out);
            collect_names_in_stmts(then_body, out);
            collect_names_in_stmts(else_body, out);
        }
        DirStmt::Switch {
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
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

fn collect_names_in_stmts(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_names_in_stmt(stmt, out);
    }
}

fn fresh_index_name(locals: &[DirBinding], stmts: &[DirStmt]) -> String {
    let mut used = HashSet::default();
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

fn index_type_for_count(count_expr: &DirExpr) -> NirType {
    match expr_type(count_expr) {
        NirType::Int { bits, signed } if bits >= 32 => NirType::Int { bits, signed },
        _ => NirType::Int {
            bits: 64,
            signed: true,
        },
    }
}

fn direct_cursor_var(expr: &DirExpr, cursor: &str) -> bool {
    matches!(strip_casts(expr), DirExpr::Var(name) if name == cursor)
}

fn index_var_expr(index_name: &str) -> DirExpr {
    DirExpr::Var(index_name.to_string())
}

fn cursor_index_expr(cursor: &str, index_name: &str, elem_ty: NirType) -> DirExpr {
    DirExpr::Index {
        base: Box::new(DirExpr::Var(cursor.to_string())),
        index: Box::new(index_var_expr(index_name)),
        elem_ty,
    }
}

fn is_one(expr: &DirExpr) -> bool {
    matches!(strip_casts(expr), DirExpr::Const(1, _))
}

fn type_size_bytes(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } | NirType::Float { bits } => (*bits / 8).max(1),
        NirType::Ptr(_) => 8,
        NirType::Aggregate { size, .. } => *size,
        NirType::Unknown => 1,
    }
}

fn is_const_val(expr: &DirExpr, val: i64) -> bool {
    matches!(strip_casts(expr), DirExpr::Const(v, _) if *v == val)
}

fn is_cursor_increment_by_one(stmt: &DirStmt, cursor: &str) -> bool {
    let DirStmt::Assign {
        lhs: DirLValue::Var(lhs),
        rhs,
    } = stmt
    else {
        return false;
    };
    if lhs != cursor {
        return false;
    }
    let DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = strip_casts(rhs)
    else {
        return false;
    };
    let element_size = match ty {
        NirType::Ptr(pointee) => type_size_bytes(pointee) as i64,
        _ => 1,
    };
    (direct_cursor_var(lhs, cursor) && (is_const_val(rhs, element_size) || is_const_val(rhs, 1)))
        || (direct_cursor_var(rhs, cursor)
            && (is_const_val(lhs, element_size) || is_const_val(lhs, 1)))
}

fn rewrite_cursor_expr_to_index(expr: &mut DirExpr, cursor: &str, index_name: &str) -> bool {
    match expr {
        DirExpr::Var(name) => name != cursor,
        DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => true,
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => {
            rewrite_cursor_expr_to_index(expr, cursor, index_name)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            rewrite_cursor_expr_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_expr_to_index(then_expr, cursor, index_name)
                && rewrite_cursor_expr_to_index(else_expr, cursor, index_name)
        }
        DirExpr::Call { args, .. } => args
            .iter_mut()
            .all(|arg| rewrite_cursor_expr_to_index(arg, cursor, index_name)),
        DirExpr::Load { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *expr = cursor_index_expr(cursor, index_name, ty.clone());
            true
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. } => {
            rewrite_cursor_expr_to_index(ptr, cursor, index_name)
        }
        DirExpr::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
    }
}

fn rewrite_cursor_lvalue_to_index(lhs: &mut DirLValue, cursor: &str, index_name: &str) -> bool {
    match lhs {
        DirLValue::Var(name) => name != cursor,
        DirLValue::Deref { ptr, ty } if direct_cursor_var(ptr, cursor) => {
            *lhs = DirLValue::Index {
                base: Box::new(DirExpr::Var(cursor.to_string())),
                index: Box::new(index_var_expr(index_name)),
                elem_ty: ty.clone(),
            };
            true
        }
        DirLValue::Deref { ptr, .. } => rewrite_cursor_expr_to_index(ptr, cursor, index_name),
        DirLValue::Index { base, index, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
                && rewrite_cursor_expr_to_index(index, cursor, index_name)
        }
        DirLValue::FieldAccess { base, .. } => {
            rewrite_cursor_expr_to_index(base, cursor, index_name)
        }
    }
}

fn rewrite_cursor_stmt_to_index(stmt: &mut DirStmt, cursor: &str, index_name: &str) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            rewrite_cursor_lvalue_to_index(lhs, cursor, index_name)
                && rewrite_cursor_expr_to_index(rhs, cursor, index_name)
        }
        DirStmt::VaStart { va_list, .. } | DirStmt::Expr(va_list) => {
            rewrite_cursor_expr_to_index(va_list, cursor, index_name)
        }
        DirStmt::Return(Some(expr)) => rewrite_cursor_expr_to_index(expr, cursor, index_name),
        DirStmt::Block(body) | DirStmt::While { body, .. } => {
            rewrite_cursor_body_to_index(body, cursor, index_name)
        }
        DirStmt::DoWhile { body, cond } => {
            rewrite_cursor_body_to_index(body, cursor, index_name)
                && rewrite_cursor_expr_to_index(cond, cursor, index_name)
        }
        DirStmt::For {
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
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_cursor_expr_to_index(cond, cursor, index_name)
                && rewrite_cursor_body_to_index(then_body, cursor, index_name)
                && rewrite_cursor_body_to_index(else_body, cursor, index_name)
        }
        DirStmt::Switch {
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
        DirStmt::Return(None)
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => true,
    }
}

fn rewrite_cursor_body_to_index(body: &mut [DirStmt], cursor: &str, index_name: &str) -> bool {
    body.iter_mut()
        .all(|stmt| rewrite_cursor_stmt_to_index(stmt, cursor, index_name))
}

/// Simple check to find the single top-level assignment to a variable in the loop body.
fn find_iv_update_simple(body: &[DirStmt], var: &str) -> Option<usize> {
    let mut found: Option<usize> = None;
    for (i, stmt) in body.iter().enumerate() {
        if let DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
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
fn find_unique_definition_in_body<'a>(body: &'a [DirStmt], var: &str) -> Option<&'a DirExpr> {
    let mut found: Option<&'a DirExpr> = None;
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

fn find_assignment_in_stmt<'a>(stmt: &'a DirStmt, var: &str) -> Option<&'a DirExpr> {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == var => Some(rhs),
        DirStmt::Block(body) => {
            let mut found: Option<&'a DirExpr> = None;
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
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let mut found: Option<&'a DirExpr> = None;
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
fn test_iterate_form(body: &[DirStmt], update_idx: usize, loop_var: &str) -> bool {
    let update_stmt = &body[update_idx];
    let DirStmt::Assign {
        lhs: DirLValue::Var(lhs_name),
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
    body: &[DirStmt],
    expr: &DirExpr,
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
fn find_loop_variable_dataflow(
    stmts: &[DirStmt],
    loop_idx: usize,
    body: &[DirStmt],
    cond: &DirExpr,
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
    stmts: &[DirStmt],
    loop_idx: usize,
    body: &[DirStmt],
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
        if let DirStmt::Assign { rhs, .. } = update_stmt {
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

/// Try to upgrade a `While` loop at `stmts[loop_idx]` to a `For` loop using
/// SCEV-lite IV detection.  Returns `true` if a transformation was applied.
fn try_scev_upgrade(stmts: &mut Vec<DirStmt>, loop_idx: usize) -> bool {
    let (is_for, init, cond, body) = match &stmts[loop_idx] {
        DirStmt::While { cond, body } => (false, None, cond.clone(), body.clone()),
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } if update.is_none() => (
            true,
            init.clone(),
            cond.as_ref().cloned().unwrap(),
            body.clone(),
        ),
        _ => return false,
    };

    // Safety: no Continue in body (semantics of `update` would change).
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);

    let (var, update_idx) =
        match find_loop_variable_dataflow(stmts, loop_idx, &body, &cond, &loop_variant) {
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

        stmts[loop_idx] = DirStmt::For {
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

        stmts[loop_idx] = DirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: new_body,
        };
        return true;
    }
}

fn extract_pointer_cursor_and_count(
    cond: &DirExpr,
    stmts: &[DirStmt],
    loop_idx: usize,
) -> Option<(String, DirExpr, Option<usize>)> {
    let DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };

    let match_addition = |expr: &DirExpr, cursor: &str| -> Option<DirExpr> {
        let stripped = strip_casts(expr);
        if let DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: add_lhs,
            rhs: add_rhs,
            ..
        } = stripped
        {
            if matches!(strip_casts(add_lhs.as_ref()), DirExpr::Var(name) if name == cursor) {
                return Some(*add_rhs.clone());
            }
            if matches!(strip_casts(add_rhs.as_ref()), DirExpr::Var(name) if name == cursor) {
                return Some(*add_lhs.clone());
            }
        }
        None
    };

    if let DirExpr::Var(cursor) = strip_casts(lhs.as_ref()) {
        if let Some(count_expr) = match_addition(rhs.as_ref(), cursor) {
            return Some((cursor.clone(), count_expr, None));
        }
    }
    if let DirExpr::Var(cursor) = strip_casts(rhs.as_ref()) {
        if let Some(count_expr) = match_addition(lhs.as_ref(), cursor) {
            return Some((cursor.clone(), count_expr, None));
        }
    }

    match (strip_casts(lhs.as_ref()), strip_casts(rhs.as_ref())) {
        (DirExpr::Var(cursor), DirExpr::Var(end)) => {
            let (scan_idx, count_expr) =
                find_pointer_end_assignment_before(stmts, loop_idx, cursor, end)?;
            Some((cursor.clone(), count_expr, Some(scan_idx)))
        }
        _ => None,
    }
}

fn try_guarded_dowhile_pointer_iv_upgrade(
    stmts: &mut [DirStmt],
    locals: &mut Vec<DirBinding>,
    loop_idx: usize,
    active_guards: &[DirExpr],
) -> bool {
    let (cond, body) = {
        let DirStmt::DoWhile { cond, body } = &stmts[loop_idx] else {
            return false;
        };
        (cond.clone(), body.clone())
    };

    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let Some((cursor_str, count_expr, end_ptr_idx_opt)) =
        extract_pointer_cursor_and_count(&cond, stmts, loop_idx)
    else {
        return false;
    };
    let cursor = &cursor_str;
    let loop_variant = loop_variant_vars(&body);
    let Some((update_idx, true)) = find_iv_update(&body, cursor, &loop_variant) else {
        return false;
    };
    let after_labels = labels_after(stmts, loop_idx);
    let count_cmp = positive_count_entry_guard_cmp(stmts, loop_idx, &count_expr, &after_labels)
        .or_else(|| {
            if !active_guards.is_empty() {
                Some(DirBinaryOp::SLt)
            } else {
                None
            }
        });
    let Some(count_cmp) = count_cmp else {
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
        locals.push(DirBinding {
            name: index_name.clone(),
            ty: index_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        let init_stmt = DirStmt::Assign {
            lhs: DirLValue::Var(index_name.clone()),
            rhs: DirExpr::Const(0, index_ty.clone()),
        };
        let cond = DirExpr::Binary {
            op: count_cmp,
            lhs: Box::new(DirExpr::Var(index_name.clone())),
            rhs: Box::new(count_expr),
            ty: NirType::Bool,
        };
        let update_stmt = DirStmt::Assign {
            lhs: DirLValue::Var(index_name.clone()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(DirExpr::Var(index_name.clone())),
                rhs: Box::new(DirExpr::Const(1, index_ty.clone())),
                ty: index_ty,
            },
        };
        stmts[loop_idx] = DirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: indexed_body,
        };
        if let Some(end_ptr_idx) = end_ptr_idx_opt {
            stmts[end_ptr_idx] = DirStmt::Block(Vec::new());
        }
        return true;
    }

    stmts[loop_idx] = DirStmt::For {
        init: None,
        cond: Some(cond),
        update: Some(Box::new(update_stmt)),
        body: new_body,
    };
    if let Some(end_ptr_idx) = end_ptr_idx_opt {
        stmts[end_ptr_idx] = DirStmt::Block(Vec::new());
    }
    true
}

/// Replace `goto label` with `continue` within `stmts`, without recursing into
/// nested loops (where `continue` would bind to the inner loop, not the outer).
fn replace_gotos_with_continue_shallow(stmts: &mut Vec<DirStmt>, label: &str) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Goto(target) if target == label => {
                *stmt = DirStmt::Continue;
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                replace_gotos_with_continue_shallow(then_body, label);
                replace_gotos_with_continue_shallow(else_body, label);
            }
            DirStmt::Block(body) => replace_gotos_with_continue_shallow(body, label),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    replace_gotos_with_continue_shallow(&mut case.body, label);
                }
                replace_gotos_with_continue_shallow(default, label);
            }
            // Do NOT recurse into nested loops: continue binds to the inner loop there.
            _ => {}
        }
    }
}

/// Try to upgrade a `for (;;)` loop at `stmts[loop_idx]` to a proper counted
/// `for` loop when the body ends with the tail pattern:
///
/// ```text
/// Label(L)              ← update-section label (forward goto target)
/// iv_var = iv_var ± k   ← IV increment
/// if (break_cond) break ← exit check
/// ```
///
/// The body may also contain `goto L` (acting as `continue`), which are
/// replaced with `continue` statements after the transformation.
/// An init assignment `iv_var = <val>` must immediately precede the loop.
fn try_upgrade_infinite_for_with_tail_update(stmts: &mut Vec<DirStmt>, loop_idx: usize) -> bool {
    let body = match &stmts[loop_idx] {
        DirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        } => body.clone(),
        _ => return false,
    };

    let n = body.len();
    if n < 3 {
        return false;
    }

    // body[n-3]: Label(L)  body[n-2]: iv_update  body[n-1]: if(break_cond) break;
    let update_label = match &body[n - 3] {
        DirStmt::Label(l) => l.clone(),
        _ => return false,
    };

    let iv_update = body[n - 2].clone();
    let iv_name = match &iv_update {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } => name.clone(),
        _ => return false,
    };

    let break_cond = match &body[n - 1] {
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } if else_body.is_empty() && matches!(then_body.as_slice(), [DirStmt::Break]) => {
            cond.clone()
        }
        _ => return false,
    };

    // Safety: no explicit `continue` in the body (hoisting the update would change semantics).
    if super::for_loops::stmt_list_contains_continue_pub(&body) {
        return false;
    }

    // Validate that iv_update is a linear recurrence on iv_name.
    let loop_variant = loop_variant_vars(&body);
    let update_ok = match &iv_update {
        DirStmt::Assign { rhs, .. } => {
            is_iv_update_dataflow(rhs, &iv_name, &loop_variant, &body, 0)
        }
        _ => false,
    };
    if !update_ok {
        return false;
    }

    // Find the init assignment immediately before the loop.
    let init_idx = match find_init_before(stmts, loop_idx, &iv_name) {
        Some(i) => i,
        None => return false,
    };

    // Build the new body: strip the tail 3 statements and replace goto→continue.
    let mut new_body = body[..n - 3].to_vec();
    replace_gotos_with_continue_shallow(&mut new_body, &update_label);

    // Negate the break condition to get the loop-continuation condition.
    let loop_cond = invert_condition(break_cond);

    // Apply transformation: lift init out of stmts, adjust loop_idx, rebuild For.
    let init_stmt = stmts[init_idx].clone();
    stmts.remove(init_idx);
    let loop_idx = loop_idx - 1; // shifted after init removal

    stmts[loop_idx] = DirStmt::For {
        init: Some(Box::new(init_stmt)),
        cond: Some(loop_cond),
        update: Some(Box::new(iv_update)),
        body: new_body,
    };
    true
}

fn apply_scev_upgrade_in_stmts(
    stmts: &mut Vec<DirStmt>,
    locals: &mut Vec<DirBinding>,
    active_guards: &mut Vec<DirExpr>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], DirStmt::While { .. }) {
            if try_scev_upgrade(stmts, i) {
                changed = true;
                continue;
            }
        } else if matches!(&stmts[i], DirStmt::DoWhile { .. })
            && try_guarded_dowhile_pointer_iv_upgrade(stmts, locals, i, active_guards)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            DirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_recover_row_stride_fill_inner_loop(stmts, i, locals)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            DirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_split_merged_dual_iv_tail(stmts, i, locals)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            DirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_upgrade_infinite_for_with_tail_update(stmts, i)
        {
            changed = true;
            continue;
        }
        match &mut stmts[i] {
            DirStmt::If {
                then_body,
                else_body,
                cond,
            } => {
                active_guards.push(cond.clone());
                changed |= apply_scev_upgrade_in_stmts(then_body, locals, active_guards);
                active_guards.pop();
                changed |= apply_scev_upgrade_in_stmts(else_body, locals, active_guards);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= apply_scev_upgrade_in_stmts(body, locals, active_guards);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_scev_upgrade_in_stmts(&mut case.body, locals, active_guards);
                }
                changed |= apply_scev_upgrade_in_stmts(default, locals, active_guards);
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
pub fn apply_iv_recovery_pass(func: &mut DirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::default();
    count_goto_targets(&func.body, &mut goto_counts);
    let mut active_guards = Vec::new();
    apply_tail_label_loop_recovery_in_stmts(&mut func.body, &goto_counts)
        | apply_scev_upgrade_in_stmts(&mut func.body, &mut func.locals, &mut active_guards)
}

/// Apply break/continue recovery across the entire function body.
/// Returns `true` if any transformation was made.
pub fn apply_break_continue_pass(func: &mut DirFunction) -> bool {
    let mut goto_counts: HashMap<String, usize> = HashMap::default();
    count_goto_targets(&func.body, &mut goto_counts);
    apply_break_continue_in_stmts(&mut func.body, &goto_counts)
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn int(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn ptr_u32() -> NirType {
        NirType::Ptr(Box::new(int(32, false)))
    }

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    fn const_i(value: i64) -> DirExpr {
        DirExpr::Const(value, int(64, true))
    }

    fn add(lhs: DirExpr, rhs: DirExpr, ty: NirType) -> DirExpr {
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty,
        }
    }

    fn ne(lhs: DirExpr, rhs: DirExpr) -> DirExpr {
        DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn guarded_pointer_dowhile_upgrades_to_for() {
        let mut func = DirFunction {
            name: "guarded_pointer_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::SLe,
                        lhs: Box::new(var("len")),
                        rhs: Box::new(const_i(0)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![DirStmt::Goto("exit".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                DirStmt::DoWhile {
                    body: vec![
                        DirStmt::Assign {
                            lhs: DirLValue::Var("value".to_string()),
                            rhs: DirExpr::Load {
                                ptr: Box::new(var("ptr")),
                                ty: int(32, false),
                            },
                        },
                        DirStmt::Assign {
                            lhs: DirLValue::Var("ptr".to_string()),
                            rhs: add(var("ptr"), const_i(1), ptr_u32()),
                        },
                    ],
                    cond: ne(var("ptr"), var("end")),
                },
                DirStmt::Label("exit".to_string()),
                DirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let DirStmt::For {
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
            Some(DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs,
                rhs,
                ..
            }) if matches!(lhs.as_ref(), DirExpr::Var(name) if name == "iVar0")
                && matches!(rhs.as_ref(), DirExpr::Var(name) if name == "len")
        ));
        assert!(update.is_some());
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs:
                    DirExpr::Index {
                        base,
                        index,
                        elem_ty
                    },
            } if name == "value"
                && matches!(base.as_ref(), DirExpr::Var(name) if name == "ptr")
                && matches!(index.as_ref(), DirExpr::Var(name) if name == "iVar0")
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
        let mut func = DirFunction {
            name: "unguarded_pointer_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                DirStmt::DoWhile {
                    body: vec![DirStmt::Assign {
                        lhs: DirLValue::Var("ptr".to_string()),
                        rhs: add(var("ptr"), const_i(1), ptr_u32()),
                    }],
                    cond: ne(var("ptr"), var("end")),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[1], DirStmt::DoWhile { .. }));
    }

    #[test]
    fn early_return_guarded_pointer_dowhile_upgrades_to_indexed_for() {
        let mut func = DirFunction {
            name: "early_return_guarded_pointer_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: int(32, false),
            surface_return_type_name: None,
            body: vec![
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::SLe,
                        lhs: Box::new(var("len")),
                        rhs: Box::new(const_i(0)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![DirStmt::Return(Some(DirExpr::Const(0, int(32, false))))],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("end".to_string()),
                    rhs: add(var("ptr"), var("len"), ptr_u32()),
                },
                DirStmt::DoWhile {
                    body: vec![
                        DirStmt::Assign {
                            lhs: DirLValue::Var("sum".to_string()),
                            rhs: DirExpr::Binary {
                                op: DirBinaryOp::Add,
                                lhs: Box::new(var("sum")),
                                rhs: Box::new(DirExpr::Load {
                                    ptr: Box::new(var("ptr")),
                                    ty: int(32, false),
                                }),
                                ty: int(32, false),
                            },
                        },
                        DirStmt::Assign {
                            lhs: DirLValue::Var("ptr".to_string()),
                            rhs: add(var("ptr"), const_i(1), ptr_u32()),
                        },
                    ],
                    cond: ne(var("ptr"), var("end")),
                },
                DirStmt::Return(Some(var("sum"))),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let DirStmt::For { body, cond, .. } = &func.body[2] else {
            panic!("expected early-return guarded do-while to become for");
        };
        assert!(matches!(
            cond,
            Some(DirExpr::Binary {
                op: DirBinaryOp::SLt,
                lhs,
                rhs,
                ..
            }) if matches!(lhs.as_ref(), DirExpr::Var(name) if name == "iVar0")
                && matches!(rhs.as_ref(), DirExpr::Var(name) if name == "len")
        ));
        assert!(matches!(
            &body[0],
            DirStmt::Assign {
                rhs:
                    DirExpr::Binary {
                        rhs,
                        ..
                    },
                ..
            } if matches!(
                rhs.as_ref(),
                DirExpr::Index { base, index, .. }
                    if matches!(base.as_ref(), DirExpr::Var(name) if name == "ptr")
                        && matches!(index.as_ref(), DirExpr::Var(name) if name == "iVar0")
            )
        ));
    }

    #[test]
    fn tail_label_counted_loop_becomes_break_guarded_for() {
        let mut func = DirFunction {
            name: "tail_label_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                DirStmt::Label("head".to_string()),
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: add(var("sum"), var("i"), int(64, true)),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                DirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![DirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], DirStmt::Assign { .. }));
        let DirStmt::For {
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
            Some(DirStmt::If {
                cond:
                    DirExpr::Binary {
                        op: DirBinaryOp::Eq,
                        lhs,
                        rhs,
                        ..
                    },
                then_body,
                else_body,
            }) if matches!(lhs.as_ref(), DirExpr::Var(name) if name == "i")
                && matches!(rhs.as_ref(), DirExpr::Var(name) if name == "n")
                && matches!(then_body.as_slice(), [DirStmt::Break])
                && else_body.is_empty()
        ));
        assert!(matches!(func.body[2], DirStmt::Return(None)));
        assert!(
            !func
                .body
                .iter()
                .any(|stmt| matches!(stmt, DirStmt::Label(label) if label == "head"))
        );
    }

    #[test]
    fn tail_label_loop_allows_body_local_goto() {
        let mut func = DirFunction {
            name: "tail_label_loop_with_local_goto".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Label("head".to_string()),
                DirStmt::If {
                    cond: var("flag"),
                    then_body: vec![DirStmt::Goto("inside".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: add(var("sum"), const_i(1), int(64, true)),
                },
                DirStmt::Label("inside".to_string()),
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                DirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![DirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let DirStmt::For { body, .. } = &func.body[0] else {
            panic!("expected local-goto tail loop to become for");
        };
        assert!(
            body.iter()
                .any(|stmt| matches!(stmt, DirStmt::Label(label) if label == "inside"))
        );
    }

    #[test]
    fn tail_label_loop_rejects_nonlocal_body_goto() {
        let mut func = DirFunction {
            name: "tail_label_loop_with_external_goto".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Label("head".to_string()),
                DirStmt::If {
                    cond: var("flag"),
                    then_body: vec![DirStmt::Goto("exit".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                DirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![DirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Label("exit".to_string()),
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], DirStmt::Label(_)));
    }

    #[test]
    fn tail_label_loop_rejects_multiple_backedges_to_head() {
        let mut func = DirFunction {
            name: "tail_label_loop_with_multiple_backedges".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Label("head".to_string()),
                DirStmt::If {
                    cond: var("retry"),
                    then_body: vec![DirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: add(var("i"), const_i(1), int(64, true)),
                },
                DirStmt::If {
                    cond: ne(var("i"), var("n")),
                    then_body: vec![DirStmt::Goto("head".to_string())],
                    else_body: Vec::new(),
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
        assert!(matches!(func.body[0], DirStmt::Label(_)));
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
        let mut func = DirFunction {
            name: "dataflow_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                DirStmt::While {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        DirStmt::Assign {
                            lhs: DirLValue::Var("t".to_string()),
                            rhs: add(var("i"), const_i(1), int(64, true)),
                        },
                        DirStmt::Assign {
                            lhs: DirLValue::Var("i".to_string()),
                            rhs: var("t"),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let DirStmt::For {
            init,
            cond,
            update,
            body,
        } = &func.body[0]
        else {
            panic!("Expected loop to become a For loop!");
        };
        assert!(init.is_some());
        assert!(cond.is_some());
        assert!(update.is_some());
        // The body should have had the update statement (i = t) removed, and only contain t = i + 1.
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(lhs),
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
        let mut func = DirFunction {
            name: "invalid_dataflow_loop".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("i".to_string()),
                    rhs: const_i(0),
                },
                DirStmt::While {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: Box::new(var("i")),
                        rhs: Box::new(var("n")),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        DirStmt::Assign {
                            lhs: DirLValue::Var("t".to_string()),
                            rhs: const_i(42),
                        },
                        DirStmt::Assign {
                            lhs: DirLValue::Var("i".to_string()),
                            rhs: var("t"),
                        },
                    ],
                },
            ],
            ..Default::default()
        };

        assert!(!apply_iv_recovery_pass(&mut func));
    }

    // ── for(;;) with tail-label update pattern ─────────────────────────────

    fn eq_expr(lhs: DirExpr, rhs: DirExpr) -> DirExpr {
        DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        }
    }

    /// Build:
    ///   i = 0;
    ///   for (;;) {
    ///       <work_stmts>
    ///   update_label:
    ///       i = i + 1;
    ///       if (i == limit) break;
    ///   }
    fn make_infinite_for_with_tail(
        work_stmts: Vec<DirStmt>,
        iter_var: &str,
        limit: &str,
        update_label: &str,
    ) -> (Vec<DirStmt>, NirType) {
        let ty = int(64, false);
        let body = {
            let mut b = work_stmts;
            b.push(DirStmt::Label(update_label.to_string()));
            b.push(DirStmt::Assign {
                lhs: DirLValue::Var(iter_var.to_string()),
                rhs: add(var(iter_var), const_i(1), ty.clone()),
            });
            b.push(DirStmt::If {
                cond: eq_expr(var(iter_var), var(limit)),
                then_body: vec![DirStmt::Break],
                else_body: vec![],
            });
            b
        };
        let stmts = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var(iter_var.to_string()),
                rhs: const_i(0),
            },
            DirStmt::For {
                init: None,
                cond: None,
                update: None,
                body,
            },
        ];
        (stmts, ty)
    }

    #[test]
    fn upgrades_infinite_for_with_tail_update_no_inner_goto() {
        // Simple case: for(;;) body has no goto to the update label.
        // Expected: for (i = 0; i != limit; i = i + 1) { work }
        let (mut stmts, _ty) = make_infinite_for_with_tail(
            vec![DirStmt::Expr(DirExpr::Const(42, int(64, false)))],
            "i",
            "limit",
            "update_lbl",
        );

        assert!(try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
        assert_eq!(stmts.len(), 1, "init should be absorbed into for-init");

        let DirStmt::For {
            init,
            cond,
            update,
            body,
        } = &stmts[0]
        else {
            panic!("expected For");
        };
        assert!(init.is_some(), "init should be set");
        assert!(cond.is_some(), "cond should be set");
        assert!(update.is_some(), "update should be set");
        assert_eq!(body.len(), 1, "body should have work stmt only");

        // Condition should be the negation: i != limit
        let cond = cond.as_ref().unwrap();
        assert!(
            matches!(
                cond,
                DirExpr::Binary {
                    op: DirBinaryOp::Ne,
                    ..
                }
            ),
            "cond should be Ne (inverted from Eq), got: {cond:?}"
        );
    }

    #[test]
    fn upgrades_infinite_for_and_replaces_inner_goto_with_continue() {
        // for(;;) body contains `if (early) goto update_lbl` — should become `continue`.
        let work_stmts = vec![
            DirStmt::If {
                cond: var("early"),
                then_body: vec![DirStmt::Goto("update_lbl".to_string())],
                else_body: vec![],
            },
            DirStmt::Expr(DirExpr::Const(99, int(64, false))),
        ];
        let (mut stmts, _ty) = make_infinite_for_with_tail(work_stmts, "i", "limit", "update_lbl");

        assert!(try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));

        let DirStmt::For { body, .. } = &stmts[0] else {
            panic!("expected For");
        };
        // Body should have the if-continue and the expr, no label or break-if.
        assert_eq!(body.len(), 2);
        let DirStmt::If { then_body, .. } = &body[0] else {
            panic!("expected If");
        };
        assert_eq!(
            then_body.as_slice(),
            [DirStmt::Continue],
            "goto should have been replaced with continue"
        );
    }

    #[test]
    fn does_not_upgrade_infinite_for_when_no_init_before_loop() {
        // No assignment to `i` immediately before the loop → no transformation.
        let body = vec![
            DirStmt::Label("lbl".to_string()),
            DirStmt::Assign {
                lhs: DirLValue::Var("i".to_string()),
                rhs: add(var("i"), const_i(1), int(64, false)),
            },
            DirStmt::If {
                cond: eq_expr(var("i"), var("limit")),
                then_body: vec![DirStmt::Break],
                else_body: vec![],
            },
        ];
        let mut stmts = vec![
            DirStmt::Expr(DirExpr::Const(0, int(64, false))), // unrelated stmt, not an init for i
            DirStmt::For {
                init: None,
                cond: None,
                update: None,
                body,
            },
        ];

        assert!(!try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
    }

    #[test]
    fn does_not_upgrade_when_body_has_explicit_continue() {
        // An existing `continue` in the body blocks the transformation.
        let work_stmts = vec![
            DirStmt::Continue, // existing continue — safety check must reject
            DirStmt::Expr(DirExpr::Const(1, int(64, false))),
        ];
        let (mut stmts, _) = make_infinite_for_with_tail(work_stmts, "i", "limit", "update_lbl");

        assert!(!try_upgrade_infinite_for_with_tail_update(&mut stmts, 1));
    }

    #[test]
    fn splits_merged_dual_iv_tail_on_infinite_for() {
        let ty = int(64, false);
        let body = vec![
            DirStmt::Expr(DirExpr::Const(1, ty.clone())),
            DirStmt::Assign {
                lhs: DirLValue::Var("off".to_string()),
                rhs: add(var("off"), const_i(1), ty.clone()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("off".to_string()),
                rhs: add(var("off"), var("cols"), ty.clone()),
            },
            DirStmt::If {
                cond: eq_expr(var("rows"), var("off")),
                then_body: vec![DirStmt::Break],
                else_body: vec![],
            },
        ];
        let mut stmts = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("off".to_string()),
                rhs: const_i(0),
            },
            DirStmt::For {
                init: None,
                cond: None,
                update: None,
                body,
            },
        ];
        let mut locals = Vec::new();
        assert!(try_split_merged_dual_iv_tail(&mut stmts, 1, &mut locals));
        assert_eq!(stmts.len(), 3, "off init + row init + for");
        assert_eq!(locals.len(), 1, "fresh row counter local");

        let DirStmt::For { body, .. } = &stmts[2] else {
            panic!("expected For");
        };
        let tail = tail_meaningful_stmts(body, 3).expect("tail triple");
        let (_, step_one) = &tail[0];
        let (_, step_k) = &tail[1];
        let (_, break_if) = &tail[2];

        let (row, _) = parse_self_add_assign(step_one).expect("row += 1");
        assert_ne!(row, "off");
        let (off, addend) = parse_self_add_assign(step_k).expect("off += cols");
        assert_eq!(off, "off");
        assert!(matches!(strip_casts(&addend), DirExpr::Var(v) if v == "cols"));
        let (_, break_var) = parse_break_eq_var(break_if).expect("rows == row");
        assert_eq!(break_var, row);
    }

    #[test]
    fn recovers_row_stride_fill_inner_loop_in_infinite_for() {
        let ty = int(64, false);
        let u32_ty = int(32, false);
        let body = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("tmp".to_string()),
                rhs: DirExpr::Cast {
                    ty: ty.clone(),
                    expr: Box::new(var("row_off")),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".to_string()),
                rhs: add(var("matrix"), var("tmp"), ptr_u32()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("tmp".to_string()),
                rhs: add(var("tmp"), var("cols"), ty.clone()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("end".to_string()),
                rhs: add(var("matrix"), var("tmp"), ptr_u32()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("end".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Sub,
                    lhs: Box::new(var("end")),
                    rhs: Box::new(var("cursor")),
                    ty: ty.clone(),
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("end".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::And,
                    lhs: Box::new(DirExpr::Cast {
                        ty: ty.clone(),
                        expr: Box::new(var("end")),
                    }),
                    rhs: Box::new(DirExpr::Const(4, ty.clone())),
                    ty: ty.clone(),
                },
            },
            DirStmt::Label("inner".to_string()),
            DirStmt::Assign {
                lhs: DirLValue::Deref {
                    ptr: Box::new(var("cursor")),
                    ty: u32_ty.clone(),
                },
                rhs: var("value"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("cursor".to_string()),
                rhs: add(var("cursor"), const_i(1), ptr_u32()),
            },
            DirStmt::Goto("inner".to_string()),
            DirStmt::Assign {
                lhs: DirLValue::Var("row".to_string()),
                rhs: add(var("row"), const_i(1), ty.clone()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("row_off".to_string()),
                rhs: add(var("row_off"), var("cols"), ty.clone()),
            },
            DirStmt::If {
                cond: eq_expr(var("rows"), var("row")),
                then_body: vec![DirStmt::Break],
                else_body: vec![],
            },
        ];
        let mut func = DirFunction {
            name: "row_stride_fill".to_string(),
            int_param_offsets: Vec::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::For {
                init: None,
                cond: None,
                update: None,
                body,
            }],
            ..Default::default()
        };

        assert!(apply_iv_recovery_pass(&mut func));
        let DirStmt::For { body, .. } = &func.body[0] else {
            panic!("expected For");
        };
        assert!(
            body.iter().any(|stmt| matches!(stmt, DirStmt::For { .. })),
            "expected nested inner for, got {body:?}"
        );
        let inner = body
            .iter()
            .find_map(|stmt| {
                if let DirStmt::For {
                    init,
                    cond,
                    update,
                    body: inner_body,
                } = stmt
                {
                    Some((
                        init.is_some(),
                        cond.is_some(),
                        update.is_some(),
                        inner_body.len(),
                    ))
                } else {
                    None
                }
            })
            .expect("inner for");
        assert!(inner.0 && inner.1 && inner.2);
        assert_eq!(inner.3, 1);
    }
}

fn substitute_var_in_expr(expr: &mut DirExpr, name: &str, replacement: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(var) if var == name => {
            *expr = replacement.clone();
            true
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. }
        | DirExpr::FieldAccess { base: expr, .. } => {
            substitute_var_in_expr(expr, name, replacement)
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            substitute_var_in_expr(lhs, name, replacement)
                | substitute_var_in_expr(rhs, name, replacement)
        }
        DirExpr::Call { args, .. } => args
            .iter_mut()
            .any(|arg| substitute_var_in_expr(arg, name, replacement)),
        DirExpr::PtrOffset { base, .. } => substitute_var_in_expr(base, name, replacement),
        DirExpr::Index { base, index, .. } => {
            substitute_var_in_expr(base, name, replacement)
                | substitute_var_in_expr(index, name, replacement)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_var_in_expr(cond, name, replacement)
                | substitute_var_in_expr(then_expr, name, replacement)
                | substitute_var_in_expr(else_expr, name, replacement)
        }
        _ => false,
    }
}
