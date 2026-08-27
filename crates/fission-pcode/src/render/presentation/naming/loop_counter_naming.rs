//! Loop induction variable naming (`i`, `j`, `k`, ...) from structured
//! `For`/`While`/`DoWhile` nodes, by nesting depth.
//!
//! Runs at the highest priority (lowest number): once a variable is a loop's
//! own counter, no other pattern's guess (pointer, size, ...) should
//! override it.

use super::super::{HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt};
use super::Candidate;
use super::util::as_var;

pub(super) const PRIORITY: u32 = 10;

const COUNTER_NAMES: &[&str] = &["i", "j", "k", "l", "m", "n"];

pub(super) fn candidates(func: &HirFunction) -> Vec<Candidate> {
    let mut found: Vec<(String, usize)> = Vec::new(); // (var name, nesting depth)
    collect(&func.body, 0, &mut found);

    // Stable order: outermost loops name first (matches source reading
    // order and angr's own depth-then-position tie-break).
    found.sort_by_key(|(_, depth)| *depth);

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, _depth) in found {
        if !seen.insert(name.clone()) {
            continue;
        }
        let idx = out.len();
        let new_name = COUNTER_NAMES
            .get(idx)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("i{idx}"));
        out.push(Candidate {
            name,
            new_name,
            score: 100,
        });
    }
    out
}

fn collect(stmts: &[HirStmt], depth: usize, out: &mut Vec<(String, usize)>) {
    for stmt in stmts {
        match stmt {
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(name) = update
                    .as_deref()
                    .and_then(counter_from_update)
                    .or_else(|| init.as_deref().and_then(counter_from_init))
                    .or_else(|| cond.as_ref().and_then(counter_from_cond))
                {
                    out.push((name, depth));
                }
                collect(body, depth + 1, out);
            }
            HirStmt::While { cond, body } => {
                if let Some(name) = counter_from_cond(cond) {
                    out.push((name, depth));
                }
                collect(body, depth + 1, out);
            }
            HirStmt::DoWhile { body, cond } => {
                if let Some(name) = counter_from_cond(cond) {
                    out.push((name, depth));
                }
                collect(body, depth + 1, out);
            }
            HirStmt::Block(body) => collect(body, depth, out),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect(then_body, depth, out);
                collect(else_body, depth, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect(&case.body, depth, out);
                }
                collect(default, depth, out);
            }
            _ => {}
        }
    }
}

/// `i = i + 1` / `i = i - 1` (the `For` update clause).
fn counter_from_update(update: &HirStmt) -> Option<String> {
    let HirStmt::Assign {
        lhs: HirLValue::Var(name),
        rhs: HirExpr::Binary { op, lhs, rhs, .. },
    } = update
    else {
        return None;
    };
    if !matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub) {
        return None;
    }
    let self_ref = as_var(lhs) == Some(name.as_str()) || as_var(rhs) == Some(name.as_str());
    self_ref.then(|| name.clone())
}

/// `i = 0` (the `For` init clause).
fn counter_from_init(init: &HirStmt) -> Option<String> {
    match init {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Const(_, _),
        } => Some(name.clone()),
        _ => None,
    }
}

/// `i < n` / `i != 0` -- prefer the left operand (`i op bound`, the common
/// source order), fall back to the right.
fn counter_from_cond(cond: &HirExpr) -> Option<String> {
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    if !matches!(
        op,
        HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
            | HirBinaryOp::Ne
            | HirBinaryOp::Eq
    ) {
        return None;
    }
    as_var(lhs).or_else(|| as_var(rhs)).map(ToOwned::to_owned)
}
