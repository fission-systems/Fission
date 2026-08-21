//! Dropping statements that control cannot reach.
//!
//! A structured region whose every path returns is still followed by the
//! function's exit block, so a body that has already returned on both arms of
//! its final `if` gets one more `return` after it:
//!
//! ```text
//! if (cond) { return 0; } else { return 1; }
//! return xVar20;                 // <- reachable by nothing
//! ```
//!
//! The variable it names is never assigned, which is what made this visible:
//! the emitted C returns an indeterminate value on a path that does not exist.
//!
//! It is also the single largest source of structural distance from source
//! measured so far. In a CFG that statement is a node with no predecessor, and
//! DecBench's graph edit distance charges `1 + in-degree + out-degree` to
//! delete a node -- so an isolated one costs exactly 1. Measured over 40
//! functions of `base-passwd/update-passwd` scored against their published
//! source CFGs: 12 matched source exactly, and **9 more were wrong by this and
//! nothing else** -- every one of them one node and zero edges away. Removing
//! it takes that binary from 30% exact to 52%.

use fission_midend_prehir::PreHirStmt;

use crate::HashMap;

/// Whether control can continue past `stmt` to the next statement.
///
/// Conservative in the only direction that matters: anything not proven to
/// leave is assumed to fall through, so a body is never truncated on a guess.
/// `Break` and `Continue` do leave, but they leave *to* somewhere inside an
/// enclosing construct, and this only ever trims a tail within the same
/// statement list, which is exactly where their target is not.
fn diverges(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => {
            true
        }
        PreHirStmt::Block(body) => body.iter().any(diverges),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            // Both arms must leave, and an absent arm is a fall-through path.
            !then_body.is_empty()
                && !else_body.is_empty()
                && then_body.iter().any(diverges)
                && else_body.iter().any(diverges)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            // A switch without a default has a path that matches no case.
            !default.is_empty()
                && default.iter().any(diverges)
                && cases.iter().all(|case| case.body.iter().any(diverges))
        }
        // A loop may exit by its condition, and `While`/`For` may not run at
        // all. `DoWhile` runs once but can still leave by its condition. None
        // of them are proven to diverge without reasoning about the condition,
        // which this deliberately does not do.
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => false,
        _ => false,
    }
}

/// Labels defined anywhere in `stmts`.
fn defined_labels(stmts: &[PreHirStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(name) => out.push(name.clone()),
            PreHirStmt::Block(body) => defined_labels(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                defined_labels(then_body, out);
                defined_labels(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    defined_labels(&case.body, out);
                }
                defined_labels(default, out);
            }
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => defined_labels(body, out),
            _ => {}
        }
    }
}

/// Remove statements no path reaches.
///
/// `referenced` is the function-wide count of goto targets: a tail that
/// defines a label something still jumps to is reachable by that jump and is
/// kept, however unreachable it looks from the statement order. C labels are
/// function-scoped, which is why this has to be a whole-function fact rather
/// than a local one.
pub fn drop_unreachable_tails(
    body: Vec<PreHirStmt>,
    referenced: &HashMap<String, usize>,
) -> (Vec<PreHirStmt>, bool) {
    let mut changed = false;
    let mut out: Vec<PreHirStmt> = Vec::with_capacity(body.len());
    let mut left = false;

    for stmt in body {
        if left {
            let mut labels = Vec::new();
            defined_labels(std::slice::from_ref(&stmt), &mut labels);
            if labels.iter().any(|l| referenced.contains_key(l)) {
                // Reachable by a jump; keep it and resume.
                out.push(stmt);
                left = false;
                continue;
            }
            changed = true;
            continue;
        }
        let stmt = recurse(stmt, referenced, &mut changed);
        left = diverges(&stmt);
        out.push(stmt);
    }
    (out, changed)
}

