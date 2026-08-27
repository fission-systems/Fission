use crate::HashMap;
use crate::HashSet;
use fission_midend_core::ir::*;
use fission_midend_prehir::ir::*;
use fission_midend_prehir::util::{collect_referenced_label_counts, negate_expr};

// ---------------------------------------------------------------------------
// Goto elimination post-pass
// ---------------------------------------------------------------------------
//
// Five fixpoint rules applied in sequence until convergence:
//
//  1. Nested fallthrough removal: a trailing `Goto(L)` in a sequential Block/If arm whose
//     enclosing successor is `Label(L)` → remove the Goto.
//  2. Empty-jump removal:   `Goto(L)` immediately followed by `Label(L)` → remove the Goto.
//  3. Single-reference inline: `Label(L)` referenced exactly once as `Goto(L)` → remove the
//     Label and the Goto (they're already adjacent after rule 1 or after inlining).
//  4. Conditional goto inversion: `if (cond) { Goto(L) }` directly followed by `Label(L)`
//     and the rest of the code → replace with `if (!cond) { rest_code }`.
//  5. Guard clause promotion: `if (cond) { Goto(L) }; code; L: return val` →
//     `if (cond) { return val }; code`.  Handles the extremely common early-exit guard
//     pattern where a forward goto jumps over the main body to a trailing return.
//
// Only rule 1 recurses, and it propagates an enclosing successor through sequential Block/If
// scopes only. The other rules remain top-level. After each changed pass, the whole pass
// restarts to reach a fixpoint.

/// Apply the goto-elimination rules once to a statement list.
/// Returns `(cleaned, changed)` where `changed` indicates whether any rule fired.
fn goto_elim_pass(stmts: Vec<PreHirStmt>) -> (Vec<PreHirStmt>, bool) {
    let mut changed = false;
    let stmts = strip_unreachable_after_unconditional_transfer(stmts, &mut changed);
    let stmts = empty_jump_removal(stmts, &mut changed);
    let stmts = single_ref_label_inline(stmts, &mut changed);
    let stmts = guard_clause_promotion(stmts, &mut changed);
    let stmts = cond_goto_inversion(stmts, &mut changed);
    // Preserve the established top-level rewrites above when they apply; this
    // recursive rule is the fallback for sequential nesting they cannot see.
    let stmts = nested_fallthrough_removal(stmts, None, &mut changed);
    (stmts, changed)
}

/// Remove a goto whose target is the next label reached by ordinary sequential
/// fallthrough, including when the goto is nested inside a `Block` or either
/// arm of an `If`.
///
/// An enclosing successor is deliberately not propagated through loop or
/// switch boundaries: falling off a loop body begins another iteration, and a
/// switch case has case/break ownership that cannot be inferred from lexical
/// nesting alone. Local same-list adjacency is still cleaned inside those
/// bodies because it needs no enclosing-flow assumption.
fn nested_fallthrough_removal(
    stmts: Vec<PreHirStmt>,
    enclosing_successor: Option<&str>,
    changed: &mut bool,
) -> Vec<PreHirStmt> {
    let mut out = Vec::with_capacity(stmts.len());

    for idx in 0..stmts.len() {
        let successor = match stmts.get(idx + 1) {
            Some(PreHirStmt::Label(label)) => Some(label.clone()),
            Some(_) => None,
            None => enclosing_successor.map(str::to_owned),
        };

        let rewritten = match stmts[idx].clone() {
            PreHirStmt::Goto(label) if successor.as_deref() == Some(label.as_str()) => {
                *changed = true;
                continue;
            }
            PreHirStmt::Block(body) => PreHirStmt::Block(std::rc::Rc::new(
                nested_fallthrough_removal(body.as_ref().clone(), successor.as_deref(), changed),
            )),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(nested_fallthrough_removal(
                    then_body.as_ref().clone(),
                    successor.as_deref(),
                    changed,
                )),
                else_body: std::rc::Rc::new(nested_fallthrough_removal(
                    else_body.as_ref().clone(),
                    successor.as_deref(),
                    changed,
                )),
            },
            PreHirStmt::While { cond, body } => PreHirStmt::While {
                cond,
                body: std::rc::Rc::new(nested_fallthrough_removal(
                    body.as_ref().clone(),
                    None,
                    changed,
                )),
            },
            PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
                body: std::rc::Rc::new(nested_fallthrough_removal(
                    body.as_ref().clone(),
                    None,
                    changed,
                )),
                cond,
            },
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => PreHirStmt::For {
                init,
                cond,
                update,
                body: std::rc::Rc::new(nested_fallthrough_removal(
                    body.as_ref().clone(),
                    None,
                    changed,
                )),
            },
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => PreHirStmt::Switch {
                expr,
                cases: cases
                    .into_iter()
                    .map(|case| PreHirSwitchCase {
                        values: case.values,
                        body: std::rc::Rc::new(nested_fallthrough_removal(
                            case.body.as_ref().clone(),
                            None,
                            changed,
                        )),
                    })
                    .collect(),
                default: std::rc::Rc::new(nested_fallthrough_removal(
                    default.as_ref().clone(),
                    None,
                    changed,
                )),
            },
            stmt => stmt,
        };
        out.push(rewritten);
    }

    out
}

