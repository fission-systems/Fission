//! Presentation-only recovery of unstructured goto/label control flow.
//!
//! This module owns the HIR cleanup that turns compiler-shaped labels and
//! gotos into structured while/if/else forms, expands shared return labels,
//! and removes unreachable or unreferenced label scaffolding.  It depends
//! only on HIR statements and the presentation owner's fixed-point driver.

use super::{HirExpr, HirStmt, HirUnaryOp, NirType};
use std::collections::{HashMap, HashSet};

// ── Goto / label presentation recovery ───────────────────────────────────────

pub(super) fn invert_cond(cond: HirExpr) -> HirExpr {
    match cond {
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

fn if_is_single_goto(stmt: &HirStmt) -> Option<(&HirExpr, &str)> {
    match stmt {
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } if else_body.is_empty() => match then_body.as_slice() {
            [HirStmt::Goto(label)] => Some((cond, label.as_str())),
            _ => None,
        },
        _ => None,
    }
}

fn stmts_have_label(stmts: &[HirStmt], label: &str) -> bool {
    stmts.iter().any(|s| match s {
        HirStmt::Label(l) => l == label,
        HirStmt::Block(b)
        | HirStmt::While { body: b, .. }
        | HirStmt::DoWhile { body: b, .. }
        | HirStmt::For { body: b, .. } => stmts_have_label(b, label),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => stmts_have_label(then_body, label) || stmts_have_label(else_body, label),
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmts_have_label(&c.body, label))
                || stmts_have_label(default, label)
        }
        _ => false,
    })
}

fn count_goto_refs(stmts: &[HirStmt], label: &str) -> usize {
    stmts.iter().map(|s| count_goto_refs_stmt(s, label)).sum()
}

fn count_goto_refs_stmt(stmt: &HirStmt, label: &str) -> usize {
    match stmt {
        HirStmt::Goto(l) => usize::from(l == label),
        HirStmt::Block(b) | HirStmt::While { body: b, .. } | HirStmt::DoWhile { body: b, .. } => {
            count_goto_refs(b, label)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => count_goto_refs(then_body, label) + count_goto_refs(else_body, label),
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .map(|c| count_goto_refs(&c.body, label))
                .sum::<usize>()
                + count_goto_refs(default, label)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().map_or(0, |s| count_goto_refs_stmt(s, label))
                + update
                    .as_ref()
                    .map_or(0, |s| count_goto_refs_stmt(s, label))
                + count_goto_refs(body, label)
        }
        _ => 0,
    }
}

fn replace_goto_with_return(stmts: &mut [HirStmt], label: &str, ret: &Option<HirExpr>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(l) if l == label => {
                *stmt = HirStmt::Return(ret.clone());
                changed = true;
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. } => {
                changed |= replace_goto_with_return(b, label, ret);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= replace_goto_with_return(then_body, label, ret);
                changed |= replace_goto_with_return(else_body, label, ret);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= replace_goto_with_return(&mut case.body, label, ret);
                }
                changed |= replace_goto_with_return(default, label, ret);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    changed |=
                        replace_goto_with_return(std::slice::from_mut(i.as_mut()), label, ret);
                }
                if let Some(u) = update {
                    changed |=
                        replace_goto_with_return(std::slice::from_mut(u.as_mut()), label, ret);
                }
                changed |= replace_goto_with_return(body, label, ret);
            }
            _ => {}
        }
    }
    changed
}

