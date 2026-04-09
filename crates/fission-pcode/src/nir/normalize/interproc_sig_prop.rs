//! Call-site arity constraints for static callee symbols (intra-build merge hook).
//!
//! Walks all `Call` expressions and records `max(args.len())` per callee name string.
//! Downstream pipelines can merge [`HirFunction::callee_observed_max_arity`] across functions
//! for interprocedural lower bounds (sound: never under-estimate arity).

use crate::nir::types::{HirExpr, HirFunction, HirStmt};
use std::collections::HashMap;

use super::wave_stats::add_interproc_constraint_rounds;

fn merge_arity(map: &mut HashMap<String, usize>, callee: &str, arity: usize) {
    map.entry(callee.to_string())
        .and_modify(|v| *v = (*v).max(arity))
        .or_insert(arity);
}

fn scan_expr(expr: &HirExpr, map: &mut HashMap<String, usize>) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            merge_arity(map, target, args.len());
            for a in args {
                scan_expr(a, map);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => scan_expr(expr, map),
        HirExpr::Binary { lhs, rhs, .. } => {
            scan_expr(lhs, map);
            scan_expr(rhs, map);
        }
        HirExpr::PtrOffset { base, .. } => scan_expr(base, map),
        HirExpr::Index { base, index, .. } => {
            scan_expr(base, map);
            scan_expr(index, map);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

fn scan_stmts(body: &[HirStmt], map: &mut HashMap<String, usize>) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } => scan_expr(rhs, map),
            HirStmt::Expr(e) => scan_expr(e, map),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => scan_stmts(stmts, map),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                scan_expr(expr, map);
                for c in cases {
                    scan_stmts(&c.body, map);
                }
                scan_stmts(default, map);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                scan_expr(cond, map);
                scan_stmts(then_body, map);
                scan_stmts(else_body, map);
            }
            HirStmt::Return(Some(e)) => scan_expr(e, map),
            _ => {}
        }
    }
}

pub(super) fn apply_interproc_callsite_arity_pass(func: &mut HirFunction) -> bool {
    let mut fresh = HashMap::new();
    scan_stmts(&func.body, &mut fresh);
    if fresh.is_empty() {
        return false;
    }
    let mut improved = 0usize;
    for (k, v) in fresh {
        let entry = func.callee_observed_max_arity.entry(k).or_insert(0);
        if v > *entry {
            *entry = v;
            improved += 1;
        }
    }
    if improved > 0 {
        add_interproc_constraint_rounds(1);
    }
    true
}