fn recurse(
    stmt: PreHirStmt,
    referenced: &HashMap<String, usize>,
    changed: &mut bool,
) -> PreHirStmt {
    fn body_of(
        body: &std::rc::Rc<Vec<PreHirStmt>>,
        referenced: &HashMap<String, usize>,
        changed: &mut bool,
    ) -> std::rc::Rc<Vec<PreHirStmt>> {
        let (next, did) = drop_unreachable_tails(body.as_ref().clone(), referenced);
        *changed |= did;
        std::rc::Rc::new(next)
    }
    match stmt {
        PreHirStmt::Block(body) => PreHirStmt::Block(body_of(&body, referenced, changed)),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond,
            then_body: body_of(&then_body, referenced, changed),
            else_body: body_of(&else_body, referenced, changed),
        },
        PreHirStmt::While { cond, body } => PreHirStmt::While {
            cond,
            body: body_of(&body, referenced, changed),
        },
        PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: body_of(&body, referenced, changed),
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
            body: body_of(&body, referenced, changed),
        },
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => PreHirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.body = body_of(&case.body, referenced, changed);
                    case
                })
                .collect(),
            default: body_of(&default, referenced, changed),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn ret(v: i64) -> PreHirStmt {
        PreHirStmt::Return(Some(fission_midend_prehir::PreHirExpr::Const(
            v,
            fission_midend_core::ir::NirType::Int {
                bits: 32,
                signed: true,
            },
        )))
    }
    fn assign() -> PreHirStmt {
        PreHirStmt::Expr(fission_midend_prehir::PreHirExpr::Var("x".into()))
    }
    fn cond() -> fission_midend_prehir::PreHirExpr {
        fission_midend_prehir::PreHirExpr::Var("c".into())
    }
    fn if_both(then_s: Vec<PreHirStmt>, else_s: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: cond(),
            then_body: Rc::new(then_s),
            else_body: Rc::new(else_s),
        }
    }
    fn run(body: Vec<PreHirStmt>) -> (Vec<PreHirStmt>, bool) {
        drop_unreachable_tails(body, &HashMap::default())
    }

    /// The measured shape: both arms return, and the exit block follows anyway.
    #[test]
    fn drops_the_exit_block_behind_an_exhaustive_if() {
        let (out, changed) = run(vec![if_both(vec![ret(0)], vec![ret(1)]), ret(2)]);
        assert!(changed);
        assert_eq!(out.len(), 1);
    }

    /// One arm falling through is a path to the tail, so the tail stays.
    #[test]
    fn keeps_the_tail_when_an_arm_falls_through() {
        let (out, changed) = run(vec![if_both(vec![ret(0)], vec![assign()]), ret(2)]);
        assert!(!changed);
        assert_eq!(out.len(), 2);
    }

    /// An empty `else` is a fall-through path even though the `then` returns.
    #[test]
    fn an_empty_arm_is_a_path() {
        let (out, changed) = run(vec![if_both(vec![ret(0)], vec![]), ret(2)]);
        assert!(!changed);
        assert_eq!(out.len(), 2);
    }

    /// A loop is not proven to diverge without reasoning about its condition,
    /// which this deliberately does not do.
    #[test]
    fn a_loop_does_not_prove_divergence() {
        let (out, _) = run(vec![
            PreHirStmt::While {
                cond: cond(),
                body: Rc::new(vec![assign()]),
            },
            ret(2),
        ]);
        assert_eq!(out.len(), 2);
    }

    /// Unreachable in statement order, reachable by a jump: C labels are
    /// function-scoped, so order alone never proves a tail dead.
    #[test]
    fn keeps_a_tail_something_still_jumps_to() {
        let mut referenced = HashMap::default();
        referenced.insert("L".to_string(), 1usize);
        let body = vec![
            if_both(vec![ret(0)], vec![ret(1)]),
            PreHirStmt::Label("L".into()),
            ret(2),
        ];
        let (out, _) = drop_unreachable_tails(body, &referenced);
        assert_eq!(out.len(), 3, "label target and its body must survive");
    }

    /// Nothing referenced the label, so the tail really is dead.
    #[test]
    fn drops_a_tail_whose_label_nothing_references() {
        let (out, changed) = run(vec![
            if_both(vec![ret(0)], vec![ret(1)]),
            PreHirStmt::Label("L".into()),
            ret(2),
        ]);
        assert!(changed);
        assert_eq!(out.len(), 1);
    }

    /// The same trim applies inside a nested body, not only at top level.
    #[test]
    fn trims_inside_a_nested_arm() {
        let inner = if_both(vec![ret(0)], vec![ret(1)]);
        let (out, changed) = run(vec![if_both(vec![inner, ret(9)], vec![assign()])]);
        assert!(changed);
        match &out[0] {
            PreHirStmt::If { then_body, .. } => assert_eq!(then_body.len(), 1),
            other => panic!("expected an if, got {other:?}"),
        }
    }
}