/// `…; goto L; …; L: return e;` → replace gotos with `return e` (presentation).
pub(super) fn expand_goto_shared_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    // Recurse into nested structured bodies first.
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= expand_goto_shared_returns(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= expand_goto_shared_returns(then_body);
                changed |= expand_goto_shared_returns(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= expand_goto_shared_returns(&mut case.body);
                }
                changed |= expand_goto_shared_returns(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        if let (HirStmt::Label(label), HirStmt::Return(ret)) = (&stmts[i], &stmts[i + 1]) {
            let label = label.clone();
            let ret = ret.clone();
            if count_goto_refs(stmts, &label) > 0 {
                changed |= replace_goto_with_return(stmts, &label, &ret);
                // Drop the label if nothing targets it anymore; keep the return
                // for fall-through predecessors.
                if count_goto_refs(stmts, &label) == 0 {
                    if matches!(&stmts[i], HirStmt::Label(l) if l == &label) {
                        stmts.remove(i);
                        changed = true;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    changed
}

fn body_is_goto_recoverable(stmts: &[HirStmt]) -> bool {
    // Fallthrough/else bodies used in recovery must not define labels (would
    // break outer label indexing). Nested if/return/assign/goto are fine.
    !stmts.iter().any(|s| matches!(s, HirStmt::Label(_)))
}

///   body…
/// Lcond:
///   if (cond) goto Lbody;
/// ```
pub(super) fn recover_while_from_gotos(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= recover_while_from_gotos(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= recover_while_from_gotos(then_body);
                changed |= recover_while_from_gotos(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= recover_while_from_gotos(&mut case.body);
                }
                changed |= recover_while_from_gotos(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if try_recover_while_at(stmts, i) {
            changed = true;
            continue;
        }
        i += 1;
    }
    changed
}

fn try_recover_while_at(stmts: &mut Vec<HirStmt>, i: usize) -> bool {
    let HirStmt::Goto(lcond) = &stmts[i] else {
        return false;
    };
    let lcond = lcond.clone();

    let Some(c_idx) = stmts
        .iter()
        .enumerate()
        .skip(i + 1)
        .find_map(|(idx, s)| match s {
            HirStmt::Label(l) if l == &lcond => Some(idx),
            _ => None,
        })
    else {
        return false;
    };

    if c_idx + 1 >= stmts.len() {
        return false;
    }

    let body_region = &stmts[i + 1..c_idx];
    // Body must start with Lbody label targeted by the condition if.
    let Some(HirStmt::Label(lbody)) = body_region.first() else {
        return false;
    };
    let lbody = lbody.clone();

    let Some((cond, target)) = if_is_single_goto(&stmts[c_idx + 1]) else {
        return false;
    };
    if target != lbody {
        return false;
    }
    let cond = cond.clone();

    // Only this loop's back-edge should target Lbody (plus nothing else in range).
    // Allow gotos to Lbody only from the condition if we are about to remove.
    let goto_lbody = count_goto_refs(stmts, &lbody);
    // The condition if contributes 1; any other ref blocks recovery.
    if goto_lbody != 1 {
        return false;
    }

    let mut body: Vec<HirStmt> = body_region[1..].to_vec();
    if stmts_have_label(&body, &lcond) || stmts_have_label(&body, &lbody) {
        return false;
    }
    // No other labels inside body (would break linear while).
    if body.iter().any(|s| matches!(s, HirStmt::Label(_))) {
        return false;
    }

    // `goto Lcond` inside body → continue (recheck condition).
    rewrite_goto_to_continue(&mut body, &lcond);

    let mut rebuilt = Vec::with_capacity(stmts.len());
    rebuilt.extend_from_slice(&stmts[..i]);
    rebuilt.push(HirStmt::While { cond, body });
    // Skip: goto Lcond, Lbody.., Label(Lcond), if (cond) goto Lbody
    rebuilt.extend_from_slice(&stmts[c_idx + 2..]);
    *stmts = rebuilt;
    true
}

fn rewrite_goto_to_continue(stmts: &mut [HirStmt], label: &str) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(l) if l == label => {
                *stmt = HirStmt::Continue;
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => rewrite_goto_to_continue(b, label),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                rewrite_goto_to_continue(then_body, label);
                rewrite_goto_to_continue(else_body, label);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    rewrite_goto_to_continue(&mut case.body, label);
                }
                rewrite_goto_to_continue(default, label);
            }
            _ => {}
        }
    }
}

/// Recover structured if/else from O0 `if (c) goto` / label shapes.
pub(super) fn recover_if_else_from_gotos(
    stmts: &mut Vec<HirStmt>,
    global_goto_refs: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= recover_if_else_from_gotos(b, global_goto_refs);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= recover_if_else_from_gotos(then_body, global_goto_refs);
                changed |= recover_if_else_from_gotos(else_body, global_goto_refs);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= recover_if_else_from_gotos(&mut case.body, global_goto_refs);
                }
                changed |= recover_if_else_from_gotos(default, global_goto_refs);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if try_recover_if_else_at(stmts, i, global_goto_refs) {
            changed = true;
            // Restart from i so nested/adjacent patterns can fire.
            continue;
        }
        i += 1;
    }
    changed
}

