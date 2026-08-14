//! Forward guard-goto inversion.
//!
//! Structuring frequently leaves a forward guard in jump form:
//!
//! ```c
//! if (cond) { goto L; }
//! A;
//! B;
//! L:
//! ```
//!
//! When `L` sits in the same statement sequence and nothing between the guard
//! and `L` is a label, the span `A; B;` is exactly "the statements executed
//! when `cond` is false". Inverting the guard expresses that directly:
//!
//! ```c
//! if (!cond) {
//!     A;
//!     B;
//! }
//! ```
//!
//! The rewrite removes one `goto`, and `L` itself is dropped by the ordinary
//! label cleanup when nothing else referenced it.
//!
//! # Why the span may not contain a label
//!
//! A label inside the span would become a label inside the new `if` body, and
//! every `goto` reaching it would then be a jump *into* a conditional block.
//! That changes which statements run and is not expressible in structured
//! form, so a label anywhere in the span (at any nesting depth) disqualifies
//! the candidate.
//!
//! # Why conditions are negated, never operator-inverted
//!
//! `PreHirBinaryOp` does not distinguish integer from floating-point
//! comparisons -- p-code `IntLess` and `FloatLess` both lower to
//! [`PreHirBinaryOp::Lt`] (`midend/support/pcode_util.rs`). Rewriting
//! `!(a < b)` to `a >= b` is valid for integers but wrong for floats, where a
//! NaN operand makes both forms false. This pass therefore always wraps in
//! [`negate_expr`], which is correct for every operand type, and accepts the
//! `!(...)` rendering rather than risking a silent floating-point miscompare.
//!
//! Statements moving into the `if` body keep their meaning: wrapping in a
//! conditional does not introduce a loop, so any `break`/`continue` in the
//! span still binds to the same enclosing loop it did before.

use fission_midend_prehir::{PreHirExpr, PreHirStmt};
use fission_midend_prehir::util::negate_expr;
use crate::HashMap;
use crate::HashSet;

/// Largest span (recursive statement count) this pass will pull into an
/// inverted guard body.
///
/// Bounded so an inversion cannot swallow an arbitrarily large region and
/// re-indent it wholesale; 103 of the 154 candidates measured on the DecBench
/// sample set have spans of ten statements or fewer. Raising this to 400 was
/// measured: NIR improves slightly (-113 vs -109 gotos) but a second HIR file
/// regresses, because a larger span is more likely to be the richer
/// `if (C) goto Lelse; THEN; goto Lend; Lelse: ELSE;` shape that the HIR
/// presentation layer recovers as a full if/else (see
/// `render/presentation/mod.rs`, `if_is_single_goto`). Staying tight leaves
/// those to the owner that handles them better.
pub const MAX_SPAN_STMTS: usize = 24;

/// Rewrite forward `if (cond) { goto L; } SPAN; L:` into `if (!cond) { SPAN }`.
///
/// Returns the rewritten body and the number of `goto` statements removed.
pub fn invert_forward_guard_gotos(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    // Function-wide label reference counts. The rewrites below only ever
    // remove `Goto`s, so a count can go stale *high* but never low, and a
    // stale-high count only makes the single-reference test decline -- the
    // safe direction.
    let global_refs = super::collect_referenced_label_counts(&body);
    let mut definition_counts = HashMap::default();
    super::collect_defined_label_counts_in(&body, &mut definition_counts);
    let mut removed = 0usize;
    invert_in_place(
        &mut body,
        protected,
        &global_refs,
        &definition_counts,
        &[],
        &mut removed,
    );
    (body, removed)
}

