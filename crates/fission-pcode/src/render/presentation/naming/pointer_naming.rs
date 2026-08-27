//! Pointer variable naming (`ptr`, `cur`, `next`, ...) from dereference,
//! pointer-arithmetic, and linked-list-traversal usage patterns.

use super::super::{HirExpr, HirFunction, HirLValue, HirStmt};
use super::Candidate;
use super::util::as_var;
use std::collections::HashMap;

pub(super) const PRIORITY: u32 = 45; // after loop counters, before size

const GENERIC_NAMES: &[&str] = &["ptr", "p", "addr"];
const ITERATOR_NAMES: &[&str] = &["cur", "iter", "node"];

const SCORE_DEREFERENCE: u32 = 25;
const SCORE_ARITHMETIC: u32 = 20;
const SCORE_LINKED_LIST: u32 = 30;
const THRESHOLD: u32 = 20;

#[derive(Default)]
struct Info {
    score: u32,
    is_linked_list: bool,
}

pub(super) fn candidates(func: &HirFunction) -> Vec<Candidate> {
    let mut info: HashMap<String, Info> = HashMap::new();
    for stmt in &func.body {
        walk_stmt(stmt, &mut info);
    }

    let mut scored: Vec<(String, Info)> = info.into_iter().collect();
    scored.sort_by(|a, b| b.1.score.cmp(&a.1.score).then_with(|| a.0.cmp(&b.0)));

    let mut generic_idx = 0;
    let mut iter_idx = 0;
    let mut out = Vec::new();
    for (name, i) in scored {
        if i.score < THRESHOLD {
            continue;
        }
        let (names, idx) = if i.is_linked_list {
            (ITERATOR_NAMES, &mut iter_idx)
        } else {
            (GENERIC_NAMES, &mut generic_idx)
        };
        let new_name = names
            .get(*idx)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ptr{idx}"));
        *idx += 1;
        out.push(Candidate {
            name,
            new_name,
            score: i.score,
        });
    }
    out
}

fn record(info: &mut HashMap<String, Info>, name: &str, score: u32, linked_list: bool) {
    let entry = info.entry(name.to_string()).or_default();
    entry.score += score;
    entry.is_linked_list |= linked_list;
}

fn walk_stmt(stmt: &HirStmt, info: &mut HashMap<String, Info>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            check_lvalue(lhs, info);
            check_expr(rhs, info);
            check_linked_list_self_update(lhs, rhs, info);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            check_expr(e, info);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body {
                walk_stmt(s, info);
            }
        }
        HirStmt::While { cond, body } => {
            check_expr(cond, info);
            for s in body {
                walk_stmt(s, info);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for s in body {
                walk_stmt(s, info);
            }
            check_expr(cond, info);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(cond, info);
            for s in then_body {
                walk_stmt(s, info);
            }
            for s in else_body {
                walk_stmt(s, info);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, info);
            }
            if let Some(c) = cond {
                check_expr(c, info);
            }
            if let Some(u) = update {
                walk_stmt(u, info);
            }
            for s in body {
                walk_stmt(s, info);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            check_expr(expr, info);
            for case in cases {
                for s in &case.body {
                    walk_stmt(s, info);
                }
            }
            for s in default {
                walk_stmt(s, info);
            }
        }
    }
}

fn check_lvalue(lhs: &HirLValue, info: &mut HashMap<String, Info>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => check_address(ptr, info),
        HirLValue::Index { base, index, .. } => {
            check_address(base, info);
            check_expr(index, info);
        }
        HirLValue::FieldAccess { base, .. } => check_address(base, info),
    }
}

fn check_expr(expr: &HirExpr, info: &mut HashMap<String, Info>) {
    match expr {
        HirExpr::Load { ptr, .. } => {
            check_address(ptr, info);
            check_expr(ptr, info);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => check_expr(expr, info),
        HirExpr::Binary { lhs, rhs, .. } => {
            check_expr(lhs, info);
            check_expr(rhs, info);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            check_expr(cond, info);
            check_expr(then_expr, info);
            check_expr(else_expr, info);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                check_expr(a, info);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            // `base + k`: base is a pointer being offset.
            if let Some(name) = as_var(base) {
                record(info, name, SCORE_ARITHMETIC, false);
            }
            check_expr(base, info);
        }
        HirExpr::FieldAccess { base, .. } | HirExpr::AggregateCopy { src: base, .. } => {
            check_expr(base, info);
        }
        HirExpr::Index { base, index, .. } => {
            check_expr(base, info);
            check_expr(index, info);
        }
    }
}

/// An address expression: a bare `Var` used directly as a load/store address
/// is a dereferenced pointer. `PtrOffset`'s own base gets credited in
/// [`check_expr`] regardless of whether this call site also reaches it.
fn check_address(addr: &HirExpr, info: &mut HashMap<String, Info>) {
    if let Some(name) = as_var(addr) {
        record(info, name, SCORE_DEREFERENCE, false);
    }
}

/// `cur = *(cur + k)` (or the `PtrOffset` surface form Fission's builder
/// prefers): the classic linked-list-traversal self-update, `cur = cur->next`.
fn check_linked_list_self_update(lhs: &HirLValue, rhs: &HirExpr, info: &mut HashMap<String, Info>) {
    let HirLValue::Var(dst) = lhs else {
        return;
    };
    let HirExpr::Load { ptr, .. } = rhs else {
        return;
    };
    let HirExpr::PtrOffset { base, .. } = ptr.as_ref() else {
        return;
    };
    if as_var(base) == Some(dst.as_str()) {
        record(info, dst, SCORE_LINKED_LIST, true);
    }
}