fn try_recover_if_else_at(
    stmts: &mut Vec<HirStmt>,
    i: usize,
    global_goto_refs: &HashMap<String, usize>,
) -> bool {
    // After goto→return expansion: `if (C) return a; return b;` → if/else.
    if try_recover_if_return_else_fallthrough(stmts, i) {
        return true;
    }

    let Some((cond, lelse)) = if_is_single_goto(&stmts[i]) else {
        return false;
    };
    let cond = cond.clone();
    let lelse = lelse.to_string();
    // Every recovery below consumes `Label(lelse)`. A recursive call operates
    // on only one subtree, so subtree-local reference counts cannot prove that
    // another arm or enclosing scope does not still jump to the label.
    if global_goto_refs.get(&lelse).copied() != Some(1) {
        return false;
    }

    // Find Label(lelse) after i.
    let Some(le_idx) = stmts
        .iter()
        .enumerate()
        .skip(i + 1)
        .find_map(|(idx, s)| match s {
            HirStmt::Label(l) if l == &lelse => Some(idx),
            _ => None,
        })
    else {
        return false;
    };

    let fallthrough = &stmts[i + 1..le_idx];
    if !body_is_goto_recoverable(fallthrough) {
        return false;
    }

    // Pattern: if (C) goto Lelse; THEN; goto Lend; Label(Lelse); ELSE; Label(Lend);
    if let Some(HirStmt::Goto(lend)) = fallthrough.last() {
        let lend = lend.clone();
        if let Some(lend_idx) = stmts
            .iter()
            .enumerate()
            .skip(le_idx + 1)
            .find_map(|(idx, s)| match s {
                HirStmt::Label(l) if l == &lend => Some(idx),
                _ => None,
            })
        {
            let else_body = &stmts[le_idx + 1..lend_idx];
            if body_is_goto_recoverable(else_body) && !stmts_have_label(else_body, &lend) {
                // if (C) { else_body } else { THEN without final goto }
                let then_for_c: Vec<HirStmt> = else_body.to_vec();
                let else_for_c: Vec<HirStmt> = fallthrough[..fallthrough.len() - 1].to_vec();
                let mut rebuilt = Vec::with_capacity(stmts.len());
                rebuilt.extend_from_slice(&stmts[..i]);
                rebuilt.push(HirStmt::If {
                    cond,
                    then_body: then_for_c,
                    else_body: else_for_c,
                });
                // Keep Label(lend) and tail for other fallthroughs.
                rebuilt.extend_from_slice(&stmts[lend_idx..]);
                *stmts = rebuilt;
                return true;
            }
        }
    }

    // Pattern: if (C) goto Lelse; THEN...; Label(Lelse); ELSE...
    // where THEN has no trailing shared lend label — both sides are self-contained
    // (typically after goto→return expansion).
    let then_body = fallthrough.to_vec();
    // ELSE runs from after Lelse until next top-level label or end. If the next
    // statement after Lelse is another Label that is not needed, take until end
    // of contiguous non-label run... Actually for:
    //   if (C) goto L; THEN; L: ELSE_STMTS...
    // ELSE is everything after L until we cannot safely absorb — use rest of list
    // only when THEN is terminal (ends with return/goto/break/continue) so control
    // never falls from THEN into ELSE without the label.
    let then_terminal = then_body.last().is_some_and(|s| {
        matches!(
            s,
            HirStmt::Return(_) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue
        )
    });
    if then_terminal || then_body.is_empty() {
        // Take else body as statements after label until next Label (exclusive) or end.
        let else_end = stmts[le_idx + 1..]
            .iter()
            .position(|s| matches!(s, HirStmt::Label(_)))
            .map(|p| le_idx + 1 + p)
            .unwrap_or(stmts.len());
        let else_body = stmts[le_idx + 1..else_end].to_vec();
        if body_is_goto_recoverable(&else_body) {
            // if (C) goto Lelse → when C, run else_body; when !C, run then_body
            let mut rebuilt = Vec::with_capacity(stmts.len());
            rebuilt.extend_from_slice(&stmts[..i]);
            rebuilt.push(HirStmt::If {
                cond,
                then_body: else_body,
                else_body: then_body,
            });
            rebuilt.extend_from_slice(&stmts[else_end..]);
            *stmts = rebuilt;
            return true;
        }
    }

    // Pattern: if (C) goto Lskip; BODY; Label(Lskip);  (no else body — skip only)
    // → if (!C) { BODY }
    if le_idx + 1 == stmts.len()
        || !matches!(&stmts[le_idx + 1], HirStmt::Label(_)) && count_goto_refs(stmts, &lelse) == 1
    {
        // If there is content after the label that is not exclusively the else of
        // this if, only recover skip when nothing after label belongs to an else
        // that was meant to pair — i.e. when fallthrough is the only body and
        // label has a single goto ref.
        if count_goto_refs(stmts, &lelse) == 1 && body_is_goto_recoverable(fallthrough) {
            // Only pure skip when there is no "else" material that should pair —
            // empty after label OR after label continues sequential code that
            // both paths should reach (fallthrough from label).
            // if (!C) { fallthrough }; Label; tail
            let mut rebuilt = Vec::with_capacity(stmts.len());
            rebuilt.extend_from_slice(&stmts[..i]);
            if !fallthrough.is_empty() {
                rebuilt.push(HirStmt::If {
                    cond: invert_cond(cond),
                    then_body: fallthrough.to_vec(),
                    else_body: vec![],
                });
            }
            // Keep label for fallthrough join if still needed by others; if single
            // ref (this if, now removed), drop label.
            if count_goto_refs(&stmts[le_idx + 1..], &lelse) > 0 {
                rebuilt.extend_from_slice(&stmts[le_idx..]);
            } else {
                rebuilt.extend_from_slice(&stmts[le_idx + 1..]);
            }
            *stmts = rebuilt;
            return true;
        }
    }

    false
}