fn strip_unreachable_after_unconditional_transfer(
    stmts: Vec<PreHirStmt>,
    changed: &mut bool,
) -> Vec<PreHirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut dropping = false;
    for (idx, stmt) in stmts.iter().cloned().enumerate() {
        if dropping {
            if matches!(stmt, PreHirStmt::Label(_)) {
                dropping = false;
                out.push(stmt);
            } else {
                *changed = true;
            }
            continue;
        }

        dropping = match &stmt {
            PreHirStmt::Goto(label) => stmts[idx + 1..]
                .iter()
                .any(|candidate| matches!(candidate, PreHirStmt::Label(next) if next == label)),
            _ => false,
        };
        out.push(stmt);
    }
    out
}

/// Rule 1: If a `Goto(L)` is immediately followed by `Label(L)`, remove the Goto.
fn empty_jump_removal(stmts: Vec<PreHirStmt>, changed: &mut bool) -> Vec<PreHirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        if let PreHirStmt::Goto(ref label) = stmt {
            if let Some(PreHirStmt::Label(next_label)) = iter.peek() {
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
fn single_ref_label_inline(stmts: Vec<PreHirStmt>, changed: &mut bool) -> Vec<PreHirStmt> {
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
        if let PreHirStmt::Goto(ref label) = stmt {
            if singleton_labels.contains(label.as_str()) {
                if let Some(PreHirStmt::Label(next_label)) = iter.peek() {
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
fn cond_goto_inversion(stmts: Vec<PreHirStmt>, changed: &mut bool) -> Vec<PreHirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        // Pattern: If { cond, then=[Goto(L)], else=[] }  followed by  Label(L)  and rest
        if let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if else_body.is_empty() {
                if let [PreHirStmt::Goto(goto_label)] = then_body.as_slice() {
                    // Find the immediately following Label(L) at the top level.
                    if i + 1 < stmts.len() {
                        if let PreHirStmt::Label(label) = &stmts[i + 1] {
                            if goto_label == label {
                                // Collect everything after the label as the inlined else body.
                                let inverted_cond = negate_expr(cond.clone());
                                let rest_body: Vec<PreHirStmt> = stmts[i + 2..].to_vec();
                                if !rest_body.is_empty() {
                                    *changed = true;
                                    out.push(PreHirStmt::If {
                                        cond: inverted_cond,
                                        then_body: std::rc::Rc::new(rest_body),
                                        else_body: std::rc::Rc::new(Vec::new()),
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
fn guard_clause_promotion(stmts: Vec<PreHirStmt>, changed: &mut bool) -> Vec<PreHirStmt> {
    let ref_counts = collect_referenced_label_counts(&stmts);
    let mut out = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        // Look for: if (cond) { Goto(L) } where L is referenced exactly once.
        if let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if else_body.is_empty() {
                if let [PreHirStmt::Goto(goto_label)] = then_body.as_slice() {
                    if ref_counts.get(goto_label).copied() == Some(1) {
                        // Scan forward for `Label(L)` at the top level.
                        if let Some(label_pos) = (i + 1..stmts.len())
                            .find(|&j| matches!(&stmts[j], PreHirStmt::Label(l) if l == goto_label))
                        {
                            // Collect the tail after the label.
                            let tail: Vec<PreHirStmt> = stmts[label_pos + 1..].to_vec();
                            // `ref_counts == 1` only proves no *textual* Goto/Label
                            // reference besides ours reaches `L` -- it can't see an
                            // *implicit* one: a `while (1) { ... break ... }` inside
                            // `code` (stmts[i+1..label_pos]) falls through to exactly
                            // this position on `break`, with no goto/label naming `L`
                            // anywhere. Promoting anyway would silently delete that
                            // fallthrough's only remaining route to the tail (see the
                            // `kv_lookup` "read xVar10 uninitialized" class of bug this
                            // guards against). Require `code` to be provably unable to
                            // fall through past its own end before consuming the tail.
                            let code = &stmts[(i + 1)..label_pos];
                            // Only promote if the tail is a simple return or
                            // a short sequence ending with a return (assignments + return).
                            if !stmts_may_fall_through(code) && is_promotable_guard_tail(&tail) {
                                *changed = true;
                                out.push(PreHirStmt::If {
                                    cond: cond.clone(),
                                    then_body: std::rc::Rc::new(tail),
                                    else_body: std::rc::Rc::new(Vec::new()),
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

/// Conservative "can control reach past the end of `stmts`" check. Used to
/// verify `code` (the statements between a promoted `if (cond) goto L` and
/// `L` itself) cannot ALSO reach `L` by falling off its own end -- the only
/// case `guard_clause_promotion`'s `ref_counts[L] == 1` textual check can't
/// see, since a loop's own `break` reaches the position right after the
/// loop with no `Goto`/`Label` naming it at all.
///
/// Errs toward `true` (assume it may fall through, so promotion is
/// declined) for any shape not proven otherwise -- a missed optimization is
/// far cheaper than silently deleting a live return path.
fn stmts_may_fall_through(stmts: &[PreHirStmt]) -> bool {
    match stmts.last() {
        None => false,
        Some(stmt) => stmt_may_fall_through(stmt),
    }
}

fn stmt_may_fall_through(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {
            false
        }
        PreHirStmt::Block(body) => stmts_may_fall_through(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            else_body.is_empty()
                || stmts_may_fall_through(then_body)
                || stmts_may_fall_through(else_body)
        }
        PreHirStmt::While {
            cond: PreHirExpr::Const(v, _),
            body,
        } if *v != 0 => loop_body_has_reachable_break(body),
        // Anything else (a non-infinite `While`, `DoWhile`, `For`, `Switch`,
        // or a plain statement) is conservatively assumed fall-through.
        _ => true,
    }
}

/// `true` if `stmts` contains a `Break` reachable from this loop's own body
/// -- i.e. not shadowed by a nested loop or `Switch`, which would own that
/// `Break` instead.
fn loop_body_has_reachable_break(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        PreHirStmt::Break => true,
        PreHirStmt::Block(body) => loop_body_has_reachable_break(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => loop_body_has_reachable_break(then_body) || loop_body_has_reachable_break(else_body),
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => false,
        PreHirStmt::Switch { .. } => false,
        _ => false,
    })
}

/// Returns true if the tail is suitable for guard clause inlining.
/// Must be a short sequence (≤8 stmts) ending with a Return.
/// The limit is set to 8 rather than something smaller because
/// structuring runs before normalization, so dead temp cleanups
/// (assignments to variables that are never read) are still present.
fn is_promotable_guard_tail(tail: &[PreHirStmt]) -> bool {
    if tail.is_empty() || tail.len() > 8 {
        return false;
    }
    // Last statement must be a Return.
    let last = &tail[tail.len() - 1];
    if !matches!(last, PreHirStmt::Return(_)) {
        return false;
    }
    // All preceding statements must be simple assignments or expressions.
    tail[..tail.len() - 1]
        .iter()
        .all(|s| matches!(s, PreHirStmt::Assign { .. } | PreHirStmt::Expr(_)))
}

/// Apply `goto_elim_pass` to fixpoint (convergence when no rule fires).
/// Structural rules other than nested fallthrough removal operate only at the
/// top level of `stmts`.
pub fn eliminate_redundant_gotos(mut stmts: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
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