/// Labels control reaches by running off the end of `stmts[idx + 1..]`.
///
/// A guard inside a nested body can be inverted against a label that sits
/// *after* the body's container, because falling out of the body arrives there
/// anyway. Without this the pass only ever saw labels in its own statement
/// list, and the commonest remaining forward jump was exactly the shape it
/// could not reach: `if (c) { goto L; } TAIL }` with `L:` in the parent.
fn fallthrough_labels_after(
    stmts: &[PreHirStmt],
    idx: usize,
    inherited: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in &stmts[idx + 1..] {
        match stmt {
            PreHirStmt::Label(l) => out.push(l.clone()),
            _ => return out,
        }
    }
    // Nothing but labels followed, so the enclosing list's own fall-through
    // is reached too.
    out.extend_from_slice(inherited);
    out
}

fn invert_in_place(
    stmts: &mut Vec<PreHirStmt>,
    protected: &HashSet<String>,
    global_refs: &HashMap<String, usize>,
    definition_counts: &HashMap<String, usize>,
    falls_through_to: &[String],
    removed: &mut usize,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let Some(target) = guard_goto_target(&stmts[idx]) {
            if !protected.contains(&target) {
                if let Some(consumed) = fold_guard_chain_into_labeled_if(
                    stmts,
                    idx,
                    &target,
                    global_refs,
                    definition_counts,
                ) {
                    *removed += consumed;
                    if let PreHirStmt::If { then_body, .. } = &mut stmts[idx] {
                        invert_in_place(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                            protected,
                            global_refs,
                            definition_counts,
                            falls_through_to,
                            removed,
                        );
                    }
                    idx += 1;
                    continue;
                }
                // Try the richer if/else shape first: it retires both the
                // guard's jump and the then-arm's jump to the join.
                if recover_if_else(stmts, idx, &target, protected, global_refs) {
                    *removed += 2;
                    if let PreHirStmt::If {
                        then_body,
                        else_body,
                        ..
                    } = &mut stmts[idx]
                    {
                        invert_in_place(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                            protected,
                            global_refs,
                            definition_counts,
                            falls_through_to,
                            removed,
                        );
                        invert_in_place(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                            protected,
                            global_refs,
                            definition_counts,
                            falls_through_to,
                            removed,
                        );
                    }
                    idx += 1;
                    continue;
                }
                let label_idx = forward_label_index(stmts, idx, &target).or_else(|| {
                    // Not in this list, but running off its end arrives there.
                    falls_through_to
                        .iter()
                        .any(|l| *l == target)
                        .then_some(stmts.len())
                });
                if let Some(label_idx) = label_idx {
                    if span_is_invertible(&stmts[idx + 1..label_idx]) {
                        let PreHirStmt::If { cond, .. } = &stmts[idx] else {
                            unreachable!("guard_goto_target matched an If");
                        };
                        let cond = negate_expr(cond.clone());
                        let span: Vec<PreHirStmt> =
                            stmts.drain(idx + 1..label_idx).collect();
                        stmts[idx] = PreHirStmt::If {
                            cond,
                            then_body: std::rc::Rc::new(span),
                            else_body: std::rc::Rc::new(Vec::new()),
                        };
                        *removed += 1;
                        // Recurse into the body just built, then continue past it.
                        if let PreHirStmt::If { then_body, .. } = &mut stmts[idx] {
                            invert_in_place(
                                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                                protected,
                                global_refs,
                                definition_counts,
                                falls_through_to,
                                removed,
                            );
                        }
                        idx += 1;
                        continue;
                    }
                }
            }
        }
        // A loop body falls back to its own head, not past the loop, so only
        // straight-line containers inherit anything.
        let inherited = if matches!(
            stmts[idx],
            PreHirStmt::If { .. } | PreHirStmt::Block(_)
        ) {
            fallthrough_labels_after(stmts, idx, falls_through_to)
        } else {
            Vec::new()
        };
        for seq in child_sequences_mut(&mut stmts[idx]) {
            invert_in_place(
                seq,
                protected,
                global_refs,
                definition_counts,
                &inherited,
                removed,
            );
        }
        idx += 1;
    }
}

