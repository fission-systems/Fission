//! Observation-only `debug_decomp` payload for CLI JSON (`schema_version` 1).
//!
//! Does not influence decompilation output or routing — aggregates loader identity,
//! coarse stage status strings, timing/size counters from [`NirBuildStats`], and
//! heuristic [`owner_buckets`] for benchmarks.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_pcode::{NirBuildStats, NirHintStats};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

/// Serialize bundle JSON with `{ "schema_version": 1, "functions": [...] }`.
pub(crate) fn write_debug_decomp_bundle_file(path: &Path, bundles: &[serde_json::Value]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let envelope = serde_json::json!({
        "schema_version": 1u32,
        "functions": bundles,
    });
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| io::Error::other(format!("debug bundle JSON: {e}")))?;
    fs::write(path, bytes)?;
    Ok(())
}

pub(crate) fn debug_decomp_bundle_json(
    binary: &LoadedBinary,
    requested_address: Option<u64>,
    func: &FunctionInfo,
    build_stats: Option<&NirBuildStats>,
    hint_stats: Option<&NirHintStats>,
    native_timing: Option<&serde_json::Value>,
    failed_hard: bool,
    assembly_fallback_no_stats: bool,
) -> serde_json::Value {
    let binary_json = serde_json::json!({
        "path": binary.path.as_str(),
        "format": binary.format.as_str(),
        "language_id": binary
            .sleigh_language_id()
            .unwrap_or(binary.arch_spec.as_str()),
        "compiler_id": binary
            .get_ghidra_compiler_id()
            .unwrap_or_else(|| "unknown".to_string()),
        "image_base": format!("0x{:x}", binary.image_base),
    });

    let resolved = func.address;
    let requested = requested_address.unwrap_or(resolved);
    let function_json = serde_json::json!({
        "requested_address": format!("0x{:x}", requested),
        "resolved_address": format!("0x{:x}", resolved),
        "name": func.name,
        "size": func.size,
        "function_origin": func.origin,
        "is_import": func.is_import,
        "is_export": func.is_export,
        "is_thunk_like": func.is_thunk_like,
    });

    let stage_status = derive_stage_status(failed_hard, assembly_fallback_no_stats, build_stats);

    let stage_metrics = build_stats.map(|s| {
        serde_json::json!({
            "validated_pcode_op_count": s.validated_pcode_op_count,
            "invalid_pcode_shape_count": s.invalid_pcode_shape_count,
            "build_duration_ms": s.build_duration_ms,
            "normalize_duration_ms": s.normalize_duration_ms,
            "structuring_duration_ms": s.structuring_duration_ms,
            "render_duration_ms": s.render_duration_ms,
        })
    });

    let quality_evidence = build_stats.map(quality_evidence_subset);

    let owner_buckets: Vec<String> = build_stats
        .map(owner_buckets_from_stats)
        .unwrap_or_default();

    let mut root = serde_json::json!({
        "schema_version": 1u32,
        "binary": binary_json,
        "function": function_json,
        "stage_status": stage_status,
        "owner_buckets": owner_buckets,
    });

    if let Some(v) = stage_metrics {
        root["stage_metrics"] = v;
    }
    if let Some(v) = quality_evidence {
        root["quality_evidence"] = v;
    }
    if let Some(h) = hint_stats {
        if let Ok(v) = serde_json::to_value(h) {
            root["preview_hint_stats"] = v;
        }
    }
    if let Some(t) = native_timing {
        root["native_timing"] = t.clone();
    }

    root
}

