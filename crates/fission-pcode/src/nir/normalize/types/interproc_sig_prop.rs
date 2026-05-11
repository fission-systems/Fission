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
    CallEdgeKind, CallEffectSummary, CallSummary, CallTargetProvenance, CallTargetRef, HirExpr,
    HirFunction, HirStmt, MemoryEffectRegion, NirType, PrototypeSummary, SummarySoundness,
    WrapperClass, parse_call_target_address,
};
use indexmap::IndexMap;

use super::super::wave_stats::{
    add_call_effect_summary_refinements, add_interproc_constraint_rounds,
    add_prototype_summary_refinements, add_prototype_summary_rounds, add_wrapper_summary_folds,
};
use super::{
    callsite_type_prop::{api_signature, is_known_api_signature, win_type_name_to_nir},
    summarize_wrapper_hir_function, summary_soundness_for_wrapper,
};

fn merge_arity(map: &mut IndexMap<String, usize>, callee: &str, arity: usize) {
    map.entry(callee.to_string())
        .and_modify(|v| *v = (*v).max(arity))
        .or_insert(arity);
}

fn summary_seed(target: &str) -> CallTargetRef {
    if is_known_api_signature(target) {
        return CallTargetRef {
            address: parse_call_target_address(target),
            symbol: target.to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 224,
        };
    }
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
            summary.prototype.min_arity = summary.prototype.min_arity.min(arity);
            summary.prototype.max_arity = summary.prototype.max_arity.max(arity);
            if let Some(locked) = summary.prototype.locked_exact_arity
                && locked != arity
            {
                summary.prototype.locked_exact_arity = None;
            }
            if summary.prototype.param_lattices.len() < arity {
                summary
                    .prototype
                    .param_lattices
                    .resize(arity, NirType::Unknown);
            }
            if arity == 0 && summary.effect_summary.escapes_args != Some(false) {
                summary.effect_summary.escapes_args = Some(false);
                summary.effect_summary.confidence = summary.effect_summary.confidence.max(96);
            }
        })
        .or_insert_with(|| CallSummary {
            target: summary_seed(callee),
            prototype: PrototypeSummary {
                min_arity: arity,
                max_arity: arity,
                locked_exact_arity: None,
                return_lattice: NirType::Unknown,
                param_lattices: vec![NirType::Unknown; arity],
                soundness: SummarySoundness::Pessimistic,
            },
            effect_summary: CallEffectSummary {
                reads_memory: None,
                writes_memory: None,
                escapes_args: (arity == 0).then_some(false),
                regions: Vec::new(),
                wrapper_class: WrapperClass::None,
                wrapper_of: None,
                confidence: if arity == 0 { 96 } else { 0 },
            },
        });
}

