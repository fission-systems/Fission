use fission_midend_core::ir::*;
use fission_midend_dir::ir::*;
use fission_midend_dir::util::{collect_referenced_label_counts, negate_expr};
use crate::HashMap;
use crate::HashSet;

// ---------------------------------------------------------------------------
// Goto elimination post-pass
// ---------------------------------------------------------------------------
//
// Three fixpoint rules applied in sequence until convergence:
//
//  1. Empty-jump removal:   `Goto(L)` immediately followed by `Label(L)` → remove the Goto.
//  2. Single-reference inline: `Label(L)` referenced exactly once as `Goto(L)` → remove the
//     Label and the Goto (they're already adjacent after rule 1 or after inlining).
//  3. Conditional goto inversion: `if (cond) { Goto(L) }` directly followed by `Label(L)`
//     and the rest of the code → replace with `if (!cond) { rest_code }`.
//  4. Guard clause promotion: `if (cond) { Goto(L) }; code; L: return val` →
//     `if (cond) { return val }; code`.  Handles the extremely common early-exit guard
//     pattern where a forward goto jumps over the main body to a trailing return.
//
// Rules are applied at the TOP LEVEL only (not recursed into nested scopes) per iteration.
// After each pass that changes anything, the whole pass restarts to reach a fixpoint.

/// Apply all three goto-elimination rules at the top level of a statement list.
/// Returns `(cleaned, changed)` where `changed` indicates whether any rule fired.
fn goto_elim_pass(stmts: Vec<DirStmt>) -> (Vec<DirStmt>, bool) {
    let mut changed = false;
    let stmts = strip_unreachable_after_unconditional_transfer(stmts, &mut changed);
    let stmts = empty_jump_removal(stmts, &mut changed);
    let stmts = single_ref_label_inline(stmts, &mut changed);
    let stmts = guard_clause_promotion(stmts, &mut changed);
    let stmts = cond_goto_inversion(stmts, &mut changed);
    (stmts, changed)
}

fn strip_unreachable_after_unconditional_transfer(
    stmts: Vec<DirStmt>,
    changed: &mut bool,
) -> Vec<DirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut dropping = false;
    for (idx, stmt) in stmts.iter().cloned().enumerate() {
        if dropping {
            if matches!(stmt, DirStmt::Label(_)) {
                dropping = false;
                out.push(stmt);
            } else {
                *changed = true;
            }
            continue;
        }

        dropping = match &stmt {
            DirStmt::Goto(label) => stmts[idx + 1..]
                .iter()
                .any(|candidate| matches!(candidate, DirStmt::Label(next) if next == label)),
            _ => false,
        };
        out.push(stmt);
    }
    out
}

/// Rule 1: If a `Goto(L)` is immediately followed by `Label(L)`, remove the Goto.
fn empty_jump_removal(stmts: Vec<DirStmt>, changed: &mut bool) -> Vec<DirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        if let DirStmt::Goto(ref label) = stmt {
            if let Some(DirStmt::Label(next_label)) = iter.peek() {
                if label == next_label {
                    *changed = true;
                    continue; // drop the Goto; Label stays
                }
            }
        }
        out.push(stmt);
    }
    out
}

/// Rule 2: If a `Label(L)` is referenced exactly once (as a `Goto(L)`) in the same list,
/// and that Goto immediately precedes the Label (after rule 1), remove both.
fn single_ref_label_inline(stmts: Vec<DirStmt>, changed: &mut bool) -> Vec<DirStmt> {
    let ref_counts = collect_referenced_label_counts(&stmts);
    let singleton_labels: HashSet<&str> = ref_counts
        .iter()
        .filter(|&(_, &count)| count == 1)
        .map(|(label, _)| label.as_str())
        .collect();
    if singleton_labels.is_empty() {
        return stmts;
    }

    let mut out = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        // If we see `Goto(L)` where L has exactly one reference and the next stmt is
        // `Label(L)`, drop both (the label was already removed by rule 1 in the same
        // pass, or the Goto and Label are genuinely adjacent here).
        if let DirStmt::Goto(ref label) = stmt {
            if singleton_labels.contains(label.as_str()) {
                if let Some(DirStmt::Label(next_label)) = iter.peek() {
                    if label == next_label {
                        *changed = true;
                        let _ = iter.next(); // consume the Label
                        continue;
                    }
                }
            }
        }
        out.push(stmt);
    }
    out
}