/// `if (C) { return …; } <terminal fallthrough>` → if/else.
fn try_recover_if_return_else_fallthrough(stmts: &mut Vec<HirStmt>, i: usize) -> bool {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = &stmts[i]
    else {
        return false;
    };
    if !else_body.is_empty() {
        return false;
    }
    let then_is_return = matches!(then_body.as_slice(), [HirStmt::Return(_)]);
    if !then_is_return {
        return false;
    }
    if i + 1 >= stmts.len() {
        return false;
    }
    // Absorb a straight-line terminal suffix as the else branch.
    let mut end = i + 1;
    while end < stmts.len() {
        match &stmts[end] {
            HirStmt::Label(_) => break,
            HirStmt::Return(_) => {
                end += 1;
                break;
            }
            HirStmt::Assign { .. } | HirStmt::Expr(_) => {
                end += 1;
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } if matches!(then_body.as_slice(), [HirStmt::Return(_)])
                && (else_body.is_empty()
                    || matches!(else_body.as_slice(), [HirStmt::Return(_)])) =>
            {
                // Nested already-structured if-return may be part of else.
                end += 1;
            }
            _ => break,
        }
    }
    if end == i + 1 {
        return false;
    }
    // Else must end terminal.
    if !matches!(
        stmts[end - 1],
        HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
    ) {
        return false;
    }
    let cond = cond.clone();
    let then_body = then_body.clone();
    let else_body = stmts[i + 1..end].to_vec();
    let mut rebuilt = Vec::with_capacity(stmts.len());
    rebuilt.extend_from_slice(&stmts[..i]);
    rebuilt.push(HirStmt::If {
        cond,
        then_body,
        else_body,
    });
    rebuilt.extend_from_slice(&stmts[end..]);
    *stmts = rebuilt;
    true
}

/// Labels named by any `Goto` anywhere in `stmts` (recursing into nested
/// bodies) -- a goto can jump into a scope that textually follows an early
/// return elsewhere in the function, so "referenced" has to be computed
/// function-wide, not just within the segment being pruned.
pub(super) fn collect_goto_targets(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Goto(label) => {
                out.insert(label.clone());
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. } => {
                collect_goto_targets(b, out);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_goto_targets(then_body, out);
                collect_goto_targets(else_body, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    collect_goto_targets(&c.body, out);
                }
                collect_goto_targets(default, out);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_goto_targets(std::slice::from_ref(i.as_ref()), out);
                }
                if let Some(u) = update {
                    collect_goto_targets(std::slice::from_ref(u.as_ref()), out);
                }
                collect_goto_targets(body, out);
            }
            _ => {}
        }
    }
}

pub(super) fn collect_goto_ref_counts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    let mut targets = HashSet::new();
    collect_goto_targets(stmts, &mut targets);
    for target in targets {
        out.insert(target.clone(), count_goto_refs(stmts, &target));
    }
}

/// Drop fallthrough stmts after an if whose every branch already returns.
pub(super) fn prune_unreachable_after_total_return(
    stmts: &mut Vec<HirStmt>,
    goto_targets: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= prune_unreachable_after_total_return(b, goto_targets);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= prune_unreachable_after_total_return(then_body, goto_targets);
                changed |= prune_unreachable_after_total_return(else_body, goto_targets);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    changed |= prune_unreachable_after_total_return(&mut c.body, goto_targets);
                }
                changed |= prune_unreachable_after_total_return(default, goto_targets);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if stmt_seq_always_returns(std::slice::from_ref(&stmts[i])) {
            // A `goto` elsewhere in the function may still jump into this
            // "after the return" tail (e.g. a forward jump past an early/
            // guarded return into code laid out later) -- pruning past that
            // label's target would leave the goto dangling and silently drop
            // real code from the printed tree. Only drop genuinely dead
            // filler before the first still-targeted label, and keep the
            // labeled tail itself untouched.
            let keep_from = stmts[i + 1..]
                .iter()
                .position(|s| stmt_contains_targeted_label(s, goto_targets))
                .map(|offset| i + 1 + offset);
            match keep_from {
                Some(keep_from) if keep_from > i + 1 => {
                    stmts.drain(i + 1..keep_from);
                    changed = true;
                }
                Some(_) => {}
                None => {
                    let before = stmts.len();
                    stmts.truncate(i + 1);
                    if stmts.len() != before {
                        changed = true;
                    }
                }
            }
            break;
        }
        i += 1;
    }
    changed
}