fn fold_guard_chain_into_labeled_if(
    stmts: &mut Vec<PreHirStmt>,
    idx: usize,
    target: &str,
    global_refs: &HashMap<String, usize>,
    definition_counts: &HashMap<String, usize>,
) -> Option<usize> {
    if definition_counts.get(target).copied() != Some(1) {
        return None;
    }

    let mut conditions = Vec::new();
    let mut cursor = idx;
    while let Some(PreHirStmt::If {
        cond,
        then_body,
        else_body,
    }) = stmts.get(cursor)
    {
        if !else_body.is_empty() || sole_goto_target(then_body) != Some(target) {
            break;
        }
        conditions.push(cond.clone());
        cursor += 1;
    }
    if conditions.is_empty()
        || global_refs.get(target).copied() != Some(conditions.len())
    {
        return None;
    }

    let Some(PreHirStmt::If {
        cond,
        then_body,
        else_body,
    }) = stmts.get(cursor)
    else {
        return None;
    };
    if !else_body.is_empty()
        || !matches!(then_body.first(), Some(PreHirStmt::Label(label)) if label == target)
    {
        return None;
    }

    conditions.push(cond.clone());
    let combined = conditions
        .into_iter()
        .reduce(|lhs, rhs| PreHirExpr::Binary {
            op: fission_midend_prehir::PreHirBinaryOp::LogicalOr,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: fission_midend_core::ir::NirType::Bool,
        })?;
    let body = then_body.iter().skip(1).cloned().collect::<Vec<_>>();
    let consumed = cursor - idx;
    stmts.splice(
        idx..=cursor,
        std::iter::once(PreHirStmt::If {
            cond: combined,
            then_body: body.into(),
            else_body: Vec::new().into(),
        }),
    );
    Some(consumed)
}

/// `Some(L)` when `stmt` is `if (cond) { goto L; }` with no `else`.
fn guard_goto_target(stmt: &PreHirStmt) -> Option<String> {
    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() {
        return None;
    }
    sole_goto_target(then_body).map(str::to_string)
}

/// The single `Goto` target of a body, looking through a lone nested `Block`.
fn sole_goto_target(body: &[PreHirStmt]) -> Option<&str> {
    match body {
        [PreHirStmt::Goto(target)] => Some(target.as_str()),
        [PreHirStmt::Block(inner)] => sole_goto_target(inner),
        _ => None,
    }
}

/// Index of `Label(target)` after `from` in the same sequence, when the span
/// between them is non-empty.
fn forward_label_index(stmts: &[PreHirStmt], from: usize, target: &str) -> Option<usize> {
    stmts
        .iter()
        .enumerate()
        .skip(from + 2)
        .find(|(_, stmt)| matches!(stmt, PreHirStmt::Label(label) if label == target))
        .map(|(idx, _)| idx)
}

/// A span may be pulled into a conditional only when it declares no label
/// anywhere and stays inside the size bound.
fn span_is_invertible(span: &[PreHirStmt]) -> bool {
    span.iter().all(|stmt| !declares_label(stmt)) && span_within_bound(span)
}

fn span_within_bound(span: &[PreHirStmt]) -> bool {
    let mut total = 0usize;
    for stmt in span {
        total += statement_count(stmt);
        if total > MAX_SPAN_STMTS {
            return false;
        }
    }
    true
}