fn derive_stage_status(
    failed_hard: bool,
    assembly_fallback_no_stats: bool,
    stats: Option<&NirBuildStats>,
) -> serde_json::Value {
    if failed_hard {
        return serde_json::json!({
            "load": "ok",
            "decode": "failed",
            "raw_pcode": "failed",
            "nir_build": "failed",
            "normalize": "skipped",
            "structuring": "skipped",
            "render": "skipped",
        });
    }
    if assembly_fallback_no_stats {
        return serde_json::json!({
            "load": "ok",
            "decode": "partial",
            "raw_pcode": "failed",
            "nir_build": "failed",
            "normalize": "skipped",
            "structuring": "skipped",
            "render": "ok",
        });
    }
    let Some(s) = stats else {
        return serde_json::json!({
            "load": "ok",
            "decode": "ok",
            "raw_pcode": "partial",
            "nir_build": "skipped",
            "normalize": "skipped",
            "structuring": "skipped",
            "render": "ok",
        });
    };

    let validated = s.validated_pcode_op_count;
    let decode_st = if validated > 0 { "ok" } else { "partial" };
    let nir_st = if validated > 0 { "ok" } else { "partial" };
    let norm_st = if s.normalize_duration_ms > 0 || validated > 0 {
        "ok"
    } else {
        "partial"
    };
    let struct_st = if structuring_partial_heuristic(s) {
        "partial"
    } else {
        "ok"
    };
    let render_st = if s.render_duration_ms > 0 || validated > 0 {
        "ok"
    } else {
        "partial"
    };

    serde_json::json!({
        "load": "ok",
        "decode": decode_st,
        "raw_pcode": decode_st,
        "nir_build": nir_st,
        "normalize": norm_st,
        "structuring": struct_st,
        "render": render_st,
    })
}

fn structuring_partial_heuristic(s: &NirBuildStats) -> bool {
    if s.forced_linear_structuring_count > 0 {
        return true;
    }
    if s.region_emit_ready_failed_count > 0 {
        return true;
    }
    let bg_rej = s.blockgraph_region_rejected_missing_follow_count
        + s.blockgraph_region_rejected_must_emit_label_count
        + s.blockgraph_region_rejected_middle_ref_count
        + s.blockgraph_region_rejected_external_ref_count
        + s.blockgraph_region_rejected_join_owner_conflict_count
        + s.blockgraph_region_rejected_nonterminal_join_count
        + s.blockgraph_region_rejected_follow_owner_conflict_count
        + s.blockgraph_region_rejected_emit_ready_count
        + s.blockgraph_region_rejected_irreducible_count;
    if bg_rej > 0 {
        return true;
    }
    let sr = s.structuring_reason_region_legality_count
        + s.structuring_reason_follow_failure_count
        + s.structuring_reason_irreducible_count
        + s.structuring_reason_loop_exit_count
        + s.structuring_reason_switch_shape_count
        + s.structuring_reason_budget_count;
    if sr > 0 {
        return true;
    }
    let gt = s.guarded_tail_rejected_missing_terminal_join_count
        + s.guarded_tail_rejected_side_entry_conflict_count
        + s.guarded_tail_rejected_alias_interleave_conflict_count
        + s.guarded_tail_rejected_ambiguous_follow_count
        + s.guarded_tail_rejected_side_effectful_callee_count;
    gt > 0
}

