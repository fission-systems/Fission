//! Sink the statements after an `if` into it, so its trailing guard can stop
//! jumping over them.
//!
//! Measured on the corpus, the commonest place a surviving jump lives is a
//! guard at the end of an `if`'s then-arm whose label sits further out:
//!
//! ```text
//!   if (c) { ...; if (b) { goto L; } }        if (c) { ...; if (!b) { SPAN } }
//!   SPAN                               ==>    else   { SPAN }
//! L:                                        L:
//! ```
//!
//! # Why the copy is necessary
//!
//! Control leaves an `if` two ways: off the end of the arm, and by failing the
//! condition. The original reaches `SPAN` on both. Moving `SPAN` into the arm
//! covers only the first, so the else-arm gets it too. That is a real
//! duplication and the reason this is bounded rather than applied wherever it
//! matches.
//!
//! # Why it bounds itself
//!
//! `structuring_quality` judges *driver candidates*; this runs in the cleanup
//! chain, on the body that already won. Nothing downstream will weigh the text
//! this adds against the jump it removes, so the ceiling lives here.

use super::{collect_defined_label_counts_in, collect_referenced_label_counts};
use crate::HashMap;
use crate::HashSet;
use fission_midend_prehir::PreHirStmt;
use fission_midend_prehir::util::negate_expr;

/// Statements a span may carry and still be worth writing twice to retire one
/// jump.
///
/// `cleanup::tail_dup` allows six for the same trade against the same
/// currency, and this copy is made once, so six it is. A jump is not worth
/// much more text than that.
const MAX_DUPLICATED_SPAN_STMTS: usize = 6;

/// Sink the span between an `if` and a label into that `if`.
pub fn sink_spans_into_if_arms(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let refs = collect_referenced_label_counts(&body);
    let mut definitions = HashMap::default();
    collect_defined_label_counts_in(&body, &mut definitions);
    let mut sunk = 0usize;
    sink_in_place(&mut body, protected, &refs, &definitions, &mut sunk);
    (body, sunk)
}

fn sink_in_place(
    stmts: &mut Vec<PreHirStmt>,
    protected: &HashSet<String>,
    refs: &HashMap<String, usize>,
    definitions: &HashMap<String, usize>,
    sunk: &mut usize,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let Some(target) = trailing_then_guard(&stmts[idx]) {
            if !protected.contains(&target)
                && definitions.get(&target).copied() == Some(1)
                && refs.contains_key(&target)
            {
                if let Some(label_idx) = forward_label_index(stmts, idx, &target) {
                    if span_is_duplicable(&stmts[idx + 1..label_idx]) {
                        let span: Vec<PreHirStmt> = stmts.drain(idx + 1..label_idx).collect();
                        sink_into(&mut stmts[idx], span);
                        *sunk += 1;
                    }
                }
            }
        }
        for seq in child_sequences_mut(&mut stmts[idx]) {
            sink_in_place(seq, protected, refs, definitions, sunk);
        }
        idx += 1;
    }
}

/// The label an `if`'s then-arm ends by jumping to.
fn trailing_then_guard(stmt: &PreHirStmt) -> Option<String> {
    let PreHirStmt::If { then_body, .. } = stmt else {
        return None;
    };
    let PreHirStmt::If {
        then_body: inner,
        else_body: inner_else,
        ..
    } = then_body.last()?
    else {
        return None;
    };
    if !inner_else.is_empty() {
        return None;
    }
    match inner.as_slice() {
        [PreHirStmt::Goto(target)] => Some(target.clone()),
        _ => None,
    }
}

/// Invert the trailing guard over `span`, and give the else-arm its own copy.
fn sink_into(stmt: &mut PreHirStmt, span: Vec<PreHirStmt>) {
    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmt
    else {
        return;
    };
    let then_mut = std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body);
    let last = then_mut.len() - 1;
    let PreHirStmt::If { cond, .. } = &then_mut[last] else {
        return;
    };
    then_mut[last] = PreHirStmt::If {
        cond: negate_expr(cond.clone()),
        then_body: std::rc::Rc::new(span.clone()),
        else_body: std::rc::Rc::new(Vec::new()),
    };
    // Failing the outer condition also used to reach the span.
    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body).extend(span);
}

fn forward_label_index(stmts: &[PreHirStmt], from: usize, target: &str) -> Option<usize> {
    stmts
        .iter()
        .enumerate()
        .skip(from + 1)
        .find(|(_, s)| matches!(s, PreHirStmt::Label(l) if l == target))
        .map(|(i, _)| i)
}

/// Whether a span can be written twice without changing what it means.
///
/// No labels: it is about to exist in two places, and a jump to it could only
/// reach one. No `Break`/`Continue`: the copies land at different loop depths.
fn span_is_duplicable(span: &[PreHirStmt]) -> bool {
    let mut total = 0usize;
    for stmt in span {
        if !no_rebinding_control(stmt) {
            return false;
        }
        total += statement_count(stmt);
        if total > MAX_DUPLICATED_SPAN_STMTS {
            return false;
        }
    }
    !span.is_empty()
}