fn apply_import_signature_seed(summary: &mut CallSummary, callee: &str) -> usize {
    let Some(sig) = api_signature(callee) else {
        return 0;
    };
    let mut refinements = 0usize;
    let exact_arity = sig.params.len();
    if summary.prototype.locked_exact_arity != Some(exact_arity) {
        summary.prototype.locked_exact_arity = Some(exact_arity);
        refinements += 1;
    }
    if summary.prototype.max_arity != exact_arity || summary.prototype.min_arity > exact_arity {
        summary.prototype.min_arity = summary.prototype.min_arity.min(exact_arity);
        summary.prototype.max_arity = exact_arity;
        refinements += 1;
    }
    if summary.prototype.param_lattices.len() < exact_arity {
        summary
            .prototype
            .param_lattices
            .resize(exact_arity, NirType::Unknown);
    }
    for (idx, param) in sig.params.iter().enumerate() {
        let Some(param_ty) = win_type_name_to_nir(&param.type_name) else {
            continue;
        };
        if summary.prototype.param_lattices[idx] == NirType::Unknown && param_ty != NirType::Unknown
        {
            summary.prototype.param_lattices[idx] = param_ty;
            refinements += 1;
        }
    }
    if summary.prototype.return_lattice == NirType::Unknown
        && let Some(ret_ty) = win_type_name_to_nir(&sig.return_type)
    {
        summary.prototype.return_lattice = ret_ty;
        refinements += 1;
    }
    if refinements > 0 {
        summary.prototype.soundness = SummarySoundness::Optimistic;
        summary.target.provenance = CallTargetProvenance::Import;
        summary.target.edge_kind = CallEdgeKind::Import;
        summary.target.confidence = summary.target.confidence.max(224);
        if sig.params.iter().any(|param| param.type_name.contains('*')) {
            if summary.effect_summary.reads_memory.is_none() {
                summary.effect_summary.reads_memory = Some(true);
                refinements += 1;
            }
            if !summary
                .effect_summary
                .regions
                .contains(&MemoryEffectRegion::Aggregate)
            {
                summary
                    .effect_summary
                    .regions
                    .push(MemoryEffectRegion::Aggregate);
            }
            summary.effect_summary.confidence = summary.effect_summary.confidence.max(128);
        }
    }
    refinements
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            scan_expr(cond, arity_map, summary_map);
            scan_expr(then_expr, arity_map, summary_map);
            scan_expr(else_expr, arity_map, summary_map);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
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
    let saw_summary_round = !summaries.is_empty();
    let mut improved = 0usize;
    let wrapper_shape = summarize_wrapper_hir_function(func);
    let mut prototype_refinements = 0usize;
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
                existing.prototype.min_arity = existing
                    .prototype
                    .min_arity
                    .min(summary.prototype.min_arity);
                existing.prototype.max_arity = existing
                    .prototype
                    .max_arity
                    .max(summary.prototype.max_arity);
                if existing.prototype.param_lattices.len() < summary.prototype.param_lattices.len()
                {
                    existing
                        .prototype
                        .param_lattices
                        .resize(summary.prototype.param_lattices.len(), NirType::Unknown);
                }
                if existing.prototype.soundness == SummarySoundness::Pessimistic
                    && summary.prototype.soundness == SummarySoundness::Optimistic
                {
                    existing.prototype.soundness = SummarySoundness::Optimistic;
                    prototype_refinements += 1;
                }
                if existing.effect_summary.escapes_args.is_none()
                    && summary.effect_summary.escapes_args.is_some()
                {
                    existing.effect_summary.escapes_args = summary.effect_summary.escapes_args;
                    existing.effect_summary.confidence = existing
                        .effect_summary
                        .confidence
                        .max(summary.effect_summary.confidence);
                    effect_refinements += 1;
                }
                for region in &summary.effect_summary.regions {
                    if !existing.effect_summary.regions.contains(region) {
                        existing.effect_summary.regions.push(*region);
                        effect_refinements += 1;
                    }
                }
                if let Some(procedure_summary) = wrapper_shape.as_ref()
                    && existing.effect_summary.wrapper_class == WrapperClass::None
                {
                    let proof = procedure_summary
                        .wrapper_contraction
                        .as_ref()
                        .expect("wrapper summary missing contraction proof");
                    existing.effect_summary.wrapper_class = proof.wrapper_class;
                    existing.effect_summary.wrapper_of = Some(proof.target.clone());
                    existing.effect_summary.confidence = existing.effect_summary.confidence.max(64);
                    existing.prototype.soundness = summary_soundness_for_wrapper(procedure_summary);
                    wrapper_refinements += 1;
                }
                let target_symbol = existing.target.symbol.clone();
                prototype_refinements += apply_import_signature_seed(existing, &target_symbol);
            })
            .or_insert_with(|| {
                let mut summary = summary;
                let target_symbol = summary.target.symbol.clone();
                prototype_refinements += apply_import_signature_seed(&mut summary, &target_symbol);
                if let Some(procedure_summary) = wrapper_shape.as_ref() {
                    let proof = procedure_summary
                        .wrapper_contraction
                        .as_ref()
                        .expect("wrapper summary missing contraction proof");
                    summary.effect_summary.wrapper_class = proof.wrapper_class;
                    summary.effect_summary.wrapper_of = Some(proof.target.clone());
                    summary.prototype.soundness = summary_soundness_for_wrapper(procedure_summary);
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
    if saw_summary_round || !func.callee_summaries.is_empty() {
        add_prototype_summary_rounds(1);
    }
    add_prototype_summary_refinements(prototype_refinements);
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
            suppress_entry_register_params: false,
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
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };
        assert!(apply_interproc_callsite_arity_pass(&mut func));
        let summary = func.callee_summaries.get("sub_140010000").unwrap();
        assert_eq!(
            summary.effect_summary.wrapper_class,
            WrapperClass::TailForwarder
        );
        assert_eq!(
            summary
                .effect_summary
                .wrapper_of
                .as_ref()
                .map(|target| target.symbol.as_str()),
            Some("sub_140010000")
        );
    }
}
