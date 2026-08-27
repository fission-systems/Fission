//! Size/length/count variable naming from argument position at well-known
//! libc-shaped calls (`memcpy`'s 3rd argument, `malloc`'s 1st, ...).
//!
//! Deliberately a small, explicit table rather than a general signature
//! database: these ~15 functions and their argument roles are effectively
//! fixed across the ecosystem (unlike a name/library match, there's no
//! version skew to track), so a hardcoded table is both simpler and more
//! maintainable than wiring up a signature-database lookup for the same
//! handful of entries.

use super::super::{HirExpr, HirFunction, HirStmt};
use super::Candidate;
use super::util::as_var;
use std::collections::HashMap;

pub(super) const PRIORITY: u32 = 70; // after pointer naming

/// Callee name (case/underscore-insensitive) -> (argument index, suggested name).
const SIZE_PARAM_FUNCTIONS: &[(&str, &[(usize, &str)])] = &[
    ("malloc", &[(0, "size")]),
    ("calloc", &[(0, "count"), (1, "size")]),
    ("realloc", &[(1, "size")]),
    ("memcpy", &[(2, "n")]),
    ("memmove", &[(2, "n")]),
    ("memset", &[(2, "n")]),
    ("memcmp", &[(2, "n")]),
    ("strncpy", &[(2, "n")]),
    ("strncat", &[(2, "n")]),
    ("strncmp", &[(2, "n")]),
    ("strnlen", &[(1, "n")]),
    ("read", &[(2, "count")]),
    ("write", &[(2, "count")]),
    ("fread", &[(1, "size"), (2, "count")]),
    ("fwrite", &[(1, "size"), (2, "count")]),
    ("recv", &[(2, "len")]),
    ("send", &[(2, "len")]),
    ("snprintf", &[(1, "size")]),
    ("vsnprintf", &[(1, "size")]),
    ("fgets", &[(1, "size")]),
];

fn normalize(name: &str) -> String {
    name.to_ascii_lowercase()
        .trim_matches('_')
        .replace("__", "_")
}

pub(super) fn candidates(func: &HirFunction) -> Vec<Candidate> {
    let mut suggestions: HashMap<String, &'static str> = HashMap::new();
    for stmt in &func.body {
        walk_stmt(stmt, &mut suggestions);
    }
    suggestions
        .into_iter()
        .map(|(name, suggested)| Candidate {
            name,
            new_name: suggested.to_string(),
            score: 50,
        })
        .collect()
}

fn walk_stmt(stmt: &HirStmt, out: &mut HashMap<String, &'static str>) {
    match stmt {
        HirStmt::Assign { rhs, .. } => check_expr(rhs, out),
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            check_expr(e, out)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body {
                walk_stmt(s, out);
            }
        }
        HirStmt::While { cond, body } => {
            check_expr(cond, out);
            for s in body {
                walk_stmt(s, out);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for s in body {
                walk_stmt(s, out);
            }
            check_expr(cond, out);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(cond, out);
            for s in then_body {
                walk_stmt(s, out);
            }
            for s in else_body {
                walk_stmt(s, out);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, out);
            }
            if let Some(c) = cond {
                check_expr(c, out);
            }
            if let Some(u) = update {
                walk_stmt(u, out);
            }
            for s in body {
                walk_stmt(s, out);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            check_expr(expr, out);
            for case in cases {
                for s in &case.body {
                    walk_stmt(s, out);
                }
            }
            for s in default {
                walk_stmt(s, out);
            }
        }
    }
}

fn check_expr(expr: &HirExpr, out: &mut HashMap<String, &'static str>) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            check_call(target, args, out);
            for a in args {
                check_expr(a, out);
            }
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => check_expr(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            check_expr(lhs, out);
            check_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            check_expr(cond, out);
            check_expr(then_expr, out);
            check_expr(else_expr, out);
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => check_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            check_expr(base, out);
            check_expr(index, out);
        }
    }
}

fn check_call(target: &str, args: &[HirExpr], out: &mut HashMap<String, &'static str>) {
    let normalized = normalize(target);
    let Some((_, param_info)) = SIZE_PARAM_FUNCTIONS
        .iter()
        .find(|(callee, _)| *callee == normalized)
    else {
        return;
    };
    for &(idx, suggested) in *param_info {
        let Some(arg) = args.get(idx) else { continue };
        let Some(name) = as_var(arg) else { continue };
        out.entry(name.to_string()).or_insert(suggested);
    }
}
