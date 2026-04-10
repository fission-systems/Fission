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
    parse_call_target_address, CallEdgeKind, CallEffectSummary, CallSummary,
    CallTargetProvenance, CallTargetRef, HirExpr, HirFunction, HirLValue, HirStmt, NirType,
    WrapperClass,
};
use indexmap::IndexMap;

use super::super::wave_stats::{
    add_call_effect_summary_refinements, add_interproc_constraint_rounds, add_wrapper_summary_folds,
};

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
            if arity == 0 && summary.effect_summary.escapes_args != Some(false) {
                summary.effect_summary.escapes_args = Some(false);
                summary.effect_summary.confidence = summary.effect_summary.confidence.max(96);
            }
        })
        .or_insert_with(|| CallSummary {
            target: summary_seed(callee),
            min_arity: arity,
            max_arity: arity,
            locked_exact_arity: None,
            return_lattice: NirType::Unknown,
            param_lattices: vec![NirType::Unknown; arity],
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: (arity == 0).then_some(false),
                wrapper_class: WrapperClass::None,
                confidence: if arity == 0 { 96 } else { 0 },
            },
        });
}

fn classify_wrapper_body(func: &HirFunction) -> Option<WrapperClass> {
    match func.body.as_slice() {
        [HirStmt::Return(Some(HirExpr::Call { .. }))] => Some(WrapperClass::TailForwarder),
        [
            HirStmt::Assign {
                lhs: HirLValue::Var(temp),
                rhs: HirExpr::Call { .. },
            },
            HirStmt::Return(Some(HirExpr::Var(ret))),
        ] if temp == ret => Some(WrapperClass::PureAdapter),
        _ => None,
    }
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
    let wrapper_class = classify_wrapper_body(func);
    let mut effect_refinements = 0usize;
    let mut wrapper_refinements = 0usize;
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
                if existing.effect_summary.escapes_args.is_none()
                    && summary.effect_summary.escapes_args.is_some()
                {
                    existing.effect_summary.escapes_args = summary.effect_summary.escapes_args;
                    existing.effect_summary.confidence =
                        existing.effect_summary.confidence.max(summary.effect_summary.confidence);
                    effect_refinements += 1;
                }
                if existing.effect_summary.wrapper_class == WrapperClass::None
                    && wrapper_class.is_some()
                {
                    existing.effect_summary.wrapper_class = wrapper_class.unwrap_or(WrapperClass::None);
                    existing.effect_summary.confidence = existing.effect_summary.confidence.max(64);
                    wrapper_refinements += 1;
                }
            })
            .or_insert_with(|| {
                let mut summary = summary;
                if let Some(wrapper_class) = wrapper_class {
                    summary.effect_summary.wrapper_class = wrapper_class;
                    summary.effect_summary.confidence = summary.effect_summary.confidence.max(64);
                    wrapper_refinements += 1;
                }
                if summary.effect_summary.escapes_args.is_some() {
                    effect_refinements += 1;
                }
                summary
            });
    }
    if improved > 0 {
        add_interproc_constraint_rounds(1);
    }
    add_call_effect_summary_refinements(effect_refinements);
    add_wrapper_summary_folds(wrapper_refinements);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::support::CallingConvention;
    use crate::nir::types::NirBinding;

    fn empty_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    #[test]
    fn interproc_summary_marks_zero_arity_as_non_escaping() {
        let mut func = HirFunction {
            name: "caller".to_string(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::Expr(HirExpr::Call {
                target: "FUN_0x140001000".to_string(),
                args: vec![],
                ty: NirType::Unknown,
            })],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };
        assert!(apply_interproc_callsite_arity_pass(&mut func));
        let summary = func.callee_summaries.get("FUN_0x140001000").unwrap();
        assert_eq!(summary.effect_summary.escapes_args, Some(false));
    }

    #[test]
    fn interproc_summary_detects_simple_wrapper_shape() {
        let mut func = HirFunction {
            name: "wrapper".to_string(),
            params: vec![empty_binding("param_1")],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Call {
                target: "sub_140010000".to_string(),
                args: vec![HirExpr::Var("param_1".to_string())],
                ty: NirType::Unknown,
            }))],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };
        assert!(apply_interproc_callsite_arity_pass(&mut func));
        let summary = func.callee_summaries.get("sub_140010000").unwrap();
        assert_eq!(summary.effect_summary.wrapper_class, WrapperClass::TailForwarder);
    }
}