fn quality_evidence_subset(s: &NirBuildStats) -> serde_json::Value {
    #[derive(Serialize)]
    struct Evidence {
        validated_pcode_op_count: usize,
        invalid_pcode_shape_count: usize,
        call_target_import_resolved_count: usize,
        call_target_direct_symbol_resolved_count: usize,
        call_target_unresolved_sub_fallback_count: usize,
        call_target_context_missing_count: usize,
        call_target_indirect_const_resolved_count: usize,
        call_target_iat_slot_resolved_count: usize,
        call_target_indirect_load_resolved_count: usize,
        call_target_unresolved_no_exact_identity_count: usize,
        call_signature_refined_count: usize,
        prototype_summary_refined_count: usize,
        prototype_summary_round_count: usize,
        call_prototype_signature_missing_count: usize,
        call_prototype_unknown_target_kept_count: usize,
        typed_fact_evidence_count: usize,
        typed_fact_conflict_count: usize,
        surface_binding_promoted_count: usize,
        object_root_recovered_count: usize,
        object_shape_recovered_count: usize,
        typed_object_shape_refined_count: usize,
        object_root_fact_promotion_count: usize,
        materialization_stabilized_count: usize,
        replacement_plan_rejected_alias_unsafe_count: usize,
        replacement_plan_rejected_missing_merge_count: usize,
        representative_downgrade_count: usize,
        forced_linear_structuring_count: usize,
        structuring_scc_component_count: usize,
        structuring_irreducible_scc_count: usize,
        region_proof_candidate_count: usize,
        region_proof_completed_count: usize,
        region_emit_ready_failed_count: usize,
        blockgraph_region_candidate_count: usize,
        blockgraph_region_complete_count: usize,
        blockgraph_region_rejected_missing_follow_count: usize,
        blockgraph_region_rejected_must_emit_label_count: usize,
        blockgraph_region_rejected_middle_ref_count: usize,
        blockgraph_region_rejected_external_ref_count: usize,
        blockgraph_region_rejected_join_owner_conflict_count: usize,
        blockgraph_region_rejected_nonterminal_join_count: usize,
        blockgraph_region_rejected_follow_owner_conflict_count: usize,
        blockgraph_region_rejected_emit_ready_count: usize,
        blockgraph_region_rejected_irreducible_count: usize,
        guarded_tail_rejected_missing_terminal_join_count: usize,
        guarded_tail_rejected_side_entry_conflict_count: usize,
        guarded_tail_rejected_alias_interleave_conflict_count: usize,
        guarded_tail_rejected_ambiguous_follow_count: usize,
        guarded_tail_rejected_side_effectful_callee_count: usize,
    }

    serde_json::to_value(Evidence {
        validated_pcode_op_count: s.validated_pcode_op_count,
        invalid_pcode_shape_count: s.invalid_pcode_shape_count,
        call_target_import_resolved_count: s.call_target_import_resolved_count,
        call_target_direct_symbol_resolved_count: s.call_target_direct_symbol_resolved_count,
        call_target_unresolved_sub_fallback_count: s.call_target_unresolved_sub_fallback_count,
        call_target_context_missing_count: s.call_target_context_missing_count,
        call_target_indirect_const_resolved_count: s.call_target_indirect_const_resolved_count,
        call_target_iat_slot_resolved_count: s.call_target_iat_slot_resolved_count,
        call_target_indirect_load_resolved_count: s.call_target_indirect_load_resolved_count,
        call_target_unresolved_no_exact_identity_count: s.call_target_unresolved_no_exact_identity_count,
        call_signature_refined_count: s.call_signature_refined_count,
        prototype_summary_refined_count: s.prototype_summary_refined_count,
        prototype_summary_round_count: s.prototype_summary_round_count,
        call_prototype_signature_missing_count: s.call_prototype_signature_missing_count,
        call_prototype_unknown_target_kept_count: s.call_prototype_unknown_target_kept_count,
        typed_fact_evidence_count: s.typed_fact_evidence_count,
        typed_fact_conflict_count: s.typed_fact_conflict_count,
        surface_binding_promoted_count: s.surface_binding_promoted_count,
        object_root_recovered_count: s.object_root_recovered_count,
        object_shape_recovered_count: s.object_shape_recovered_count,
        typed_object_shape_refined_count: s.typed_object_shape_refined_count,
        object_root_fact_promotion_count: s.object_root_fact_promotion_count,
        materialization_stabilized_count: s.materialization_stabilized_count,
        replacement_plan_rejected_alias_unsafe_count: s.replacement_plan_rejected_alias_unsafe_count,
        replacement_plan_rejected_missing_merge_count: s.replacement_plan_rejected_missing_merge_count,
        representative_downgrade_count: s.representative_downgrade_count,
        forced_linear_structuring_count: s.forced_linear_structuring_count,
        structuring_scc_component_count: s.structuring_scc_component_count,
        structuring_irreducible_scc_count: s.structuring_irreducible_scc_count,
        region_proof_candidate_count: s.region_proof_candidate_count,
        region_proof_completed_count: s.region_proof_completed_count,
        region_emit_ready_failed_count: s.region_emit_ready_failed_count,
        blockgraph_region_candidate_count: s.blockgraph_region_candidate_count,
        blockgraph_region_complete_count: s.blockgraph_region_complete_count,
        blockgraph_region_rejected_missing_follow_count: s.blockgraph_region_rejected_missing_follow_count,
        blockgraph_region_rejected_must_emit_label_count: s.blockgraph_region_rejected_must_emit_label_count,
        blockgraph_region_rejected_middle_ref_count: s.blockgraph_region_rejected_middle_ref_count,
        blockgraph_region_rejected_external_ref_count: s.blockgraph_region_rejected_external_ref_count,
        blockgraph_region_rejected_join_owner_conflict_count: s.blockgraph_region_rejected_join_owner_conflict_count,
        blockgraph_region_rejected_nonterminal_join_count: s.blockgraph_region_rejected_nonterminal_join_count,
        blockgraph_region_rejected_follow_owner_conflict_count: s.blockgraph_region_rejected_follow_owner_conflict_count,
        blockgraph_region_rejected_emit_ready_count: s.blockgraph_region_rejected_emit_ready_count,
        blockgraph_region_rejected_irreducible_count: s.blockgraph_region_rejected_irreducible_count,
        guarded_tail_rejected_missing_terminal_join_count: s.guarded_tail_rejected_missing_terminal_join_count,
        guarded_tail_rejected_side_entry_conflict_count: s.guarded_tail_rejected_side_entry_conflict_count,
        guarded_tail_rejected_alias_interleave_conflict_count: s.guarded_tail_rejected_alias_interleave_conflict_count,
        guarded_tail_rejected_ambiguous_follow_count: s.guarded_tail_rejected_ambiguous_follow_count,
        guarded_tail_rejected_side_effectful_callee_count: s.guarded_tail_rejected_side_effectful_callee_count,
    })
    .unwrap_or_else(|_| serde_json::Value::Null)
}

