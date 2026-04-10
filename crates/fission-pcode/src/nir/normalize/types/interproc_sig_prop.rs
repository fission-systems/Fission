//! Call-site arity constraints for static callee symbols (intra-build merge hook).
//!
//! Walks all `Call` expressions and records `max(args.len())` per callee name string.
//! Downstream pipelines can merge [`HirFunction::callee_observed_max_arity`] across functions
//! for interprocedural lower bounds (sound: never under-estimate arity).
//!
//! Broader **call-graph SCC + lattice fixpoint** propagation of pointer/integer types belongs
//! in a session-scoped pass with a fact store / provenance layer; keep arity here as a cheap
//! monotone lower bound per function.

use crate::nir::types::{
    parse_call_target_address, CallEdgeKind, CallSummary, CallTargetProvenance, CallTargetRef,
    HirExpr, HirFunction, HirStmt, NirType,
};
use indexmap::IndexMap;

use super::super::wave_stats::add_interproc_constraint_rounds;

fn merge_arity(map: &mut IndexMap<String, usize>, callee: &str, arity: usize) {
    map.entry(callee.to_string())
        .and_modify(|v| *v = (*v).max(arity))
        .or_insert(arity);
}

fn summary_seed(target: &str) -> CallTargetRef {
    if let Some(address) = parse_call_target_address(target) {
        return CallTargetRef {
            address: Some(address),
            symbol: target.to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 160,
        };
    }
    CallTargetRef {
        address: None,
        symbol: target.to_string(),
        provenance: CallTargetProvenance::Reference,
        edge_kind: CallEdgeKind::Reference,
        confidence: 128,
    }
}

fn merge_summary(map: &mut IndexMap<String, CallSummary>, callee: &str, arity: usize) {
    map.entry(callee.to_string())
        .and_modify(|summary| {
            summary.min_arity = summary.min_arity.min(arity);
            summary.max_arity = summary.max_arity.max(arity);
            if let Some(locked) = summary.locked_exact_arity && locked != arity {
                summary.locked_exact_arity = None;
            }
            if summary.param_lattices.len() < arity {
                summary.param_lattices.resize(arity, NirType::Unknown);
            }
        })
        .or_insert_with(|| CallSummary {
            target: summary_seed(callee),
            min_arity: arity,
            max_arity: arity,
            locked_exact_arity: None,
            return_lattice: NirType::Unknown,
            param_lattices: vec![NirType::Unknown; arity],
        });
}

fn scan_expr(
    expr: &HirExpr,
    arity_map: &mut IndexMap<String, usize>,
    summary_map: &mut IndexMap<String, CallSummary>,
) {
    match expr {
        HirExpr::Call { target, args, .. } => {
            merge_arity(arity_map, target, args.len());
            merge_summary(summary_map, target, args.len());
            for a in args {
                scan_expr(a, arity_map, summary_map);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => scan_expr(expr, arity_map, summary_map),
        HirExpr::Binary { lhs, rhs, .. } => {
            scan_expr(lhs, arity_map, summary_map);
            scan_expr(rhs, arity_map, summary_map);
        }
        HirExpr::PtrOffset { base, .. } => scan_expr(base, arity_map, summary_map),
        HirExpr::Index { base, index, .. } => {
            scan_expr(base, arity_map, summary_map);
            scan_expr(index, arity_map, summary_map);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

fn scan_stmts(
    body: &[HirStmt],
    arity_map: &mut IndexMap<String, usize>,
    summary_map: &mut IndexMap<String, CallSummary>,
) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } => scan_expr(rhs, arity_map, summary_map),
            HirStmt::Expr(e) => scan_expr(e, arity_map, summary_map),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => scan_stmts(stmts, arity_map, summary_map),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                scan_expr(expr, arity_map, summary_map);
                for c in cases {
                    scan_stmts(&c.body, arity_map, summary_map);
                }
                scan_stmts(default, arity_map, summary_map);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                scan_expr(cond, arity_map, summary_map);
                scan_stmts(then_body, arity_map, summary_map);
                scan_stmts(else_body, arity_map, summary_map);
            }
            HirStmt::Return(Some(e)) => scan_expr(e, arity_map, summary_map),
            _ => {}
        }
    }
}

pub(crate) fn apply_interproc_callsite_arity_pass(func: &mut HirFunction) -> bool {
    let mut fresh = IndexMap::new();
    let mut summaries = IndexMap::new();
    scan_stmts(&func.body, &mut fresh, &mut summaries);
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
    for (key, summary) in summaries {
        func.callee_summaries
            .entry(key)
            .and_modify(|existing| {
                existing.min_arity = existing.min_arity.min(summary.min_arity);
                existing.max_arity = existing.max_arity.max(summary.max_arity);
                if existing.param_lattices.len() < summary.param_lattices.len() {
                    existing
                        .param_lattices
                        .resize(summary.param_lattices.len(), NirType::Unknown);
                }
            })
            .or_insert(summary);
    }
    if improved > 0 {
        add_interproc_constraint_rounds(1);
    }
    true
}