fn stmt_contains_targeted_label(stmt: &HirStmt, targets: &HashSet<String>) -> bool {
    match stmt {
        HirStmt::Label(label) => targets.contains(label),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => body
            .iter()
            .any(|stmt| stmt_contains_targeted_label(stmt, targets)),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => then_body
            .iter()
            .chain(else_body.iter())
            .any(|stmt| stmt_contains_targeted_label(stmt, targets)),
        HirStmt::For {
            init, update, body, ..
        } => init
            .iter()
            .chain(update.iter())
            .map(|stmt| stmt.as_ref())
            .chain(body.iter())
            .any(|stmt| stmt_contains_targeted_label(stmt, targets)),
        HirStmt::Switch { cases, default, .. } => cases
            .iter()
            .flat_map(|case| case.body.iter())
            .chain(default.iter())
            .any(|stmt| stmt_contains_targeted_label(stmt, targets)),
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SeqExit {
    /// Every path ends in `return`.
    Return,
    /// Some path falls through the end of the sequence/statement.
    Fallthrough,
    /// Some path leaves via goto/break/continue (or mixed non-return exit).
    Other,
}

fn stmt_seq_always_returns(stmts: &[HirStmt]) -> bool {
    matches!(seq_exit(stmts), SeqExit::Return)
}

fn seq_exit(stmts: &[HirStmt]) -> SeqExit {
    let mut i = 0;
    while i < stmts.len() {
        match stmt_exit(&stmts[i]) {
            SeqExit::Return => return SeqExit::Return,
            SeqExit::Other => return SeqExit::Other,
            SeqExit::Fallthrough => i += 1,
        }
    }
    SeqExit::Fallthrough
}

fn stmt_exit(stmt: &HirStmt) -> SeqExit {
    match stmt {
        HirStmt::Return(_) => SeqExit::Return,
        HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => SeqExit::Other,
        HirStmt::Block(b) => seq_exit(b),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let t = seq_exit(then_body);
            let e = if else_body.is_empty() {
                SeqExit::Fallthrough
            } else {
                seq_exit(else_body)
            };
            merge_branch_exit(t, e)
        }
        HirStmt::Switch { cases, default, .. } => {
            if cases.is_empty() {
                return SeqExit::Fallthrough;
            }
            let mut acc = seq_exit(default);
            for case in cases {
                acc = merge_branch_exit(acc, seq_exit(&case.body));
            }
            acc
        }
        // Assigns/labels fall through; loops conservatively may fall through.
        _ => SeqExit::Fallthrough,
    }
}

fn merge_branch_exit(a: SeqExit, b: SeqExit) -> SeqExit {
    use SeqExit::*;
    match (a, b) {
        (Return, Return) => Return,
        (Fallthrough, Fallthrough) => Fallthrough,
        (Return, Fallthrough) | (Fallthrough, Return) => Fallthrough,
        _ => Other,
    }
}

pub(super) fn remove_unreferenced_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let mut labels = HashSet::new();
    collect_labels(stmts, &mut labels);
    let mut referenced = HashSet::new();
    for lab in &labels {
        if count_goto_refs(stmts, lab) > 0 {
            referenced.insert(lab.clone());
        }
    }
    remove_labels_not_in(stmts, &referenced)
}

fn collect_labels(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Label(l) => {
                out.insert(l.clone());
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => collect_labels(b, out),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_labels(then_body, out);
                collect_labels(else_body, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    collect_labels(&c.body, out);
                }
                collect_labels(default, out);
            }
            _ => {}
        }
    }
}

fn remove_labels_not_in(stmts: &mut Vec<HirStmt>, keep: &HashSet<String>) -> bool {
    let before = stmts.len();
    stmts.retain(|s| match s {
        HirStmt::Label(l) => keep.contains(l),
        _ => true,
    });
    let mut changed = stmts.len() != before;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= remove_labels_not_in(b, keep);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= remove_labels_not_in(then_body, keep);
                changed |= remove_labels_not_in(else_body, keep);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    changed |= remove_labels_not_in(&mut c.body, keep);
                }
                changed |= remove_labels_not_in(default, keep);
            }
            _ => {}
        }
    }
    changed
}