/// Rule 3: `if (cond) { Goto(L) }` directly followed by `Label(L)` + rest →
/// `if (!cond) { rest }`.  This handles early-exit / guard patterns.
fn cond_goto_inversion(stmts: Vec<DirStmt>, changed: &mut bool) -> Vec<DirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        // Pattern: If { cond, then=[Goto(L)], else=[] }  followed by  Label(L)  and rest
        if let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if else_body.is_empty() {
                if let [DirStmt::Goto(goto_label)] = then_body.as_slice() {
                    // Find the immediately following Label(L) at the top level.
                    if i + 1 < stmts.len() {
                        if let DirStmt::Label(label) = &stmts[i + 1] {
                            if goto_label == label {
                                // Collect everything after the label as the inlined else body.
                                let inverted_cond = negate_expr(cond.clone());
                                let rest_body: Vec<DirStmt> = stmts[i + 2..].to_vec();
                                if !rest_body.is_empty() {
                                    *changed = true;
                                    out.push(DirStmt::If {
                                        cond: inverted_cond,
                                        then_body: rest_body,
                                        else_body: Vec::new(),
                                    });
                                    break; // rest_body is now inside the if, stop iteration
                                }
                            }
                        }
                    }
                }
            }
        }
        out.push(stmts[i].clone());
        i += 1;
    }
    out
}

/// Rule 4: Guard clause promotion.
///
/// Pattern: `if (cond) { Goto(L) }; <main_body>; L: <tail>` where `<tail>` is a
/// simple return (possibly preceded by assignments) and `L` is referenced only
/// once in the whole statement list.
///
/// Transformed to: `if (cond) { <tail> }; <main_body>`.
///
/// This is the dominant pattern for early-exit guards generated by compilers:
/// ```text
///   cmp ecx, 0
///   jle .Lreturn_zero
///   ; ... main loop body ...
///   ret
/// .Lreturn_zero:
///   xor eax, eax
///   ret
/// ```
fn guard_clause_promotion(stmts: Vec<DirStmt>, changed: &mut bool) -> Vec<DirStmt> {
    let ref_counts = collect_referenced_label_counts(&stmts);
    let mut out = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        // Look for: if (cond) { Goto(L) } where L is referenced exactly once.
        if let DirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if else_body.is_empty() {
                if let [DirStmt::Goto(goto_label)] = then_body.as_slice() {
                    if ref_counts.get(goto_label).copied() == Some(1) {
                        // Scan forward for `Label(L)` at the top level.
                        if let Some(label_pos) = (i + 1..stmts.len())
                            .find(|&j| matches!(&stmts[j], DirStmt::Label(l) if l == goto_label))
                        {
                            // Collect the tail after the label.
                            let tail: Vec<DirStmt> = stmts[label_pos + 1..].to_vec();
                            // Only promote if the tail is a simple return or
                            // a short sequence ending with a return (assignments + return).
                            if is_promotable_guard_tail(&tail) {
                                *changed = true;
                                out.push(DirStmt::If {
                                    cond: cond.clone(),
                                    then_body: tail,
                                    else_body: Vec::new(),
                                });
                                // Emit the main body between the if and the label,
                                // skipping the label and tail.
                                for j in (i + 1)..label_pos {
                                    out.push(stmts[j].clone());
                                }
                                break; // tail was consumed, stop iterating
                            }
                        }
                    }
                }
            }
        }
        out.push(stmts[i].clone());
        i += 1;
    }
    out
}

/// Returns true if the tail is suitable for guard clause inlining.
/// Must be a short sequence (≤8 stmts) ending with a Return.
/// The limit is set to 8 rather than something smaller because
/// structuring runs before normalization, so dead temp cleanups
/// (assignments to variables that are never read) are still present.
fn is_promotable_guard_tail(tail: &[DirStmt]) -> bool {
    if tail.is_empty() || tail.len() > 8 {
        return false;
    }
    // Last statement must be a Return.
    let last = &tail[tail.len() - 1];
    if !matches!(last, DirStmt::Return(_)) {
        return false;
    }
    // All preceding statements must be simple assignments or expressions.
    tail[..tail.len() - 1]
        .iter()
        .all(|s| matches!(s, DirStmt::Assign { .. } | DirStmt::Expr(_)))
}

/// Apply `goto_elim_pass` to fixpoint (convergence when no rule fires).
/// Only operates at the TOP LEVEL of `stmts`; nested scopes are not recursed.
/// Callers that need nested cleanup should call this recursively.
pub fn eliminate_redundant_gotos(mut stmts: Vec<DirStmt>) -> Vec<DirStmt> {
    const MAX_GOTO_ELIM_ITERS: usize = 32;
    for _ in 0..MAX_GOTO_ELIM_ITERS {
        let (new_stmts, changed) = goto_elim_pass(stmts);
        stmts = new_stmts;
        if !changed {
            break;
        }
    }
    stmts
}
