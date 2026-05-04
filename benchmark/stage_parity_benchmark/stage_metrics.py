from __future__ import annotations

from typing import Any


def _safe_int(value: Any, default: int = 0) -> int:
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _extract_metrics(source: dict[str, Any], keys: tuple[str, ...]) -> dict[str, int]:
    return {key: _safe_int(source.get(key), 0) for key in keys}


def _derive_stage_status(preview_build_stats: dict[str, Any] | None) -> dict[str, str]:
    if not isinstance(preview_build_stats, dict):
        return {
            "nir_build": "missing",
            "normalize": "missing",
            "structuring": "missing",
        }
    invalid = _safe_int(preview_build_stats.get("invalid_pcode_shape_count"), 0)
    return {
        "nir_build": "error" if invalid > 0 else "ok",
        "normalize": "ok",
        "structuring": "ok",
    }


def build_stage_report(entry: dict[str, Any]) -> dict[str, Any]:
    preview = entry.get("preview_build_stats")
    if not isinstance(preview, dict):
        preview = {}

    code_metrics = entry.get("metrics") if isinstance(entry.get("metrics"), dict) else {}

    nir_build_keys = (
        "validated_pcode_op_count",
        "invalid_pcode_shape_count",
        "raw_pcode_compat_import_count",
        "call_target_import_resolved_count",
        "call_target_direct_symbol_resolved_count",
        "call_target_unresolved_sub_fallback_count",
        "call_target_context_missing_count",
        "unsupported_indirect_call_count",
        "unsupported_indirect_control_count",
    )
    normalize_keys = (
        "call_signature_refined_count",
        "prototype_summary_refined_count",
        "prototype_summary_round_count",
        "call_prototype_signature_missing_count",
        "call_prototype_unknown_target_kept_count",
        "object_root_recovered_count",
        "object_shape_recovered_count",
        "typed_object_shape_refined_count",
        "surface_binding_promoted_count",
        "materialization_stabilized_count",
        "replacement_plan_rejected_alias_unsafe_count",
        "replacement_plan_rejected_missing_merge_count",
        "replacement_plan_rejected_representative_root_attribution_count",
        "replacement_plan_rejected_temp_only_representative_lifecycle_count",
        "replacement_plan_rejected_dead_temp_representative_count",
    )
    structuring_keys = (
        "forced_linear_structuring_count",
        "structuring_scc_component_count",
        "structuring_irreducible_scc_count",
        "region_proof_candidate_count",
        "region_proof_completed_count",
        "region_emit_ready_failed_count",
        "blockgraph_region_candidate_count",
        "blockgraph_region_complete_count",
        "blockgraph_region_rejected_missing_follow_count",
        "blockgraph_region_rejected_must_emit_label_count",
        "blockgraph_region_rejected_middle_ref_count",
        "blockgraph_region_rejected_external_ref_count",
        "blockgraph_region_rejected_join_owner_conflict_count",
        "blockgraph_region_rejected_nonterminal_join_count",
        "blockgraph_region_rejected_follow_owner_conflict_count",
        "blockgraph_region_rejected_emit_ready_count",
        "blockgraph_region_rejected_irreducible_count",
        "guarded_tail_candidate_count",
        "guarded_tail_promoted_count",
        "guarded_tail_rejected_alias_interleave_conflict_count",
    )

    nir_build = _extract_metrics(preview, nir_build_keys)
    normalize = _extract_metrics(preview, normalize_keys)
    structuring = _extract_metrics(preview, structuring_keys)

    normalize.update(
        {
            "unknown_type_var_count": _safe_int(code_metrics.get("unknown_type_var_count"), 0),
            "param_name_generic_count": _safe_int(code_metrics.get("param_name_generic_count"), 0),
            "local_name_generic_count": _safe_int(code_metrics.get("local_name_generic_count"), 0),
        }
    )
    structuring.update(
        {
            "goto_count": _safe_int(code_metrics.get("goto_count"), 0),
            "top_level_label_count": _safe_int(code_metrics.get("top_level_label_count"), 0),
        }
    )

    owner_bucket: list[str] = []
    if (
        nir_build.get("call_target_unresolved_sub_fallback_count", 0) > 0
        or nir_build.get("call_target_context_missing_count", 0) > 0
    ):
        owner_bucket.append("call_target_missing")
    if (
        normalize.get("call_prototype_signature_missing_count", 0) > 0
        or normalize.get("call_prototype_unknown_target_kept_count", 0) > 0
    ):
        owner_bucket.append("prototype_arity_missing")
    if (
        nir_build.get("unsupported_indirect_call_count", 0) > 0
        or nir_build.get("unsupported_indirect_control_count", 0) > 0
    ):
        owner_bucket.append("unsupported_indirect")
    if (
        normalize.get("unknown_type_var_count", 0) > 0
        or normalize.get("param_name_generic_count", 0) > 0
        or normalize.get("local_name_generic_count", 0) > 0
    ):
        owner_bucket.append("type_name_surface")
    if (
        structuring.get("goto_count", 0) > 0
        or structuring.get("top_level_label_count", 0) > 0
        or structuring.get("forced_linear_structuring_count", 0) > 0
        or structuring.get("region_emit_ready_failed_count", 0) > 0
    ):
        owner_bucket.append("control_flow_goto_heavy")

    status = _derive_stage_status(preview)
    return {
        "status": status,
        "nir_build": nir_build,
        "normalize": normalize,
        "structuring": structuring,
        "owner_bucket": owner_bucket,
    }