fn owner_buckets_from_stats(s: &NirBuildStats) -> Vec<String> {
    let mut buckets = BTreeSet::new();
    if s.call_target_unresolved_sub_fallback_count > 0
        || s.call_target_unresolved_no_exact_identity_count > 0
        || s.call_target_context_missing_count > 0
    {
        buckets.insert("call_target_missing".to_string());
    }
    if s.call_prototype_signature_missing_count > 0 {
        buckets.insert("prototype_arity_missing".to_string());
    }
    if s.typed_fact_conflict_count > 0 || s.representative_downgrade_count > 0 {
        buckets.insert("type_surface_evidence".to_string());
    }
    if s.replacement_plan_rejected_alias_unsafe_count > 0
        || s.replacement_plan_rejected_missing_merge_count > 0
    {
        buckets.insert("replacement_plan_rejected".to_string());
    }
    if structuring_partial_heuristic(s) {
        buckets.insert("structuring_partial".to_string());
    }
    buckets.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_buckets_call_target_and_prototype() {
        let mut s = NirBuildStats::default();
        s.call_target_unresolved_sub_fallback_count = 1;
        s.call_prototype_signature_missing_count = 2;
        let b = owner_buckets_from_stats(&s);
        assert!(b.contains(&"call_target_missing".to_string()));
        assert!(b.contains(&"prototype_arity_missing".to_string()));
    }

    #[test]
    fn structuring_partial_heuristic_detects_forced_linear() {
        let mut s = NirBuildStats::default();
        s.forced_linear_structuring_count = 1;
        assert!(structuring_partial_heuristic(&s));
    }
}