/// Recover a full if/else from the two-jump shape:
///
/// ```text
/// if (C) { goto Lelse; }   THEN...;  goto Lend;   Lelse:  ELSE...;  Lend:
/// ```
///
/// becomes
///
/// ```text
/// if (C) { ELSE } else { THEN }   Lend:
/// ```
///
/// Both jumps retire -- the guard's, and the then-arm's jump to the join --
/// so this is preferred over the plain inversion, which only retires one.
/// `Lelse` disappears with its single reference; `Lend` is left in place
/// because other predecessors may still target it.
///
/// This mirrors the recovery the HIR presentation layer already performs
/// (`render/presentation/mod.rs`, `if_is_single_goto`). Doing it here means
/// NIR -- which has no such layer -- gets the same shape, and the two owners
/// agree instead of the earlier PreHIR pass pre-empting the better HIR one
/// with a weaker single-jump inversion.
fn recover_if_else(
    stmts: &mut Vec<PreHirStmt>,
    idx: usize,
    lelse: &str,
    protected: &HashSet<String>,
    global_refs: &HashMap<String, usize>,
) -> bool {
    // The guard's jump must be this label's only reference; otherwise removing
    // the label would strand the other predecessors that target it.
    if global_refs.get(lelse).copied() != Some(1) {
        return false;
    }
    let Some(le_idx) = forward_label_index(stmts, idx, lelse) else {
        return false;
    };
    let then_span = &stmts[idx + 1..le_idx];
    // The then-arm must hand control to a join rather than falling into ELSE.
    let Some(PreHirStmt::Goto(lend)) = then_span.last() else {
        return false;
    };
    let lend = lend.clone();
    if lend == lelse || protected.contains(&lend) {
        return false;
    }
    if !span_is_invertible(then_span) {
        return false;
    }
    let Some(lend_idx) = stmts
        .iter()
        .enumerate()
        .skip(le_idx + 1)
        .find(|(_, stmt)| matches!(stmt, PreHirStmt::Label(label) if *label == lend))
        .map(|(i, _)| i)
    else {
        return false;
    };
    let else_span = &stmts[le_idx + 1..lend_idx];
    if !span_is_invertible(else_span) {
        return false;
    }

    let PreHirStmt::If { cond, .. } = &stmts[idx] else {
        return false;
    };
    let cond = cond.clone();
    // `goto Lelse` when C means: when C run ELSE, otherwise run THEN.
    let recovered_then: Vec<PreHirStmt> = else_span.to_vec();
    let mut recovered_else: Vec<PreHirStmt> = then_span.to_vec();
    recovered_else.pop();

    stmts.splice(
        idx..lend_idx,
        std::iter::once(PreHirStmt::If {
            cond,
            then_body: std::rc::Rc::new(recovered_then),
            else_body: std::rc::Rc::new(recovered_else),
        }),
    );
    true
}

fn declares_label(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Label(_) => true,
        _ => child_sequences(stmt)
            .into_iter()
            .any(|seq| seq.iter().any(declares_label)),
    }
}

fn statement_count(stmt: &PreHirStmt) -> usize {
    1 + child_sequences(stmt)
        .into_iter()
        .map(|seq| seq.iter().map(statement_count).sum::<usize>())
        .sum::<usize>()
}

fn child_sequences(stmt: &PreHirStmt) -> Vec<&[PreHirStmt]> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => vec![body.as_slice()],
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![then_body.as_slice(), else_body.as_slice()],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&[PreHirStmt]> =
                cases.iter().map(|case| case.body.as_slice()).collect();
            out.push(default.as_slice());
            out
        }
        _ => Vec::new(),
    }
}

