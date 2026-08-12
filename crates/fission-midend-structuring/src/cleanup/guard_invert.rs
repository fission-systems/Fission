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

use fission_midend_prehir::PreHirStmt;
use fission_midend_prehir::util::negate_expr;
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
    let mut removed = 0usize;
    invert_in_place(&mut body, protected, &mut removed);
    (body, removed)
}

fn invert_in_place(stmts: &mut Vec<PreHirStmt>, protected: &HashSet<String>, removed: &mut usize) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let Some(target) = guard_goto_target(&stmts[idx]) {
            if !protected.contains(&target) {
                if let Some(label_idx) = forward_label_index(stmts, idx, &target) {
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
                                removed,
                            );
                        }
                        idx += 1;
                        continue;
                    }
                }
            }
        }
        for seq in child_sequences_mut(&mut stmts[idx]) {
            invert_in_place(seq, protected, removed);
        }
        idx += 1;
    }
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
    let mut total = 0usize;
    for stmt in span {
        if declares_label(stmt) {
            return false;
        }
        total += statement_count(stmt);
        if total > MAX_SPAN_STMTS {
            return false;
        }
    }
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