fn no_rebinding_control(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Label(_) | PreHirStmt::Break | PreHirStmt::Continue => false,
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().all(no_rebinding_control) && else_body.iter().all(no_rebinding_control)
        }
        PreHirStmt::Block(inner) => inner.iter().all(no_rebinding_control),
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body.iter().all(no_label),
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().all(|c| c.body.iter().all(no_label)) && default.iter().all(no_label)
        }
        _ => true,
    }
}

fn no_label(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Label(_) => false,
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => then_body.iter().all(no_label) && else_body.iter().all(no_label),
        PreHirStmt::Block(inner) => inner.iter().all(no_label),
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body.iter().all(no_label),
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().all(|c| c.body.iter().all(no_label)) && default.iter().all(no_label)
        }
        _ => true,
    }
}

fn statement_count(stmt: &PreHirStmt) -> usize {
    1 + child_sequences(stmt)
        .into_iter()
        .flat_map(|seq| seq.iter())
        .map(statement_count)
        .sum::<usize>()
}

fn child_sequences(stmt: &PreHirStmt) -> Vec<&Vec<PreHirStmt>> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => vec![body],
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![then_body, else_body],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&Vec<PreHirStmt>> = cases.iter().map(|c| &*c.body).collect();
            out.push(default);
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
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirExpr, PreHirLValue};
    use std::rc::Rc;

    fn var(n: &str) -> PreHirExpr {
        PreHirExpr::Var(n.to_string())
    }
    fn assign(name: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: PreHirExpr::Const(1, NirType::Bool),
        }
    }
    fn guard(label: &str) -> PreHirStmt {
        PreHirStmt::If {
            cond: var("b"),
            then_body: Rc::new(vec![PreHirStmt::Goto(label.to_string())]),
            else_body: Rc::new(Vec::new()),
        }
    }
    fn outer(then: Vec<PreHirStmt>, els: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: var("c"),
            then_body: Rc::new(then),
            else_body: Rc::new(els),
        }
    }

    #[test]
    fn both_ways_out_of_the_if_keep_the_span() {
        let body = vec![
            outer(vec![assign("inside"), guard("L")], Vec::new()),
            assign("span"),
            PreHirStmt::Label("L".into()),
        ];
        let (out, n) = sink_spans_into_if_arms(body, &HashSet::default());
        assert_eq!(n, 1);
        let [
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            },
            PreHirStmt::Label(_),
        ] = &out[..]
        else {
            panic!("expected the if then the label, got {out:#?}");
        };
        // Falling off the arm: the guard is inverted over the span.
        let [
            _,
            PreHirStmt::If {
                cond,
                then_body: inner,
                ..
            },
        ] = then_body.as_slice()
        else {
            panic!("expected the guard rewritten, got {then_body:#?}");
        };
        assert_eq!(cond, &negate_expr(var("b")));
        assert_eq!(inner.len(), 1);
        // Failing the condition: the else-arm got its own copy.
        assert_eq!(else_body.len(), 1, "the else path still reaches the span");
    }

    #[test]
    fn an_existing_else_arm_keeps_its_own_statements() {
        let body = vec![
            outer(vec![guard("L")], vec![assign("existing")]),
            assign("span"),
            PreHirStmt::Label("L".into()),
        ];
        let (out, n) = sink_spans_into_if_arms(body, &HashSet::default());
        assert_eq!(n, 1);
        let [PreHirStmt::If { else_body, .. }, _] = &out[..] else {
            panic!("expected the if then the label");
        };
        assert_eq!(else_body.len(), 2, "existing statement then the span copy");
    }

    #[test]
    fn a_span_too_large_to_write_twice_is_refused() {
        let mut span: Vec<PreHirStmt> = (0..MAX_DUPLICATED_SPAN_STMTS + 1)
            .map(|i| assign(&format!("s{i}")))
            .collect();
        let mut body = vec![outer(vec![guard("L")], Vec::new())];
        body.append(&mut span);
        body.push(PreHirStmt::Label("L".into()));
        let (_, n) = sink_spans_into_if_arms(body, &HashSet::default());
        assert_eq!(n, 0);
    }

    #[test]
    fn a_label_in_the_span_is_refused() {
        // Copied, it would exist twice and a jump could reach only one.
        let body = vec![
            outer(vec![guard("L")], Vec::new()),
            PreHirStmt::Label("inner".into()),
            assign("span"),
            PreHirStmt::Label("L".into()),
        ];
        let (_, n) = sink_spans_into_if_arms(body, &HashSet::default());
        assert_eq!(n, 0);
    }

    #[test]
    fn a_break_in_the_span_is_refused() {
        // The copies land at different loop depths.
        let body = vec![
            outer(vec![guard("L")], Vec::new()),
            PreHirStmt::Break,
            PreHirStmt::Label("L".into()),
        ];
        let (_, n) = sink_spans_into_if_arms(body, &HashSet::default());
        assert_eq!(n, 0);
    }
}