fn child_sequences_mut(stmt: &mut PreHirStmt) -> Vec<&mut Vec<PreHirStmt>> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            vec![std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body)]
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
        ],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&mut Vec<PreHirStmt>> = Vec::new();
            for case in cases.iter_mut() {
                out.push(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            out.push(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            out
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::{PreHirExpr, PreHirUnaryOp};

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }
    fn expr_stmt(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(var(name))
    }
    fn guard(target: &str) -> PreHirStmt {
        PreHirStmt::If {
            cond: var("c"),
            then_body: vec![PreHirStmt::Goto(target.into())].into(),
            else_body: Vec::new().into(),
        }
    }

    fn labeled_if(target: &str) -> PreHirStmt {
        PreHirStmt::If {
            cond: var("tail_cond"),
            then_body: vec![
                PreHirStmt::Label(target.into()),
                expr_stmt("body"),
            ]
            .into(),
            else_body: Vec::new().into(),
        }
    }

    #[test]
    fn folds_guard_into_labeled_if_as_short_circuit_or() {
        let (out, removed) = invert_forward_guard_gotos(
            vec![guard("L"), labeled_if("L")],
            &HashSet::default(),
        );
        assert_eq!(removed, 1);
        assert_eq!(out.len(), 1);
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &out[0]
        else {
            panic!("expected folded if");
        };
        assert!(matches!(
            cond,
            PreHirExpr::Binary {
                op: fission_midend_prehir::PreHirBinaryOp::LogicalOr,
                ..
            }
        ));
        assert_eq!(then_body.as_slice(), &[expr_stmt("body")]);
        assert!(else_body.is_empty());
    }

    #[test]
    fn folds_multiple_same_target_guards_in_order() {
        let (out, removed) = invert_forward_guard_gotos(
            vec![guard("L"), guard("L"), labeled_if("L")],
            &HashSet::default(),
        );
        assert_eq!(removed, 2);
        assert_eq!(out.len(), 1);
        assert!(!declares_label(&out[0]));
    }

    #[test]
    fn labeled_if_fold_refuses_external_reference_and_protected_label() {
        let external = vec![
            guard("L"),
            labeled_if("L"),
            PreHirStmt::If {
                cond: var("external"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
        ];
        let (out, _) = invert_forward_guard_gotos(external, &HashSet::default());
        assert!(out.iter().any(|stmt| declares_label(stmt)));

        let protected: HashSet<String> = ["L".to_string()].into_iter().collect();
        let body = vec![guard("L"), labeled_if("L")];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &protected);
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn labeled_if_fold_requires_label_at_arm_entry() {
        let labeled = PreHirStmt::If {
            cond: var("tail_cond"),
            then_body: vec![expr_stmt("prefix"), PreHirStmt::Label("L".into())].into(),
            else_body: Vec::new().into(),
        };
        let body = vec![guard("L"), labeled];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn inverts_forward_guard_and_absorbs_span() {
        let body = vec![
            guard("L"),
            expr_stmt("a"),
            expr_stmt("b"),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(None),
        ];
        let (out, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 1);
        assert_eq!(out.len(), 3);
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &out[0]
        else {
            panic!("expected inverted guard, got {:?}", out[0]);
        };
        // Negated, never operator-inverted -- see module docs (float NaN).
        assert!(matches!(
            cond,
            PreHirExpr::Unary {
                op: PreHirUnaryOp::Not,
                ..
            }
        ));
        assert_eq!(then_body.as_slice(), &[expr_stmt("a"), expr_stmt("b")]);
        assert!(else_body.is_empty());
        assert_eq!(out[1], PreHirStmt::Label("L".into()));
    }

    #[test]
    fn double_negation_collapses() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(var("rax")),
                    ty: fission_midend_core::ir::NirType::Bool,
                },
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            expr_stmt("a"),
            PreHirStmt::Label("L".into()),
        ];
        let (out, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 1);
        let PreHirStmt::If { cond, .. } = &out[0] else {
            panic!("expected if");
        };
        assert_eq!(cond, &var("rax"), "!(!rax) must collapse to rax");
    }

    #[test]
    fn refuses_span_containing_a_label() {
        // `goto M` from elsewhere would become a jump into the new if body.
        let body = vec![
            guard("L"),
            expr_stmt("a"),
            PreHirStmt::Label("M".into()),
            expr_stmt("b"),
            PreHirStmt::Label("L".into()),
        ];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_nested_label_in_span() {
        let body = vec![
            guard("L"),
            PreHirStmt::Block(vec![PreHirStmt::Label("M".into())].into()),
            PreHirStmt::Label("L".into()),
        ];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_backward_target() {
        let body = vec![
            PreHirStmt::Label("L".into()),
            expr_stmt("a"),
            guard("L"),
        ];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_empty_span() {
        // `if (c) goto L; L:` is the redundant-goto pass's job, not this one.
        let body = vec![guard("L"), PreHirStmt::Label("L".into())];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_guard_with_else() {
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: vec![expr_stmt("e")].into(),
            },
            expr_stmt("a"),
            PreHirStmt::Label("L".into()),
        ];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_protected_target() {
        let protected: HashSet<String> = ["L".to_string()].into_iter().collect();
        let body = vec![guard("L"), expr_stmt("a"), PreHirStmt::Label("L".into())];
        let (out, removed) = invert_forward_guard_gotos(body.clone(), &protected);
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn respects_span_bound() {
        let mut body = vec![guard("L")];
        for i in 0..(MAX_SPAN_STMTS + 1) {
            body.push(expr_stmt(&format!("s{i}")));
        }
        body.push(PreHirStmt::Label("L".into()));
        let (_, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 0, "over-long span must not be absorbed");
    }

    #[test]
    fn recovers_full_if_else_and_retires_both_jumps() {
        // if (c) goto Lelse; THEN; goto Lend; Lelse: ELSE; Lend: rest
        let body = vec![
            guard("Lelse"),
            expr_stmt("then1"),
            PreHirStmt::Goto("Lend".into()),
            PreHirStmt::Label("Lelse".into()),
            expr_stmt("else1"),
            PreHirStmt::Label("Lend".into()),
            PreHirStmt::Return(None),
        ];
        let (out, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 2, "guard jump and join jump both retire");
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &out[0]
        else {
            panic!("expected recovered if/else, got {:?}", out[0]);
        };
        // Condition is kept as-is here; `goto Lelse` when c means "when c, ELSE".
        assert_eq!(cond, &var("c"));
        assert_eq!(then_body.as_slice(), &[expr_stmt("else1")]);
        assert_eq!(else_body.as_slice(), &[expr_stmt("then1")]);
        // Lelse is gone with its only reference; Lend stays for other callers.
        assert!(!out.contains(&PreHirStmt::Label("Lelse".into())));
        assert_eq!(out[1], PreHirStmt::Label("Lend".into()));
    }

    #[test]
    fn if_else_recovery_needs_single_reference_label() {
        // A second `goto Lelse` means removing the label would strand it.
        let body = vec![
            guard("Lelse"),
            expr_stmt("then1"),
            PreHirStmt::Goto("Lend".into()),
            PreHirStmt::Label("Lelse".into()),
            expr_stmt("else1"),
            PreHirStmt::Label("Lend".into()),
            PreHirStmt::If {
                cond: var("d"),
                then_body: vec![PreHirStmt::Goto("Lelse".into())].into(),
                else_body: Vec::new().into(),
            },
        ];
        let (out, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        // Falls back to the plain single-jump inversion, never the if/else form.
        assert_ne!(removed, 2);
        assert!(out.iter().any(|s| matches!(s, PreHirStmt::Label(l) if l == "Lelse")));
    }

    #[test]
    fn if_else_recovery_refuses_label_in_else_span() {
        let body = vec![
            guard("Lelse"),
            expr_stmt("then1"),
            PreHirStmt::Goto("Lend".into()),
            PreHirStmt::Label("Lelse".into()),
            PreHirStmt::Label("Inner".into()),
            expr_stmt("else1"),
            PreHirStmt::Label("Lend".into()),
        ];
        let (_, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_ne!(removed, 2, "label inside ELSE must block if/else recovery");
    }

    #[test]
    fn break_in_span_keeps_its_loop_binding() {
        // Wrapping in an `if` does not introduce a loop, so the `break` still
        // belongs to the enclosing `while`.
        let body = vec![PreHirStmt::While {
            cond: var("w"),
            body: vec![
                guard("L"),
                expr_stmt("a"),
                PreHirStmt::Break,
                PreHirStmt::Label("L".into()),
            ]
            .into(),
        }];
        let (out, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 1);
        let PreHirStmt::While { body, .. } = &out[0] else {
            panic!("expected while");
        };
        let PreHirStmt::If { then_body, .. } = &body[0] else {
            panic!("expected inverted guard inside loop");
        };
        assert_eq!(then_body.as_slice(), &[expr_stmt("a"), PreHirStmt::Break]);
    }
}
