#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

try:
    from rapidfuzz import fuzz as _rapidfuzz_fuzz
except Exception:
    _rapidfuzz_fuzz = None


def _safe_float(value: Any, default: float = 0.0) -> float:
    try:
        if value is None:
            return default
        return float(value)
    except (TypeError, ValueError):
        return default


def _safe_int(value: Any, default: int = 0) -> int:
    try:
        if value is None:
            return default
        return int(value)
    except (TypeError, ValueError):
        return default


def _lookup_path(data: dict[str, Any], path: tuple[str, ...], default: Any = None) -> Any:
    cur: Any = data
    for key in path:
        if not isinstance(cur, dict):
            return default
        cur = cur.get(key)
        if cur is None:
            return default
    return cur


def _canonical_indirect_flags(preview_build_stats: dict[str, Any]) -> dict[str, Any]:
    unsupported_indirect = _safe_int(
        preview_build_stats.get("unsupported_indirect_control_count"), 0
    )
    unsupported_call = _safe_int(
        preview_build_stats.get("unsupported_indirect_call_count"), 0
    )
    unsupported_external = _safe_int(
        preview_build_stats.get("unsupported_external_target_count"), 0
    )
    preserved = _safe_int(
        preview_build_stats.get("indirect_surface_preserved_count"), 0
    )
    dispatcher = _safe_int(
        preview_build_stats.get("dispatcher_shape_recovered_count"), 0
    )
    refined = _safe_int(
        preview_build_stats.get("indirect_target_set_refined_count"), 0
    )
    return {
        "has_unresolved_unsupported_indirect": unsupported_indirect > 0
        or unsupported_call > 0
        or unsupported_external > 0,
        "has_preserved_indirect_surface": preserved > 0,
        "has_dispatcher_recovery": dispatcher > 0,
        "has_indirect_target_proof": refined > 0,
    }

def find_repo_root() -> Path:
    current = Path(__file__).resolve()
    for parent in [current.parent, *current.parents]:
        if (parent / "Cargo.toml").exists() and (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not locate repository root from script path")


ROOT_DIR = find_repo_root()
DEFAULT_RESULTS_DIR = ROOT_DIR / "benchmark" / "artifacts" / "full_benchmark"
DEFAULT_GHIDRA_DIRS = (
    ROOT_DIR / "vendor" / "ghidra" / "ghidra-Ghidra_12.0.4_build",
    ROOT_DIR / "ghidra-Ghidra_12.0.4_build",
)
BASE_TYPES_JSON = ROOT_DIR / "crates" / "fission-signatures" / "data" / "win_types" / "base_types.json"

BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT_RE = re.compile(r"//.*?$", re.MULTILINE)
HEX_RE = re.compile(r"0x[0-9a-fA-F]+")
AUTO_FUNC_RE = re.compile(r"\b(?:FUN|sub)_[0-9a-fA-F]+\b")
AUTO_SYMBOL_RE = re.compile(r"\b(?:DAT|LAB|UNK|EXT)_[0-9a-fA-F]+\b")
AUTO_VAR_RE = re.compile(
    r"\b(?:local|param|extraout|in|unaff|uStack|puStack|iVar|uVar|bVar|cVar|sVar|lVar|auStack)"
    r"[A-Za-z0-9_]*\b"
)
SYNTHETIC_FAILURE_PREFIX = "// Decompilation failed:"
SIMILARITY_BACKEND = "rapidfuzz" if _rapidfuzz_fuzz is not None else "difflib"

ROW_FIDELITY_TARGETS: tuple[tuple[str, str], ...] = (
    ("0x140001160", "primary"),
    ("0x140008900", "secondary"),
    ("0x140007da0", "canary"),
    ("0x140008090", "canary"),
    ("0x140006ef0", "canary"),
    ("0x140006c20", "canary"),
    ("0x140006fe0", "canary"),
)

CORPUS_ROLE_ALIASES: dict[str, str] = {
    "secondary": "release_candidate",
    "release": "release_candidate",
    "candidate": "release_candidate",
    "smoke": "smoke_sentinel",
    "diagnostic": "diagnostic_only",
}
CORPUS_RELEASE_WEIGHT_ROLES = frozenset({"primary_canary", "release_candidate"})
CORPUS_REPORT_ONLY_ROLES = frozenset({"smoke_sentinel", "diagnostic_only"})
CORPUS_SUITE_TIERS = frozenset({"smoke", "release", "parity"})
CORPUS_GATE_MODES = frozenset({"advisory", "blocking"})
DEFAULT_DYNAMIC_WATCHLIST_LIMIT = 5

OWNER_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    ("procedure_summary_contracted_count", "procedure_summary_contracted"),
    ("procedure_summary_tail_wrapper_count", "procedure_summary_tail_wrapper"),
    ("procedure_summary_import_thunk_count", "procedure_summary_import_thunk"),
    ("replacement_plan_rejected_alias_unsafe_count", "alias_unsafe"),
    ("replacement_plan_rejected_missing_merge_count", "missing_merge"),
    ("replacement_plan_rejected_representative_root_attribution_count", "representative_root"),
    ("replacement_plan_rejected_temp_only_representative_lifecycle_count", "temp_only_lifecycle"),
    ("replacement_plan_rejected_dead_temp_representative_count", "dead_temp"),
    ("representative_downgrade_count", "representative_downgrade"),
    ("representative_downgrade_no_aliassafe_source_count", "downgrade_no_aliassafe_source"),
    ("representative_downgrade_join_conflict_count", "downgrade_join_conflict"),
    ("materialization_stabilized_count", "materialization_stabilized"),
)

SHAPE_DRIFT_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    ("goto_total", "goto_total"),
    ("top_level_label_total", "top_level_label_total"),
    ("generic_local_name_sum", "generic_local_name_sum"),
    ("generic_param_name_sum", "generic_param_name_sum"),
    ("unknown_type_var_total", "unknown_type_var_total"),
    ("ptr_offset_total", "ptr_offset_total"),
    ("index_expr_total", "index_expr_total"),
    ("heuristic_avg_line_length_mean", "heuristic_avg_line_length_mean"),
    ("heuristic_max_brace_nesting_mean", "heuristic_max_brace_nesting_mean"),
    ("synthetic_helper_call_total", "synthetic_helper_call_total"),
)

BASELINE_GATE_SHAPE_KEYS = frozenset(
    {
        "generic_local_name_sum",
        "generic_param_name_sum",
        "heuristic_max_brace_nesting_mean",
        "synthetic_helper_call_total",
    }
)

CORPUS_GATE_SHAPE_KEYS = frozenset(
    {
        "generic_local_name_sum",
        "generic_param_name_sum",
        "synthetic_helper_call_total",
    }
)

SELECTED_NORMALIZE_PASSES = (
    "wide_dead_assignment",
    "sccp",
    "jump_resolver",
    "break_continue_recovery",
)

GHIDRA_ACTION_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    ("ghidra_action_stage_count", "stage_count"),
    ("ghidra_action_funcdata_build_count", "funcdata_build"),
    ("ghidra_action_heritage_value_recovery_count", "heritage_value_recovery"),
    ("ghidra_action_normalize_count", "normalize"),
    ("ghidra_action_prototype_types_count", "prototype_types"),
    ("ghidra_action_blockgraph_structuring_count", "blockgraph_structuring"),
    ("ghidra_action_printc_count", "printc"),
    ("ghidra_clean_room_pipeline_complete_count", "pipeline_complete"),
)

MIR_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    ("mir_enabled_count", "enabled"),
    ("mir_function_count", "function"),
    ("mir_block_count", "block"),
    ("mir_value_count", "value"),
    ("mir_memory_region_count", "memory_region"),
    ("mir_join_proof_count", "join_proof"),
    ("mir_region_proof_count", "region_proof"),
    ("mir_projection_duration_ms", "projection_duration_ms"),
    ("mir_blockgraph_admission_enabled_count", "blockgraph_admission_enabled"),
    (
        "mir_blockgraph_irreducible_budget_bypass_count",
        "blockgraph_irreducible_budget_bypass",
    ),
    (
        "mir_blockgraph_extreme_budget_blocked_count",
        "blockgraph_extreme_budget_blocked",
    ),
)

BLOCKGRAPH_REGION_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    ("blockgraph_region_candidate_count", "candidate"),
    ("blockgraph_region_complete_count", "complete"),
    ("blockgraph_region_rejected_missing_follow_count", "rejected_missing_follow"),
    ("blockgraph_region_rejected_must_emit_label_count", "rejected_must_emit_label"),
    ("blockgraph_region_rejected_middle_ref_count", "rejected_middle_ref"),
    ("blockgraph_region_rejected_external_ref_count", "rejected_external_ref"),
    (
        "blockgraph_region_rejected_join_owner_conflict_count",
        "rejected_join_owner_conflict",
    ),
    ("blockgraph_region_rejected_nonterminal_join_count", "rejected_nonterminal_join"),
    (
        "blockgraph_region_rejected_follow_owner_conflict_count",
        "rejected_follow_owner_conflict",
    ),
    ("blockgraph_region_rejected_emit_ready_count", "rejected_emit_ready"),
    ("blockgraph_region_rejected_irreducible_count", "rejected_irreducible"),
)

ALIAS_INTERLEAVE_METRIC_SPECS: tuple[tuple[str, str], ...] = (
    (
        "guarded_tail_rejected_alias_interleave_conflict_count",
        "alias_interleave_conflict",
    ),
    (
        "canonicalization_failed_alias_has_nonlocal_ref_count",
        "alias_has_nonlocal_ref",
    ),
    (
        "canonicalization_failed_alias_has_nonlocal_ref_external_before_count",
        "alias_has_nonlocal_ref_external_before",
    ),
    (
        "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count",
        "alias_has_nonlocal_ref_nested_before",
    ),
    (
        "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count",
        "alias_has_nonlocal_ref_post_segment_ref",
    ),
    (
        "canonicalization_failed_alias_not_fallthrough_count",
        "alias_not_fallthrough",
    ),
    (
        "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count",
        "alias_not_fallthrough_top_level_after_label",
    ),
    (
        "canonicalization_failed_alias_not_fallthrough_nested_after_label_count",
        "alias_not_fallthrough_nested_after_label",
    ),
    (
        "canonicalization_failed_alias_has_multiple_internal_predecessors_count",
        "alias_has_multiple_internal_predecessors",
    ),
    (
        "canonicalization_failed_payload_crosses_join_count",
        "payload_crosses_join",
    ),
)

GIANT_RENDERED_CODE_THRESHOLD = 100000
GIANT_REPLACEMENT_THRESHOLD = 10000
GIANT_MATERIALIZATION_THRESHOLD = 10000
GIANT_STRUCTURING_SCC_THRESHOLD = 128
GIANT_STAGE_DOMINANCE_THRESHOLD = 0.5
MAX_PATHOLOGICAL_EXAMPLES = 10


def _extract_named_metrics(
    source: dict[str, Any],
    specs: tuple[tuple[str, str], ...],
) -> dict[str, float]:
    return {
        alias: _safe_float(source.get(key, 0.0), 0.0)
        for key, alias in specs
    }


def _extract_owner_metrics_from_engine_summary(engine_summary: dict[str, Any]) -> dict[str, float]:
    return _extract_named_metrics(engine_summary, OWNER_METRIC_SPECS)


def _extract_shape_drift_metrics_from_engine_summary(engine_summary: dict[str, Any]) -> dict[str, float]:
    return _extract_named_metrics(engine_summary, SHAPE_DRIFT_METRIC_SPECS)


def _extract_ghidra_action_metrics(preview_build_stats: dict[str, Any]) -> dict[str, float]:
    source = preview_build_stats if isinstance(preview_build_stats, dict) else {}
    return _extract_named_metrics(source, GHIDRA_ACTION_METRIC_SPECS)


def _extract_mir_metrics(preview_build_stats: dict[str, Any]) -> dict[str, float]:
    source = preview_build_stats if isinstance(preview_build_stats, dict) else {}
    return _extract_named_metrics(source, MIR_METRIC_SPECS)


def _extract_blockgraph_region_metrics(preview_build_stats: dict[str, Any]) -> dict[str, float]:
    source = preview_build_stats if isinstance(preview_build_stats, dict) else {}
    return _extract_named_metrics(source, BLOCKGRAPH_REGION_METRIC_SPECS)


def _extract_alias_interleave_metrics(preview_build_stats: dict[str, Any]) -> dict[str, float]:
    source = preview_build_stats if isinstance(preview_build_stats, dict) else {}
    return _extract_named_metrics(source, ALIAS_INTERLEAVE_METRIC_SPECS)


def _aggregate_ghidra_action_metrics_from_entries(
    entries: dict[str, dict[str, Any]] | None,
) -> dict[str, float]:
    totals = {alias: 0.0 for _key, alias in GHIDRA_ACTION_METRIC_SPECS}
    for entry in (entries or {}).values():
        if not isinstance(entry, dict):
            continue
        preview_build_stats = entry.get("preview_build_stats", {})
        if not isinstance(preview_build_stats, dict):
            continue
        for key, alias in GHIDRA_ACTION_METRIC_SPECS:
            totals[alias] += _safe_float(preview_build_stats.get(key), 0.0)
    return dict(sorted(totals.items()))


def _aggregate_mir_metrics_from_entries(
    entries: dict[str, dict[str, Any]] | None,
) -> dict[str, float]:
    totals = {alias: 0.0 for _key, alias in MIR_METRIC_SPECS}
    for entry in (entries or {}).values():
        if not isinstance(entry, dict):
            continue
        preview_build_stats = entry.get("preview_build_stats", {})
        if not isinstance(preview_build_stats, dict):
            continue
        for key, alias in MIR_METRIC_SPECS:
            totals[alias] += _safe_float(preview_build_stats.get(key), 0.0)
    return dict(sorted(totals.items()))


def _aggregate_blockgraph_region_metrics_from_entries(
    entries: dict[str, dict[str, Any]] | None,
) -> dict[str, float]:
    totals = {alias: 0.0 for _key, alias in BLOCKGRAPH_REGION_METRIC_SPECS}
    for entry in (entries or {}).values():
        if not isinstance(entry, dict):
            continue
        preview_build_stats = entry.get("preview_build_stats", {})
        if not isinstance(preview_build_stats, dict):
            continue
        for key, alias in BLOCKGRAPH_REGION_METRIC_SPECS:
            totals[alias] += _safe_float(preview_build_stats.get(key), 0.0)
    return dict(sorted(totals.items()))


def _aggregate_alias_interleave_metrics_from_entries(
    entries: dict[str, dict[str, Any]] | None,
) -> dict[str, float]:
    totals = {alias: 0.0 for _key, alias in ALIAS_INTERLEAVE_METRIC_SPECS}
    for entry in (entries or {}).values():
        if not isinstance(entry, dict):
            continue
        preview_build_stats = entry.get("preview_build_stats", {})
        if not isinstance(preview_build_stats, dict):
            continue
        for key, alias in ALIAS_INTERLEAVE_METRIC_SPECS:
            totals[alias] += _safe_float(preview_build_stats.get(key), 0.0)
    return dict(sorted(totals.items()))


def _extract_selected_normalize_pass_metrics(
    preview_build_stats: dict[str, Any],
) -> dict[str, dict[str, float]]:
    pass_metrics = preview_build_stats.get("pass_metrics", {}) if isinstance(preview_build_stats, dict) else {}
    if not isinstance(pass_metrics, dict):
        return {}
    selected: dict[str, dict[str, float]] = {}
    for pass_name in SELECTED_NORMALIZE_PASSES:
        raw = pass_metrics.get(pass_name, {})
        if not isinstance(raw, dict):
            raw = {}
        selected[pass_name] = {
            "total_time_ms": _safe_float(raw.get("total_time_ms"), 0.0),
            "total_invocations": _safe_float(raw.get("total_invocations"), 0.0),
            "changed_count": _safe_float(raw.get("changed_count"), 0.0),
        }
    return selected


def _flatten_selected_normalize_pass_metrics(
    selected_metrics: dict[str, dict[str, float]],
) -> dict[str, float]:
    flattened: dict[str, float] = {}
    for pass_name in SELECTED_NORMALIZE_PASSES:
        metrics = selected_metrics.get(pass_name, {})
        flattened[f"{pass_name}_total_time_ms"] = _safe_float(metrics.get("total_time_ms"), 0.0)
        flattened[f"{pass_name}_total_invocations"] = _safe_float(
            metrics.get("total_invocations"), 0.0
        )
        flattened[f"{pass_name}_changed_count"] = _safe_float(metrics.get("changed_count"), 0.0)
    return _normalize_metric_map_for_json(flattened)


def _merge_normalize_pass_metric_totals(
    per_binary_metrics: dict[str, dict[str, float]],
) -> dict[str, float]:
    totals: dict[str, float] = {}
    for metrics in per_binary_metrics.values():
        for key, value in metrics.items():
            totals[key] = totals.get(key, 0.0) + _safe_float(value, 0.0)
    return _normalize_metric_map_for_json(totals)


def _merge_named_metric_totals(metric_maps: dict[str, dict[str, float]]) -> dict[str, float]:
    totals: dict[str, float] = {}
    for metrics in metric_maps.values():
        for key, value in metrics.items():
            totals[key] = totals.get(key, 0.0) + _safe_float(value, 0.0)
    return {
        key: int(value) if abs(value - round(value)) <= 1e-9 else round(value, 6)
        for key, value in sorted(totals.items())
    }


def _merge_cpu_metric_totals(metric_maps: dict[str, dict[str, float]]) -> dict[str, float]:
    totals: dict[str, float] = {}
    max_keys = {
        "process_cpu_utilization_pct",
        "process_effective_parallelism",
        "worker_count",
        "available_parallelism",
    }
    mean_keys = {"func_per_cpu_second"}
    mean_acc: dict[str, list[float]] = {key: [] for key in mean_keys}

    for metrics in metric_maps.values():
        for key, value in metrics.items():
            value_f = _safe_float(value, 0.0)
            if key in max_keys:
                totals[key] = max(totals.get(key, 0.0), value_f)
            elif key in mean_keys:
                if value_f > 0.0:
                    mean_acc[key].append(value_f)
            else:
                totals[key] = totals.get(key, 0.0) + value_f

    for key, values in mean_acc.items():
        if values:
            totals[key] = sum(values) / len(values)

    return _normalize_metric_map_for_json(totals)


def _merge_count_maps(count_maps: dict[str, dict[str, int]]) -> dict[str, int]:
    totals: dict[str, int] = {}
    for metrics in count_maps.values():
        for key, value in metrics.items():
            totals[str(key)] = totals.get(str(key), 0) + _safe_int(value, 0)
    return dict(sorted(totals.items()))


def _normalize_metric_map_for_json(metrics: dict[str, float]) -> dict[str, float]:
    return {
        key: int(value) if abs(value - round(value)) <= 1e-9 else round(value, 6)
        for key, value in sorted(metrics.items())
    }


def _extract_stage_cost_metrics(preview_build_stats: dict[str, Any]) -> dict[str, float]:
    preview = preview_build_stats if isinstance(preview_build_stats, dict) else {}
    return _normalize_metric_map_for_json(
        {
            "build_duration_ms": _safe_float(preview.get("build_duration_ms"), 0.0),
            "normalize_duration_ms": _safe_float(preview.get("normalize_duration_ms"), 0.0),
            "structuring_duration_ms": _safe_float(
                preview.get("structuring_duration_ms"), 0.0
            ),
            "render_duration_ms": _safe_float(preview.get("render_duration_ms"), 0.0),
            "rendered_code_len": _safe_float(preview.get("rendered_code_len"), 0.0),
            "max_structuring_scc_component_size": _safe_float(
                preview.get("max_structuring_scc_component_size"), 0.0
            ),
        }
    )


def _derive_giant_function_speed_family(entry: dict[str, Any]) -> dict[str, Any]:
    preview_build_stats = entry.get("preview_build_stats") or {}
    name = str(entry.get("name", "") or "")
    size = _safe_int(entry.get("size"), -1)
    build_duration_ms = _safe_float(preview_build_stats.get("build_duration_ms"), 0.0)
    normalize_duration_ms = _safe_float(
        preview_build_stats.get("normalize_duration_ms"), 0.0
    )
    structuring_duration_ms = _safe_float(
        preview_build_stats.get("structuring_duration_ms"), 0.0
    )
    render_duration_ms = _safe_float(preview_build_stats.get("render_duration_ms"), 0.0)
    rendered_code_len = _safe_int(preview_build_stats.get("rendered_code_len"), 0)
    forced_linear_structuring_count = _safe_int(
        preview_build_stats.get("forced_linear_structuring_count"), 0
    )
    structuring_scc_component_count = _safe_int(
        preview_build_stats.get("structuring_scc_component_count"), 0
    )
    replacement_plan_candidate_count = _safe_int(
        preview_build_stats.get("replacement_plan_candidate_count"), 0
    )
    materialization_stabilized_count = _safe_int(
        preview_build_stats.get("materialization_stabilized_count"), 0
    )
    stage_total = normalize_duration_ms + structuring_duration_ms + render_duration_ms

    zero_size_runtime_wrapper = size == 0 and name == "register_frame_ctor"
    normalize_heavy = (
        stage_total > 0.0
        and normalize_duration_ms > (structuring_duration_ms + render_duration_ms)
        and normalize_duration_ms >= stage_total * GIANT_STAGE_DOMINANCE_THRESHOLD
    )
    structuring_heavy = (
        stage_total > 0.0
        and structuring_duration_ms >= stage_total * GIANT_STAGE_DOMINANCE_THRESHOLD
        and (
            forced_linear_structuring_count > 0
            or structuring_scc_component_count >= GIANT_STRUCTURING_SCC_THRESHOLD
        )
    )
    render_heavy = (
        stage_total > 0.0
        and render_duration_ms >= stage_total * GIANT_STAGE_DOMINANCE_THRESHOLD
        and rendered_code_len >= GIANT_RENDERED_CODE_THRESHOLD
    )
    replacement_explosion = (
        replacement_plan_candidate_count >= GIANT_REPLACEMENT_THRESHOLD
        or materialization_stabilized_count >= GIANT_MATERIALIZATION_THRESHOLD
    )

    active_reasons = sum(
        int(flag)
        for flag in (
            normalize_heavy,
            structuring_heavy,
            render_heavy,
            replacement_explosion,
        )
    )
    if zero_size_runtime_wrapper:
        family = "ZeroSizeRuntimeWrapper"
    elif active_reasons >= 2:
        family = "MixedGiantFunction"
    elif normalize_heavy:
        family = "NormalizeHeavy"
    elif structuring_heavy:
        family = "StructuringHeavy"
    elif render_heavy:
        family = "RenderHeavy"
    elif replacement_explosion:
        family = "ReplacementPlanExplosion"
    else:
        family = "UnknownGiantFunction"

    is_candidate = (
        zero_size_runtime_wrapper
        or normalize_heavy
        or structuring_heavy
        or render_heavy
        or replacement_explosion
    )
    return {
        "binary_id": entry.get("binary_id"),
        "address": entry.get("address"),
        "name": name,
        "size": size if size >= 0 else None,
        "build_duration_ms": round(build_duration_ms, 6),
        "normalize_duration_ms": round(normalize_duration_ms, 6),
        "structuring_duration_ms": round(structuring_duration_ms, 6),
        "render_duration_ms": round(render_duration_ms, 6),
        "rendered_code_len": rendered_code_len,
        "forced_linear_structuring_count": forced_linear_structuring_count,
        "structuring_scc_component_count": structuring_scc_component_count,
        "replacement_plan_candidate_count": replacement_plan_candidate_count,
        "materialization_stabilized_count": materialization_stabilized_count,
        "giant_function_speed_family": family,
        "is_candidate": is_candidate,
    }


def _build_giant_function_diagnostics(
    entries: dict[str, dict[str, Any]] | None,
    *,
    binary_id: str | None = None,
) -> dict[str, Any]:
    family_counts: dict[str, int] = {}
    examples: list[dict[str, Any]] = []
    max_rendered_code_len = 0
    max_structuring_scc_component_count = 0
    max_replacement_plan_candidate_count = 0
    max_materialization_stabilized_count = 0

    for raw_entry in (entries or {}).values():
        if not isinstance(raw_entry, dict):
            continue
        entry = dict(raw_entry)
        if binary_id is not None:
            entry["binary_id"] = binary_id
        derived = _derive_giant_function_speed_family(entry)
        max_rendered_code_len = max(
            max_rendered_code_len,
            _safe_int(derived.get("rendered_code_len"), 0),
        )
        max_structuring_scc_component_count = max(
            max_structuring_scc_component_count,
            _safe_int(derived.get("structuring_scc_component_count"), 0),
        )
        max_replacement_plan_candidate_count = max(
            max_replacement_plan_candidate_count,
            _safe_int(derived.get("replacement_plan_candidate_count"), 0),
        )
        max_materialization_stabilized_count = max(
            max_materialization_stabilized_count,
            _safe_int(derived.get("materialization_stabilized_count"), 0),
        )
        if not derived.get("is_candidate"):
            continue
        family = str(derived.get("giant_function_speed_family") or "UnknownGiantFunction")
        family_counts[family] = family_counts.get(family, 0) + 1
        examples.append(
            {
                key: value
                for key, value in derived.items()
                if key != "is_candidate"
            }
        )

    examples.sort(
        key=lambda row: (
            -_safe_float(row.get("build_duration_ms"), 0.0),
            -_safe_int(row.get("rendered_code_len"), 0),
            str(row.get("address", "")),
        )
    )
    return {
        "giant_function_candidates": len(examples),
        "giant_function_speed_family_counts": dict(sorted(family_counts.items())),
        "max_rendered_code_len": max_rendered_code_len,
        "max_structuring_scc_component_count": max_structuring_scc_component_count,
        "max_replacement_plan_candidate_count": max_replacement_plan_candidate_count,
        "max_materialization_stabilized_count": max_materialization_stabilized_count,
        "max_pathological_examples": examples[:MAX_PATHOLOGICAL_EXAMPLES],
    }


TARGET_STRUCTURING_ROW_NAMES = frozenset({"fibonacci", "fibonacci_memo"})
TARGET_STRUCTURING_ROW_ADDRESSES = frozenset({"0x140001470"})
TARGET_STRUCTURING_ROW_BINARY_ADDRESSES = frozenset(
    {
        ("test-functions", "0x140001470"),
        ("test_functions", "0x140001470"),
    }
)


def _index_row_fidelity_gate_rows_by_address(row_gate: dict[str, Any] | None) -> dict[str, dict[str, Any]]:
    indexed: dict[str, dict[str, Any]] = {}
    rows = row_gate.get("rows", []) if isinstance(row_gate, dict) else []
    for row in rows if isinstance(rows, list) else []:
        if not isinstance(row, dict):
            continue
        address = canonical_address(str(row.get("address") or "0x0"))
        indexed[address] = row
    return indexed


def _index_pairwise_comparisons_by_address(comparisons: Any) -> dict[str, dict[str, Any]]:
    indexed: dict[str, dict[str, Any]] = {}
    if isinstance(comparisons, dict):
        iterable = comparisons.values()
    elif isinstance(comparisons, list):
        iterable = comparisons
    else:
        iterable = []
    for row in iterable:
        if not isinstance(row, dict):
            continue
        address = canonical_address(str(row.get("address") or "0x0"))
        indexed[address] = row
    return indexed


def _hash_code_text(value: Any) -> str | None:
    text = str(value or "")
    if not text:
        return None
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _entry_code_sha256(entry: dict[str, Any] | None) -> str | None:
    if not isinstance(entry, dict):
        return None
    for key in ("code", "rendered_text", "text"):
        digest = _hash_code_text(entry.get(key))
        if digest:
            return digest
    return None


def _annotate_target_structuring_rows(
    rows: list[dict[str, Any]] | None,
    *,
    row_gate: dict[str, Any] | None = None,
    pairwise_comparisons: Any = None,
    engine_entries: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    gate_rows_by_address = _index_row_fidelity_gate_rows_by_address(row_gate)
    pairwise_by_address = _index_pairwise_comparisons_by_address(pairwise_comparisons)
    engine_entries = engine_entries if isinstance(engine_entries, dict) else {}
    annotated: list[dict[str, Any]] = []
    for row in list(rows or []):
        if not isinstance(row, dict):
            continue
        address = canonical_address(str(row.get("address") or "0x0"))
        annotated_row = dict(row)
        current_code_sha256 = _entry_code_sha256(engine_entries.get(address))
        if current_code_sha256:
            annotated_row["current_code_sha256"] = current_code_sha256
        pairwise_row = pairwise_by_address.get(address, {})
        gate_row = gate_rows_by_address.get(address, {})
        if isinstance(pairwise_row, dict):
            annotated_row["current_normalized_similarity"] = _safe_float(
                pairwise_row.get("normalized_similarity"),
                _safe_float(annotated_row.get("current_normalized_similarity"), 0.0),
            )
            annotated_row["current_fission_success"] = bool(
                pairwise_row.get("fission_success", annotated_row.get("current_fission_success", False))
            )
            annotated_row["current_pyghidra_success"] = bool(
                pairwise_row.get("pyghidra_success", annotated_row.get("current_pyghidra_success", False))
            )
        if isinstance(gate_row, dict):
            annotated_row["watchlist_role"] = str(
                gate_row.get("role") or annotated_row.get("watchlist_role") or ""
            )
            annotated_row["row_gate_status"] = str(
                gate_row.get("status") or annotated_row.get("row_gate_status") or "unknown"
            )
            annotated_row["previous_normalized_similarity"] = _safe_float(
                gate_row.get("previous_normalized_similarity"),
                _safe_float(annotated_row.get("previous_normalized_similarity"), 0.0),
            )
            annotated_row["current_normalized_similarity"] = _safe_float(
                gate_row.get("current_normalized_similarity"),
                _safe_float(annotated_row.get("current_normalized_similarity"), 0.0),
            )
            annotated_row["normalized_similarity_delta"] = _safe_float(
                gate_row.get("normalized_similarity_delta"),
                _safe_float(annotated_row.get("normalized_similarity_delta"), 0.0),
            )
            annotated_row["failure_reasons"] = [
                str(reason)
                for reason in list(gate_row.get("failure_reasons", []) or [])
                if str(reason)
            ]
            if gate_row.get("previous_code_sha256") is not None:
                annotated_row["previous_code_sha256"] = gate_row.get("previous_code_sha256")
            if gate_row.get("current_code_sha256") is not None:
                annotated_row["current_code_sha256"] = gate_row.get("current_code_sha256")
            if gate_row.get("code_changed") is not None:
                annotated_row["code_changed"] = bool(gate_row.get("code_changed"))
        annotated.append(annotated_row)
    return annotated


def _refresh_single_summary_target_rows_from_row_gate(
    benchmark: dict[str, Any],
    row_gate: dict[str, Any] | None = None,
) -> None:
    if not isinstance(benchmark, dict):
        return
    summary = benchmark.get("summary")
    if not isinstance(summary, dict):
        return
    annotated_rows = _annotate_target_structuring_rows(
        list(summary.get("target_structuring_rows", []) or []),
        row_gate=row_gate,
    )
    summary["target_structuring_rows"] = annotated_rows
    summary["unchanged_target_rows"] = [
        dict(row)
        for row in annotated_rows
        if str(row.get("row_gate_status") or "").strip() == "unchanged"
    ]


def _build_target_structuring_rows(
    entries: dict[str, dict[str, Any]] | None,
    *,
    binary_id: str | None = None,
    limit: int = 10,
) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for entry in (entries or {}).values():
        if not isinstance(entry, dict):
            continue
        name = str(entry.get("name") or "")
        address = canonical_address(str(entry.get("address") or "0x0"))
        matches_named_target = name in TARGET_STRUCTURING_ROW_NAMES
        matches_address_target = False
        if binary_id:
            matches_address_target = (str(binary_id), address) in TARGET_STRUCTURING_ROW_BINARY_ADDRESSES
        else:
            matches_address_target = address in TARGET_STRUCTURING_ROW_ADDRESSES
        if not matches_named_target and not matches_address_target:
            continue
        preview = entry.get("preview_build_stats") or {}
        if not isinstance(preview, dict):
            preview = {}
        rows.append(
            {
                "binary_id": binary_id or entry.get("binary_id"),
                "address": address,
                "name": name,
                "build_duration_ms": _safe_float(preview.get("build_duration_ms"), 0.0),
                "normalize_duration_ms": _safe_float(preview.get("normalize_duration_ms"), 0.0),
                "structuring_duration_ms": _safe_float(
                    preview.get("structuring_duration_ms"), 0.0
                ),
                "render_duration_ms": _safe_float(preview.get("render_duration_ms"), 0.0),
                "rendered_code_len": _safe_int(preview.get("rendered_code_len"), 0),
                "forced_linear_structuring_count": _safe_int(
                    preview.get("forced_linear_structuring_count"), 0
                ),
                "structuring_scc_component_count": _safe_int(
                    preview.get("structuring_scc_component_count"), 0
                ),
                "blockgraph_region_metrics": _normalize_metric_map_for_json(
                    _extract_blockgraph_region_metrics(preview)
                ),
                "current_code_sha256": _entry_code_sha256(entry),
            }
        )
    rows.sort(
        key=lambda row: (
            -_safe_float(row.get("structuring_duration_ms"), 0.0),
            str(row.get("binary_id") or ""),
            str(row.get("address") or ""),
        )
    )
    return rows[:limit]


def _derive_binary_arch(manifest_entry: dict[str, Any]) -> str:
    tags = {str(tag).lower() for tag in manifest_entry.get("tags", [])}
    if "x86" in tags:
        return "x86"
    if "x64" in tags:
        return "x64"

    binary_path = str(manifest_entry.get("binary_path", "")).lower()
    if "/samples/windows/x86/" in binary_path:
        return "x86"
    if "/samples/windows/x64/" in binary_path:
        return "x64"
    return "unknown"


def select_row_fidelity_targets(role_filter: str) -> list[tuple[str, str]]:
    if role_filter == "all":
        return list(ROW_FIDELITY_TARGETS)
    if role_filter == "canary-only":
        return [
            (address, role)
            for address, role in ROW_FIDELITY_TARGETS
            if role == "canary"
        ]
    if role_filter == "primary-secondary-only":
        return [
            (address, role)
            for address, role in ROW_FIDELITY_TARGETS
            if role in ("primary", "secondary")
        ]
    return list(ROW_FIDELITY_TARGETS)


def _normalize_row_target_pairs(
    rows: list[tuple[str, str]] | list[dict[str, Any]] | None,
) -> list[tuple[str, str]]:
    normalized: list[tuple[str, str]] = []
    for row in list(rows or []):
        if isinstance(row, dict):
            address = str(row.get("address", "")).strip()
            if not address:
                continue
            role = str(row.get("role", "canary") or "canary")
            normalized.append((canonical_address(address), role))
            continue
        if isinstance(row, (list, tuple)) and len(row) >= 2:
            address = str(row[0]).strip()
            if not address:
                continue
            role = str(row[1] or "canary")
            normalized.append((canonical_address(address), role))
    return normalized


def _sanitize_output_component(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", value.strip())
    cleaned = re.sub(r"-{2,}", "-", cleaned).strip("-._")
    return cleaned or "benchmark"


def _default_binary_output_name(binary_path: Path, profile: str, timestamped: bool) -> str:
    stem = _sanitize_output_component(binary_path.stem)
    profile_tag = _sanitize_output_component(profile)
    if timestamped:
        timestamp = time.strftime("%Y%m%d-%H%M%S")
        return f"{stem}-{profile_tag}-{timestamp}"
    return f"{stem}-{profile_tag}-latest"


def _default_corpus_output_name(
    manifest_name: str,
    manifest_path: Path,
    profile: str,
    timestamped: bool,
) -> str:
    source_name = manifest_name.strip() or manifest_path.stem
    source_name = re.sub(r"_corpus$", "", source_name, flags=re.IGNORECASE)
    suite_tag = _sanitize_output_component(source_name)
    profile_tag = _sanitize_output_component(profile)
    if timestamped:
        timestamp = time.strftime("%Y%m%d-%H%M%S")
        return f"{suite_tag}-{profile_tag}-{timestamp}"
    return f"{suite_tag}-{profile_tag}-latest"

from .compact_summary import (
    build_corpus_compact_summary,
    build_single_compact_summary,
    write_compact_summary,
)
from .metrics import collect_code_metrics, load_struct_pointer_aliases, normalize_address
from .llm_advisory import maybe_generate_benchmark_llm_advisory
from .render_console import (
    print_corpus_benchmark_console,
    print_single_benchmark_console,
)
from .render_markdown import (
    render_baseline_regression_markdown,
    render_corpus_benchmark_markdown,
    render_previous_comparison_markdown,
    render_single_benchmark_markdown,
)
from .resource_monitor import (
    HAS_PSUTIL,
    collect_macos_activity_snapshot,
    run_popen_with_resource_monitor,
    start_self_resource_monitor,
    summarize_macos_activity_delta,
)
from .runners import list_functions_with_fission, run_ghidra_binary_with_meta, sample_functions


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Benchmark whole-binary decompilation quality and speed between "
            "Fission and Ghidra (pyghidra)."
        )
    )
    parser.add_argument(
        "binary",
        type=Path,
        nargs="?",
        help="Path to the target binary",
    )
    parser.add_argument(
        "--corpus-manifest",
        type=Path,
        default=None,
        help=(
            "Path to a JSON corpus manifest. When set, runs the benchmark over "
            "multiple binaries and emits a corpus-global summary."
        ),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help=(
            "Directory to write benchmark artifacts into. "
            "If omitted, a fixed per-binary folder is used: "
            "benchmark/artifacts/full_benchmark/<target>-<profile>-latest"
        ),
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=None,
        help="Path to Ghidra installation directory",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=None,
        help="Path to a prebuilt fission_cli binary",
    )
    parser.add_argument(
        "--profile",
        choices=("balanced", "quality", "speed"),
        default="balanced",
        help="Fission decompiler profile",
    )
    parser.add_argument(
        "--function-discovery-profile",
        choices=("conservative", "balanced", "aggressive"),
        default="balanced",
        help="Fission function discovery profile",
    )
    parser.add_argument(
        "--compiler-id",
        default=None,
        help="Optional Fission compiler ID override (auto|windows|gcc|clang|default)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=1800,
        help="Process timeout in seconds for each whole-binary run",
    )
    parser.add_argument(
        "--ghidra-func-timeout",
        type=int,
        default=60,
        help="Per-function decompile timeout for Ghidra",
    )
    parser.add_argument(
        "--skip-thunks",
        action="store_true",
        help="Skip thunk functions on the Ghidra side",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        metavar="N",
        help="Decompile only first N functions (for faster validation)",
    )
    parser.add_argument(
        "--pairwise-similarity-mode",
        choices=("auto", "full", "shared-full", "sampled"),
        default="auto",
        help=(
            "Pairwise similarity workload mode: auto (choose by shared size), "
            "full/shared-full (all shared functions), sampled (deterministic subset)."
        ),
    )
    parser.add_argument(
        "--pairwise-sample-size",
        type=int,
        default=5000,
        help="Maximum shared functions to compare when pairwise mode is sampled/auto.",
    )
    parser.add_argument(
        "--pairwise-auto-shared-full-max",
        type=int,
        default=2000,
        help=(
            "When --pairwise-similarity-mode=auto, use shared-full only if shared_count <= N. "
            "Default: 2000 (larger sets switch to sampled)."
        ),
    )
    parser.add_argument(
        "--raw-similarity",
        action="store_true",
        help=(
            "Also compute raw (non-normalized) code similarity. "
            "Disabled by default to reduce pairwise CPU cost."
        ),
    )
    parser.add_argument(
        "--aggregate-similarity-mode",
        choices=("weighted", "sequence"),
        default="weighted",
        help=(
            "How to compute aggregate normalized similarity: "
            "weighted (fast weighted mean, default) or sequence (slow full-string matcher)."
        ),
    )

    # ── Ghidra output cache ───────────────────────────────────────────────────
    parser.add_argument(
        "--ghidra-cache-dir",
        type=Path,
        default=None,
        help="Directory for storing/loading cached Ghidra output JSON files.",
    )
    parser.add_argument(
        "--save-ghidra-cache",
        action="store_true",
        help="Save Ghidra output to the cache directory after a live run.",
    )
    parser.add_argument(
        "--use-ghidra-cache",
        action="store_true",
        help="Load Ghidra output from cache instead of running pyghidra.",
    )

    # ── Auto function limit ───────────────────────────────────────────────────
    parser.add_argument(
        "--auto-limit-functions",
        type=int,
        default=None,
        metavar="N",
        help=(
            "If the binary has more than N functions, automatically apply "
            "--limit N to prevent Ghidra from timing out on large binaries."
        ),
    )

    # ── Regression gate ───────────────────────────────────────────────────────
    parser.add_argument(
        "--baseline-dir",
        type=Path,
        default=None,
        help=(
            "Path to a previous benchmark output directory containing "
            "benchmark_summary.json. Used to detect regressions."
        ),
    )
    parser.add_argument(
        "--regression-threshold",
        type=float,
        default=2.0,
        metavar="PP",
        help=(
            "Allowed drop in avg_normalized_similarity (percentage points) "
            "before the run is considered a regression. Default: 2.0 pp."
        ),
    )
    parser.add_argument(
        "--row-fidelity-role-filter",
        choices=("all", "canary-only", "primary-secondary-only"),
        default="all",
        help=(
            "Filter row-fidelity target roles for preflight/acceptance reporting. "
            "all=use full target set, canary-only=only canary rows, "
            "primary-secondary-only=only primary and secondary rows."
        ),
    )

    parser.add_argument(
        "--timestamped-output",
        action="store_true",
        help=(
            "Use timestamped output folder naming (legacy behavior). "
            "Default is fixed folder naming for stable latest reports."
        ),
    )

    args = parser.parse_args()
    if args.corpus_manifest is not None and args.binary is not None:
        parser.error("specify either <binary> or --corpus-manifest, not both")
    if args.corpus_manifest is None and args.binary is None:
        parser.error("either <binary> or --corpus-manifest is required")
    return args


def resolve_binary(path: Path) -> Path:
    binary = path.expanduser().resolve()
    if not binary.is_file():
        raise FileNotFoundError(f"binary not found: {binary}")
    return binary


def _canonical_corpus_entry_id(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", value.strip())
    cleaned = cleaned.strip("-._")
    return cleaned or "binary"


def _normalize_manifest_row_targets(value: Any) -> list[tuple[str, str]]:
    if not isinstance(value, list):
        return []
    rows: list[tuple[str, str]] = []
    for item in value:
        if isinstance(item, dict):
            address = str(item.get("address", "")).strip()
            role = str(item.get("role", "watchlist")).strip() or "watchlist"
        elif isinstance(item, (list, tuple)) and len(item) >= 1:
            address = str(item[0]).strip()
            role = str(item[1]).strip() if len(item) >= 2 else "watchlist"
        else:
            continue
        if address:
            rows.append((canonical_address(address), role))
    return rows


def _normalize_manifest_canonical_quality_rows(value: Any) -> list[dict[str, str]]:
    if not isinstance(value, list):
        return []
    rows: list[dict[str, str]] = []
    for item in value:
        if not isinstance(item, dict):
            continue
        address = str(item.get("address", "")).strip()
        if not address:
            continue
        rows.append(
            {
                "address": canonical_address(address),
                "role": str(item.get("role", "canonical_quality")).strip() or "canonical_quality",
                "name": str(item.get("name", "")).strip(),
                "selected_because": "canonical_quality",
            }
        )
    return rows


def _normalize_corpus_role(value: Any) -> str:
    role = str(value or "release_candidate").strip().lower().replace(" ", "_")
    role = CORPUS_ROLE_ALIASES.get(role, role)
    if role in CORPUS_RELEASE_WEIGHT_ROLES or role in CORPUS_REPORT_ONLY_ROLES:
        return role
    if role == "single":
        return role
    return "release_candidate"


def _normalize_corpus_suite_tier(value: Any) -> str:
    tier = str(value or "release").strip().lower().replace(" ", "_")
    return tier if tier in CORPUS_SUITE_TIERS else "release"


def _normalize_corpus_gate_mode(value: Any) -> str:
    mode = str(value or "advisory").strip().lower().replace(" ", "_")
    return mode if mode in CORPUS_GATE_MODES else "advisory"


def load_corpus_manifest(manifest_path: Path) -> dict[str, Any]:
    resolved = manifest_path.expanduser().resolve()
    if not resolved.is_file():
        raise FileNotFoundError(f"corpus manifest not found: {resolved}")
    with resolved.open("r", encoding="utf-8") as fh:
        payload = json.load(fh)

    suite_tier = (
        _normalize_corpus_suite_tier(payload.get("suite_tier"))
        if isinstance(payload, dict)
        else "release"
    )
    gate_mode = (
        _normalize_corpus_gate_mode(payload.get("gate_mode"))
        if isinstance(payload, dict)
        else "advisory"
    )
    dynamic_watchlist_limit = max(
        _safe_int(
            payload.get("dynamic_watchlist_limit") if isinstance(payload, dict) else None,
            DEFAULT_DYNAMIC_WATCHLIST_LIMIT,
        ),
        1,
    )
    notes = (
        str(payload.get("notes", "")).strip()
        if isinstance(payload, dict) and payload.get("notes") is not None
        else ""
    )

    raw_entries = payload.get("entries", payload) if isinstance(payload, dict) else payload
    if not isinstance(raw_entries, list) or not raw_entries:
        raise ValueError(f"corpus manifest must contain a non-empty entries list: {resolved}")

    normalized_entries: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for index, raw_entry in enumerate(raw_entries):
        if not isinstance(raw_entry, dict):
            raise ValueError(f"corpus manifest entry #{index} must be an object")
        binary_path_raw = raw_entry.get("binary_path")
        if not binary_path_raw:
            raise ValueError(f"corpus manifest entry #{index} missing binary_path")
        binary_path = resolve_binary(Path(str(binary_path_raw)))
        entry_id = _canonical_corpus_entry_id(
            str(raw_entry.get("id") or raw_entry.get("ghidra_project_key") or binary_path.stem)
        )
        if entry_id in seen_ids:
            raise ValueError(f"duplicate corpus manifest id: {entry_id}")
        seen_ids.add(entry_id)
        role = _normalize_corpus_role(raw_entry.get("role", "release_candidate"))
        tags = raw_entry.get("tags", [])
        if not isinstance(tags, list):
            raise ValueError(f"corpus manifest entry {entry_id} has non-list tags")
        seed_limit = raw_entry.get("seed_limit")
        if seed_limit is not None:
            seed_limit = _safe_int(seed_limit, 0)
            if seed_limit <= 0:
                raise ValueError(f"corpus manifest entry {entry_id} has invalid seed_limit")
        normalized_entries.append(
            {
                "id": entry_id,
                "binary_path": str(binary_path),
                "ghidra_project_key": str(
                    raw_entry.get("ghidra_project_key") or entry_id
                ).strip()
                or entry_id,
                "tags": [str(tag) for tag in tags],
                "seed_limit": seed_limit,
                "role": role,
                "weight": max(
                    _safe_int(
                        raw_entry.get("weight"),
                        2 if role == "primary_canary" else 1,
                    ),
                    1,
                ),
                "row_fidelity_targets": _normalize_manifest_row_targets(
                    raw_entry.get("row_fidelity_targets", [])
                ),
                "canonical_quality_rows": _normalize_manifest_canonical_quality_rows(
                    raw_entry.get("canonical_quality_rows", [])
                ),
                "suite_tier": suite_tier,
                "gate_mode": gate_mode,
                "dynamic_watchlist_limit": dynamic_watchlist_limit,
            }
        )

    manifest_name = (
        str(payload.get("name")).strip()
        if isinstance(payload, dict) and payload.get("name")
        else resolved.stem
    )
    return {
        "path": str(resolved),
        "name": manifest_name or resolved.stem,
        "suite_tier": suite_tier,
        "gate_mode": gate_mode,
        "dynamic_watchlist_limit": dynamic_watchlist_limit,
        "notes": notes,
        "entries": normalized_entries,
    }


# ── Ghidra output cache helpers ───────────────────────────────────────────────

def _binary_sha256_prefix(binary_path: Path, n: int = 8) -> str:
    """Return first `n` hex chars of the binary's SHA-256 hash."""
    h = hashlib.sha256()
    with binary_path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()[:n]


def _ghidra_version_tag(ghidra_dir: Path) -> str:
    """Extract a short version string from the Ghidra installation directory name."""
    name = ghidra_dir.name  # e.g. "ghidra-Ghidra_12.0.4_build"
    # Keep only alphanumeric/dot/underscore segments
    return re.sub(r"[^A-Za-z0-9._]", "_", name)[:24]


def resolve_ghidra_cache_path(
    cache_dir: Path,
    binary_path: Path,
    ghidra_dir: Path,
    limit: int | None,
) -> Path:
    sha = _binary_sha256_prefix(binary_path)
    ver = _ghidra_version_tag(ghidra_dir)
    limit_tag = f"limit{limit}" if limit is not None else "full"
    filename = f"{binary_path.stem}_{ver}_{sha}_{limit_tag}.json"
    return cache_dir / filename


def load_ghidra_cache(cache_path: Path) -> dict[str, Any]:
    with cache_path.open("r", encoding="utf-8") as fh:
        payload = json.load(fh)
    meta = payload.get("_meta", {})
    meta["from_cache"] = True
    meta["cache_path"] = str(cache_path)
    entries: dict[str, dict[str, Any]] = {}
    for entry in payload.get("functions", []):
        address = canonical_address(entry.get("address", "0x0"))
        entries[address] = entry
        entries[address].setdefault("address", address)
    return {"meta": meta, "entries": entries}


def save_ghidra_cache(cache_path: Path, raw_output_path: Path) -> None:
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(raw_output_path, cache_path)


# ── Function-count helper for auto-limit ─────────────────────────────────────

def count_binary_functions(binary_path: Path, fission_bin: Path) -> int | None:
    """Return the number of functions in the binary via fission_cli --list."""
    try:
        result = subprocess.run(
            [str(fission_bin), str(binary_path), "--list"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
            check=False,
        )
        count = sum(
            1
            for line in result.stdout.splitlines()
            if len(line.split()) >= 3 and line.split()[0].startswith("0x")
        )
        return count if count > 0 else None
    except Exception:
        return None


def resolve_effective_limit(
    binary_path: Path,
    fission_bin: Path,
    explicit_limit: int | None,
    auto_limit: int | None,
) -> int | None:
    """
    Return the effective function limit to use.

    Priority:
      1. Explicit --limit N always wins.
      2. --auto-limit-functions N: apply if function count exceeds N.
      3. No limit.
    """
    if explicit_limit is not None:
        return explicit_limit
    if auto_limit is None:
        return None
    func_count = count_binary_functions(binary_path, fission_bin)
    if func_count is not None and func_count > auto_limit:
        print(
            f"[*] Auto-limit: binary has {func_count} functions > {auto_limit}; "
            f"applying --limit {auto_limit}"
        )
        return auto_limit
    return None


# ── Regression gate ───────────────────────────────────────────────────────────

def load_baseline_summary(baseline_dir: Path) -> dict[str, Any] | None:
    candidate = baseline_dir / "benchmark_summary.json"
    if not candidate.is_file():
        print(f"[!] No benchmark_summary.json found in baseline dir: {baseline_dir}", file=sys.stderr)
        return None
    with candidate.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def _pairwise_row_map(summary_payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    rows = _lookup_path(
        summary_payload,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    if not isinstance(rows, list):
        return {}
    return {
        str(row.get("address", "")): row
        for row in rows
        if isinstance(row, dict) and row.get("address")
    }


def _engine_entries_map(summary_payload: dict[str, Any], engine: str) -> dict[str, Any]:
    entries = _lookup_path(summary_payload, ("engines", engine, "entries"), {})
    return entries if isinstance(entries, dict) else {}


def _derive_dynamic_row_targets(
    baseline_summary_json: dict[str, Any],
    limit: int = 5,
) -> list[dict[str, str]]:
    derived: list[dict[str, str]] = []
    seen: set[str] = set()
    degraded_rows = (
        _lookup_path(
            baseline_summary_json,
            ("baseline_regression_gate", "row_fidelity_gate", "rows"),
            [],
        )
        or []
    )
    if isinstance(degraded_rows, list):
        for row in degraded_rows:
            if not isinstance(row, dict):
                continue
            if str(row.get("status", "")).strip() != "degraded":
                continue
            address = str(row.get("address", "")).strip()
            if not address:
                continue
            canonical = canonical_address(address)
            if canonical in seen:
                continue
            seen.add(canonical)
            derived.append(
                {
                    "address": canonical,
                    "role": str(row.get("role", "dynamic_degraded")) or "dynamic_degraded",
                    "selected_because": "baseline_degraded",
                }
            )
            if len(derived) >= limit:
                return derived

    pair_rows = _lookup_path(
        baseline_summary_json,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    if isinstance(pair_rows, list):
        ranked = sorted(
            (
                row for row in pair_rows
                if isinstance(row, dict)
                and row.get("address")
                and row.get("fission_success")
                and row.get("pyghidra_success")
            ),
            key=lambda row: _safe_float(row.get("normalized_similarity", 0.0), 0.0),
        )
        for row in ranked:
            canonical = canonical_address(str(row.get("address")))
            if canonical in seen:
                continue
            seen.add(canonical)
            derived.append(
                {
                    "address": canonical,
                    "role": "dynamic_low_similarity",
                    "selected_because": "baseline_low_similarity",
                }
            )
            if len(derived) >= limit:
                break
    return derived


def _resolve_binary_watchlist(
    manifest_entry: dict[str, Any] | None,
    baseline_summary_json: dict[str, Any] | None,
    default_row_targets: list[tuple[str, str]],
    dynamic_watchlist_limit: int,
) -> dict[str, Any]:
    if manifest_entry is None:
        canonical_quality_rows: list[dict[str, str]] = []
        bootstrap_rows = [
            {
                "address": canonical_address(address),
                "role": role,
                "selected_because": "bootstrap_explicit",
            }
            for address, role in default_row_targets
        ]
        dynamic_rows: list[dict[str, str]] = []
        final_rows = [(row["address"], row["role"]) for row in bootstrap_rows]
        watchlist_source = "explicit"
    else:
        canonical_quality_rows = [
            {
                "address": canonical_address(str(row.get("address", ""))),
                "role": str(row.get("role", "canonical_quality") or "canonical_quality"),
                "selected_because": "canonical_quality",
                "name": str(row.get("name", "") or ""),
            }
            for row in list(manifest_entry.get("canonical_quality_rows", []) or [])
            if isinstance(row, dict) and str(row.get("address", "")).strip()
        ]
        bootstrap_rows = [
            {
                "address": canonical_address(address),
                "role": role,
                "selected_because": "bootstrap_explicit",
            }
            for address, role in manifest_entry.get("row_fidelity_targets", [])
        ]
        dynamic_rows = (
            _derive_dynamic_row_targets(
                baseline_summary_json,
                limit=max(dynamic_watchlist_limit, 1),
            )
            if baseline_summary_json is not None
            else []
        )
        final_rows = []
        seen: set[str] = set()
        for row in [*canonical_quality_rows, *bootstrap_rows, *dynamic_rows]:
            canonical = canonical_address(row["address"])
            if canonical in seen:
                continue
            seen.add(canonical)
            final_rows.append((canonical, str(row["role"])))
            if len(final_rows) >= max(dynamic_watchlist_limit, 1):
                break
        if (canonical_quality_rows or bootstrap_rows) and dynamic_rows:
            watchlist_source = "mixed"
        elif canonical_quality_rows or bootstrap_rows:
            watchlist_source = "explicit"
        else:
            watchlist_source = "dynamic"
    final_addresses = {canonical_address(address) for address, _ in final_rows}
    canonical_quality_rows_final = [
        row for row in canonical_quality_rows if canonical_address(row["address"]) in final_addresses
    ]
    bootstrap_rows_final = [
        row for row in bootstrap_rows if canonical_address(row["address"]) in final_addresses
    ]
    dynamic_rows_final = [
        row for row in dynamic_rows if canonical_address(row["address"]) in final_addresses
    ]
    selected_because_counts: dict[str, int] = {}
    for row in [*canonical_quality_rows_final, *bootstrap_rows_final, *dynamic_rows_final]:
        reason = str(row.get("selected_because", "") or "")
        if not reason:
            continue
        selected_because_counts[reason] = selected_because_counts.get(reason, 0) + 1

    return {
        "rows": final_rows,
        "watchlist_source": watchlist_source,
        "canonical_quality_rows": canonical_quality_rows_final,
        "bootstrap_row_targets": bootstrap_rows_final,
        "dynamic_watchlist_rows": dynamic_rows_final,
        "watchlist_diagnostics": {
            "watchlist_source": watchlist_source,
            "canonical_quality_row_count": len(canonical_quality_rows_final),
            "bootstrap_row_target_count": len(bootstrap_rows_final),
            "dynamic_watchlist_row_count": len(dynamic_rows_final),
            "selected_because_counts": dict(sorted(selected_because_counts.items())),
        },
    }


def _build_row_fidelity_gate(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    row_targets: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    cur_rows_map = _pairwise_row_map(current_benchmark)
    base_rows_map = _pairwise_row_map(baseline_summary_json)
    cur_fission_entries = _engine_entries_map(current_benchmark, "fission")
    base_fission_entries = _engine_entries_map(baseline_summary_json, "fission")

    row_results: list[dict[str, Any]] = []
    improved = 0
    degraded = 0
    unchanged = 0
    missing = 0
    failed_targets: list[str] = []

    targets = (
        _normalize_row_target_pairs(row_targets)
        or _normalize_row_target_pairs(_derive_dynamic_row_targets(baseline_summary_json))
        or list(ROW_FIDELITY_TARGETS)
    )

    for address, role in targets:
        cur_row = cur_rows_map.get(address)
        base_row = base_rows_map.get(address)
        if cur_row is None or base_row is None:
            status = "missing"
            missing += 1
            failed_targets.append(address)
            row_results.append(
                {
                    "address": address,
                    "role": role,
                    "status": status,
                    "present_in_current": cur_row is not None,
                    "present_in_baseline": base_row is not None,
                    "failure_reasons": ["missing_row"],
                }
            )
            continue

        cur_norm = _safe_float(cur_row.get("normalized_similarity", 0.0), 0.0)
        base_norm = _safe_float(base_row.get("normalized_similarity", 0.0), 0.0)
        delta = cur_norm - base_norm

        cur_fission = cur_fission_entries.get(address, {})
        base_fission = base_fission_entries.get(address, {})
        cur_stats = cur_fission.get("preview_build_stats") if isinstance(cur_fission, dict) else {}
        base_stats = base_fission.get("preview_build_stats") if isinstance(base_fission, dict) else {}
        if not isinstance(cur_stats, dict):
            cur_stats = {}
        if not isinstance(base_stats, dict):
            base_stats = {}

        failure_reasons: list[str] = []
        cur_success = bool(cur_row.get("fission_success", False))
        base_success = bool(base_row.get("fission_success", False))
        if base_success and not cur_success:
            failure_reasons.append("new_failure")
        if delta < -1e-9:
            failure_reasons.append("similarity_regressed")

        if failure_reasons:
            status = "degraded"
            degraded += 1
            failed_targets.append(address)
        elif delta > 1e-9:
            status = "improved"
            improved += 1
        else:
            status = "unchanged"
            unchanged += 1

        row_result = {
            "address": address,
            "role": role,
            "status": status,
            "failure_reasons": failure_reasons,
            "pyghidra_name": cur_row.get("pyghidra_name") or base_row.get("pyghidra_name") or "",
            "fission_name": cur_row.get("fission_name") or base_row.get("fission_name") or "",
            "previous_normalized_similarity": round(base_norm, 3),
            "current_normalized_similarity": round(cur_norm, 3),
            "normalized_similarity_delta": round(delta, 3),
            "previous_fission_success": base_success,
            "current_fission_success": cur_success,
            "previous_code_sha256": _entry_code_sha256(base_fission_entries.get(address)),
            "current_code_sha256": _entry_code_sha256(cur_fission_entries.get(address)),
            "previous_unsupported_indirect_control_count": _safe_int(
                base_stats.get("unsupported_indirect_control_count"), 0
            ),
            "current_unsupported_indirect_control_count": _safe_int(
                cur_stats.get("unsupported_indirect_control_count"), 0
            ),
            "previous_indirect_surface_preserved_count": _safe_int(
                base_stats.get("indirect_surface_preserved_count"), 0
            ),
            "current_indirect_surface_preserved_count": _safe_int(
                cur_stats.get("indirect_surface_preserved_count"), 0
            ),
            "previous_dispatcher_shape_recovered_count": _safe_int(
                base_stats.get("dispatcher_shape_recovered_count"), 0
            ),
            "current_dispatcher_shape_recovered_count": _safe_int(
                cur_stats.get("dispatcher_shape_recovered_count"), 0
            ),
            "previous_dispatcher_proof_completed_count": _safe_int(
                base_stats.get("dispatcher_proof_completed_count"), 0
            ),
            "current_dispatcher_proof_completed_count": _safe_int(
                cur_stats.get("dispatcher_proof_completed_count"), 0
            ),
            "previous_proof_payload_direct_emit_count": _safe_int(
                base_stats.get("proof_payload_direct_emit_count"), 0
            ),
            "current_proof_payload_direct_emit_count": _safe_int(
                cur_stats.get("proof_payload_direct_emit_count"), 0
            ),
            "previous_region_proof_candidate_count": _safe_int(
                base_stats.get("region_proof_candidate_count"), 0
            ),
            "current_region_proof_candidate_count": _safe_int(
                cur_stats.get("region_proof_candidate_count"), 0
            ),
            "previous_region_proof_completed_count": _safe_int(
                base_stats.get("region_proof_completed_count"), 0
            ),
            "current_region_proof_completed_count": _safe_int(
                cur_stats.get("region_proof_completed_count"), 0
            ),
            "previous_region_emit_ready_failed_count": _safe_int(
                base_stats.get("region_emit_ready_failed_count"), 0
            ),
            "current_region_emit_ready_failed_count": _safe_int(
                cur_stats.get("region_emit_ready_failed_count"), 0
            ),
            "previous_switch_emit_ready_failed_count": _safe_int(
                base_stats.get("switch_emit_ready_failed_count"), 0
            ),
            "current_switch_emit_ready_failed_count": _safe_int(
                cur_stats.get("switch_emit_ready_failed_count"), 0
            ),
            "previous_conditional_region_candidate_count": _safe_int(
                base_stats.get("conditional_region_candidate_count"), 0
            ),
            "current_conditional_region_candidate_count": _safe_int(
                cur_stats.get("conditional_region_candidate_count"), 0
            ),
            "previous_conditional_region_promoted_count": _safe_int(
                base_stats.get("conditional_region_promoted_count"), 0
            ),
            "current_conditional_region_promoted_count": _safe_int(
                cur_stats.get("conditional_region_promoted_count"), 0
            ),
            "previous_guarded_tail_replacement_plan_rejected_missing_merge_count": _safe_int(
                base_stats.get("guarded_tail_replacement_plan_rejected_missing_merge_count"), 0
            ),
            "current_guarded_tail_replacement_plan_rejected_missing_merge_count": _safe_int(
                cur_stats.get("guarded_tail_replacement_plan_rejected_missing_merge_count"), 0
            ),
            "previous_guarded_tail_replacement_plan_rejected_unstable_read_count": _safe_int(
                base_stats.get("guarded_tail_replacement_plan_rejected_unstable_read_count"), 0
            ),
            "current_guarded_tail_replacement_plan_rejected_unstable_read_count": _safe_int(
                cur_stats.get("guarded_tail_replacement_plan_rejected_unstable_read_count"), 0
            ),
            "previous_materialization_stabilized_count": _safe_int(
                base_stats.get("materialization_stabilized_count"), 0
            ),
            "current_materialization_stabilized_count": _safe_int(
                cur_stats.get("materialization_stabilized_count"), 0
            ),
            "previous_guarded_tail_rejected_side_entry_conflict_count": _safe_int(
                base_stats.get("guarded_tail_rejected_side_entry_conflict_count"), 0
            ),
            "current_guarded_tail_rejected_side_entry_conflict_count": _safe_int(
                cur_stats.get("guarded_tail_rejected_side_entry_conflict_count"), 0
            ),
            "previous_guarded_tail_rejected_alias_interleave_conflict_count": _safe_int(
                base_stats.get("guarded_tail_rejected_alias_interleave_conflict_count"), 0
            ),
            "current_guarded_tail_rejected_alias_interleave_conflict_count": _safe_int(
                cur_stats.get("guarded_tail_rejected_alias_interleave_conflict_count"), 0
            ),
            "previous_guarded_tail_rejected_ambiguous_follow_count": _safe_int(
                base_stats.get("guarded_tail_rejected_ambiguous_follow_count"), 0
            ),
            "current_guarded_tail_rejected_ambiguous_follow_count": _safe_int(
                cur_stats.get("guarded_tail_rejected_ambiguous_follow_count"), 0
            ),
            "previous_representative_downgrade_count": _safe_int(
                base_stats.get("representative_downgrade_count"), 0
            ),
            "current_representative_downgrade_count": _safe_int(
                cur_stats.get("representative_downgrade_count"), 0
            ),
            "previous_representative_downgrade_join_conflict_count": _safe_int(
                base_stats.get("representative_downgrade_join_conflict_count"), 0
            ),
            "current_representative_downgrade_join_conflict_count": _safe_int(
                cur_stats.get("representative_downgrade_join_conflict_count"), 0
            ),
        }
        row_result["code_changed"] = (
            row_result["previous_code_sha256"] != row_result["current_code_sha256"]
            if row_result["previous_code_sha256"] and row_result["current_code_sha256"]
            else False
        )
        row_result["canonical_rejection_reasons"] = (
            _classify_row_regression_reasons(row_result) if status == "degraded" else []
        )
        row_results.append(row_result)

    return {
        "status": "failed" if failed_targets else "passed",
        "failed_target_count": len(failed_targets),
        "failed_targets": failed_targets,
        "improved_count": improved,
        "degraded_count": degraded,
        "unchanged_count": unchanged,
        "missing_count": missing,
        "rows": row_results,
    }


def _build_baseline_regression_report(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    threshold_pp: float,
    row_targets: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    regressions: list[str] = []

    cur_quality = (
        current_benchmark
        .get("summary", {})
        .get("quality", {})
        .get("pyghidra_vs_fission", {})
    )
    base_quality = (
        baseline_summary_json
        .get("summary", {})
        .get("quality", {})
        .get("pyghidra_vs_fission", {})
    )

    cur_sim = float(cur_quality.get("avg_normalized_similarity", 0.0) or 0.0)
    base_sim = float(base_quality.get("avg_normalized_similarity", 0.0) or 0.0)
    sim_delta = cur_sim - base_sim
    if sim_delta < -threshold_pp:
        regressions.append(
            f"avg_norm_sim: {base_sim:.2f}% → {cur_sim:.2f}% "
            f"(Δ={sim_delta:.2f}pp, threshold={-threshold_pp:.2f}pp)"
        )

    cur_both_success = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("both_success_rate_pct", 0.0) or 0.0
    )
    base_both_success = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("both_success_rate_pct", 0.0) or 0.0
    )
    both_success_delta = cur_both_success - base_both_success
    if both_success_delta < -1.0:
        regressions.append(
            f"both_success_rate: {base_both_success:.3f}% -> {cur_both_success:.3f}% "
            f"(Δ={both_success_delta:.3f}pp, threshold=-1.000pp)"
        )

    cur_high_div = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("high_divergence_pct", 0.0) or 0.0
    )
    base_high_div = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("high_divergence_pct", 0.0) or 0.0
    )
    high_div_delta = cur_high_div - base_high_div
    if high_div_delta > 2.0:
        regressions.append(
            f"high_divergence_pct: {base_high_div:.3f}% -> {cur_high_div:.3f}% "
            f"(Δ={high_div_delta:.3f}pp, threshold=+2.000pp)"
        )

    cur_succ = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("engines", {})
        .get("fission", {})
        .get("quality_kpi", {})
        .get("success_rate_pct", 100.0) or 100.0
    )
    base_succ = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("engines", {})
        .get("fission", {})
        .get("quality_kpi", {})
        .get("success_rate_pct", 100.0) or 100.0
    )
    if cur_succ - base_succ < -1.0:
        regressions.append(
            f"fission success_rate: {base_succ:.3f}% → {cur_succ:.3f}% "
            f"(Δ={cur_succ - base_succ:.3f}pp)"
        )

    cur_goto = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("goto_total", 0) or 0
    )
    base_goto = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("goto_total", 0) or 0
    )
    if base_goto > 0 and cur_goto > base_goto * 1.10:
        regressions.append(
            f"fission goto_total: {base_goto} → {cur_goto} "
            f"(+{((cur_goto - base_goto) / base_goto * 100):.1f}%, threshold=10%)"
        )

    cur_readability_penalty = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("readability_control_flow_penalty", 0) or 0
    )
    base_readability_penalty = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("readability_control_flow_penalty", 0) or 0
    )
    if (
        base_readability_penalty > 0
        and cur_readability_penalty > int(base_readability_penalty * 1.10)
    ):
        regressions.append(
            f"fission readability_control_flow_penalty: {base_readability_penalty} -> {cur_readability_penalty} "
            f"(+{((cur_readability_penalty - base_readability_penalty) / base_readability_penalty * 100):.1f}%, threshold=10%)"
        )

    cur_undef_total = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("undefined_return_type_total", 0) or 0
    )
    base_undef_total = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("undefined_return_type_total", 0) or 0
    )
    cur_success_count = max(
        int(
            current_benchmark.get("summary", {})
            .get("engines", {})
            .get("fission", {})
            .get("success_count", 0) or 0
        ),
        1,
    )
    base_success_count = max(
        int(
            baseline_summary_json.get("summary", {})
            .get("engines", {})
            .get("fission", {})
            .get("success_count", 0) or 0
        ),
        1,
    )
    cur_undef_rate = (cur_undef_total / cur_success_count) * 100.0
    base_undef_rate = (base_undef_total / base_success_count) * 100.0
    undef_delta = cur_undef_rate - base_undef_rate
    if undef_delta > 5.0:
        regressions.append(
            f"fission undefined_return_type_rate: {base_undef_rate:.3f}% -> {cur_undef_rate:.3f}% "
            f"(Δ={undef_delta:.3f}pp, threshold=+5.000pp)"
        )
    cur_unsupported_indirect = _safe_int(
        _lookup_path(
            current_benchmark,
            ("summary", "engines", "fission", "unsupported_indirect_control_count"),
            0,
        ),
        0,
    )
    base_unsupported_indirect = _safe_int(
        _lookup_path(
            baseline_summary_json,
            ("summary", "engines", "fission", "unsupported_indirect_control_count"),
            0,
        ),
        0,
    )
    if cur_unsupported_indirect > base_unsupported_indirect:
        regressions.append(
            f"fission unsupported_indirect_control_count: {base_unsupported_indirect} -> {cur_unsupported_indirect}"
        )

    current_snapshot_metrics = extract_snapshot_metrics(current_benchmark)
    baseline_snapshot_metrics = extract_snapshot_metrics(baseline_summary_json)
    for _, alias in OWNER_METRIC_SPECS:
        metric_key = f"owner_{alias}"
        current_value = _safe_float(current_snapshot_metrics.get(metric_key, 0.0), 0.0)
        baseline_value = _safe_float(baseline_snapshot_metrics.get(metric_key, 0.0), 0.0)
        if current_value > baseline_value + 1e-9:
            regressions.append(f"{metric_key}: {baseline_value:.3f} -> {current_value:.3f}")
    for shape_key in BASELINE_GATE_SHAPE_KEYS:
        alias = next(
            (
                metric_alias
                for metric_name, metric_alias in SHAPE_DRIFT_METRIC_SPECS
                if metric_name == shape_key
            ),
            shape_key,
        )
        metric_key = f"shape_{alias}"
        current_value = _safe_float(current_snapshot_metrics.get(metric_key, 0.0), 0.0)
        baseline_value = _safe_float(baseline_snapshot_metrics.get(metric_key, 0.0), 0.0)
        if current_value > baseline_value + 1e-9:
            regressions.append(f"{metric_key}: {baseline_value:.3f} -> {current_value:.3f}")

    row_fidelity_gate = _build_row_fidelity_gate(
        current_benchmark,
        baseline_summary_json,
        row_targets=row_targets,
    )
    if row_fidelity_gate.get("status") != "passed":
        regressions.append(
            "row_fidelity_gate failed for "
            + ", ".join(str(addr) for addr in row_fidelity_gate.get("failed_targets", []))
        )

    return {
        "status": "failed" if regressions else "passed",
        "regressions": regressions,
        "threshold_pp": round(float(threshold_pp), 3),
        "row_fidelity_gate": row_fidelity_gate,
        "top_degraded_functions": collect_top_degraded_functions_vs_previous(
            current_benchmark,
            baseline_summary_json,
            limit=20,
            similarity_drop_pp_threshold=0.0,
        ),
    }


def check_regression(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    threshold_pp: float,
    row_targets: list[tuple[str, str]] | None = None,
) -> bool:
    """
    Compare current run against baseline.  Returns True if a regression is detected.
    """
    report = _build_baseline_regression_report(
        current_benchmark,
        baseline_summary_json,
        threshold_pp,
        row_targets=row_targets,
    )
    if report.get("status") != "passed":
        print("\n[REGRESSION DETECTED]", file=sys.stderr)
        for msg in report.get("regressions", []):
            print(f"  - {msg}", file=sys.stderr)
        return True

    print("[*] Regression check passed — no significant degradation detected.")
    return False


def extract_snapshot_metrics(summary_payload: dict[str, Any]) -> dict[str, float]:
    fission_success_count = _safe_int(
        _lookup_path(summary_payload, ("summary", "engines", "fission", "success_count"), 0),
        0,
    )
    fission_undefined_return_total = _safe_int(
        _lookup_path(summary_payload, ("summary", "engines", "fission", "undefined_return_type_total"), 0),
        0,
    )
    fission_undefined_return_rate = (
        (fission_undefined_return_total / max(fission_success_count, 1)) * 100.0
    )

    metrics: dict[str, float] = {
        "avg_normalized_similarity_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "quality", "pyghidra_vs_fission", "avg_normalized_similarity"), 0.0),
            0.0,
        ),
        "both_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "intersection", "pyghidra_vs_fission", "both_success_rate_pct"), 0.0),
            0.0,
        ),
        "high_divergence_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "intersection", "pyghidra_vs_fission", "high_divergence_pct"), 0.0),
            0.0,
        ),
        "fission_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "quality_kpi", "success_rate_pct"), 0.0),
            0.0,
        ),
        "fission_direct_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "quality_kpi", "direct_success_rate_pct"), 0.0),
            0.0,
        ),
        "wall_speedup_vs_pyghidra": _safe_float(
            _lookup_path(summary_payload, ("summary", "speed", "fission", "wall_speedup_vs_pyghidra"), 0.0),
            0.0,
        ),
        "fission_throughput_func_per_sec": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "performance_kpi", "throughput_func_per_sec"), 0.0),
            0.0,
        ),
        "fission_wall_sec": _safe_float(
            _lookup_path(summary_payload, ("summary", "speed", "fission", "wall_sec"), 0.0),
            0.0,
        ),
        "fission_goto_total": float(
            _safe_int(_lookup_path(summary_payload, ("summary", "engines", "fission", "goto_total"), 0), 0)
        ),
        "fission_readability_control_flow_penalty": float(
            _safe_int(_lookup_path(summary_payload, ("summary", "engines", "fission", "readability_control_flow_penalty"), 0), 0)
        ),
        "fission_undefined_return_type_total": float(
            fission_undefined_return_total
        ),
        "fission_undefined_return_type_rate_pct": round(fission_undefined_return_rate, 6),
    }
    for key, alias in OWNER_METRIC_SPECS:
        metrics[f"owner_{alias}"] = _safe_float(
            _lookup_path(summary_payload, ("summary", "engines", "fission", key), 0.0),
            0.0,
        )
    for key, alias in SHAPE_DRIFT_METRIC_SPECS:
        metrics[f"shape_{alias}"] = _safe_float(
            _lookup_path(summary_payload, ("summary", "engines", "fission", key), 0.0),
            0.0,
        )
    return metrics


def _anti_pattern_total(metrics: dict[str, Any]) -> int:
    anti = (metrics or {}).get("anti_pattern_counts") or {}
    return sum(int(v or 0) for v in anti.values())


def collect_top_degraded_functions_vs_previous(
    current_benchmark: dict[str, Any],
    previous_summary_payload: dict[str, Any],
    limit: int = 20,
    similarity_drop_pp_threshold: float = 1.0,
) -> dict[str, Any]:
    cur_rows = _lookup_path(
        current_benchmark,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    prev_rows = _lookup_path(
        previous_summary_payload,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    cur_rows_map = {
        str(row.get("address", "")): row
        for row in cur_rows
        if isinstance(row, dict) and row.get("address")
    }
    prev_rows_map = {
        str(row.get("address", "")): row
        for row in prev_rows
        if isinstance(row, dict) and row.get("address")
    }

    cur_fission_entries = _lookup_path(current_benchmark, ("engines", "fission", "entries"), {})
    prev_fission_entries = _lookup_path(previous_summary_payload, ("engines", "fission", "entries"), {})
    if not isinstance(cur_fission_entries, dict):
        cur_fission_entries = {}
    if not isinstance(prev_fission_entries, dict):
        prev_fission_entries = {}

    degraded_rows: list[dict[str, Any]] = []
    for address in sorted(set(cur_rows_map) & set(prev_rows_map), key=lambda a: int(a, 16)):
        cur_row = cur_rows_map[address]
        prev_row = prev_rows_map[address]

        cur_norm = _safe_float(cur_row.get("normalized_similarity", 0.0), 0.0)
        prev_norm = _safe_float(prev_row.get("normalized_similarity", 0.0), 0.0)
        delta = cur_norm - prev_norm
        if delta >= -abs(similarity_drop_pp_threshold):
            continue

        cur_fission = cur_fission_entries.get(address, {}) if isinstance(cur_fission_entries.get(address, {}), dict) else {}
        prev_fission = prev_fission_entries.get(address, {}) if isinstance(prev_fission_entries.get(address, {}), dict) else {}
        cur_metrics = cur_fission.get("metrics") if isinstance(cur_fission.get("metrics"), dict) else {}
        prev_metrics = prev_fission.get("metrics") if isinstance(prev_fission.get("metrics"), dict) else {}

        reasons: list[str] = []

        prev_success = bool(prev_fission.get("success", prev_row.get("fission_success", False)))
        cur_success = bool(cur_fission.get("success", cur_row.get("fission_success", False)))
        if prev_success and not cur_success:
            reasons.append("new_failure")

        if bool(cur_fission.get("fell_back", False)) and not bool(prev_fission.get("fell_back", False)):
            reasons.append("new_fallback")

        goto_delta = _safe_int(cur_metrics.get("goto_count", 0), 0) - _safe_int(prev_metrics.get("goto_count", 0), 0)
        if goto_delta > 0:
            reasons.append("goto_increase")

        undef_delta = int(bool(cur_metrics.get("undefined_return_type", False))) - int(bool(prev_metrics.get("undefined_return_type", False)))
        if undef_delta > 0:
            reasons.append("new_undefined_return")

        unknown_type_var_delta = _safe_int(cur_metrics.get("unknown_type_var_count", 0), 0) - _safe_int(prev_metrics.get("unknown_type_var_count", 0), 0)
        if unknown_type_var_delta > 0:
            reasons.append("unknown_type_var_increase")

        anti_delta = _anti_pattern_total(cur_metrics) - _anti_pattern_total(prev_metrics)
        if anti_delta > 0:
            reasons.append("anti_pattern_increase")

        if not reasons:
            reasons.append("semantic_divergence")

        degraded_rows.append(
            {
                "address": address,
                "pyghidra_name": cur_row.get("pyghidra_name") or prev_row.get("pyghidra_name") or "",
                "fission_name": cur_row.get("fission_name") or prev_row.get("fission_name") or "",
                "previous_normalized_similarity": round(prev_norm, 3),
                "current_normalized_similarity": round(cur_norm, 3),
                "normalized_similarity_delta": round(delta, 3),
                "previous_fission_success": prev_success,
                "current_fission_success": cur_success,
                "goto_delta": goto_delta,
                "unknown_type_var_delta": unknown_type_var_delta,
                "anti_pattern_delta": anti_delta,
                "reason_tags": reasons,
            }
        )

    degraded_rows.sort(
        key=lambda row: (
            row.get("normalized_similarity_delta", 0.0),
            -len(row.get("reason_tags", [])),
            row.get("address", ""),
        )
    )
    return {
        "similarity_drop_pp_threshold": round(abs(similarity_drop_pp_threshold), 3),
        "degraded_function_count": len(degraded_rows),
        "top_degraded": degraded_rows[: max(limit, 1)],
    }


def compare_with_previous_summary(
    current_benchmark: dict[str, Any],
    previous_summary_payload: dict[str, Any],
) -> dict[str, Any]:
    previous_metrics = extract_snapshot_metrics(previous_summary_payload)
    current_metrics = extract_snapshot_metrics(current_benchmark)

    metric_specs = [
        ("avg_normalized_similarity_pct", "avg normalized similarity (%)", "higher_is_better"),
        ("both_success_rate_pct", "both-success rate (%)", "higher_is_better"),
        ("high_divergence_pct", "high divergence (%)", "lower_is_better"),
        ("fission_success_rate_pct", "fission success rate (%)", "higher_is_better"),
        ("fission_direct_success_rate_pct", "fission direct success rate (%)", "higher_is_better"),
        ("wall_speedup_vs_pyghidra", "wall speedup vs pyghidra (x)", "higher_is_better"),
        ("fission_throughput_func_per_sec", "fission throughput (func/s)", "higher_is_better"),
        ("fission_wall_sec", "fission wall time (s)", "lower_is_better"),
        ("fission_goto_total", "fission goto total", "lower_is_better"),
        (
            "fission_readability_control_flow_penalty",
            "fission readability control-flow penalty",
            "lower_is_better",
        ),
        (
            "fission_undefined_return_type_total",
            "fission undefined return type total",
            "lower_is_better",
        ),
        (
            "fission_undefined_return_type_rate_pct",
            "fission undefined return type rate (%)",
            "lower_is_better",
        ),
    ]
    metric_specs.extend(
        (f"owner_{alias}", f"owner {alias}", "lower_is_better")
        for _, alias in OWNER_METRIC_SPECS
    )
    metric_specs.extend(
        (f"shape_{alias}", f"shape drift {alias}", "lower_is_better")
        for key, alias in SHAPE_DRIFT_METRIC_SPECS
        if key in BASELINE_GATE_SHAPE_KEYS
    )

    rows: list[dict[str, Any]] = []
    improved_count = 0
    degraded_count = 0
    unchanged_count = 0
    tolerance = 1e-9

    for key, label, direction in metric_specs:
        prev = _safe_float(previous_metrics.get(key, 0.0), 0.0)
        cur = _safe_float(current_metrics.get(key, 0.0), 0.0)
        delta = cur - prev
        if abs(delta) <= tolerance:
            status = "unchanged"
            unchanged_count += 1
        elif direction == "higher_is_better":
            if delta > 0:
                status = "improved"
                improved_count += 1
            else:
                status = "degraded"
                degraded_count += 1
        else:
            if delta < 0:
                status = "improved"
                improved_count += 1
            else:
                status = "degraded"
                degraded_count += 1

        rows.append(
            {
                "key": key,
                "label": label,
                "direction": direction,
                "previous": round(prev, 6),
                "current": round(cur, 6),
                "delta": round(delta, 6),
                "status": status,
            }
        )

    previous_generated_at = _lookup_path(previous_summary_payload, ("summary", "generated_at"), "unknown")
    current_generated_at = _lookup_path(current_benchmark, ("summary", "generated_at"), "unknown")
    degraded_functions = collect_top_degraded_functions_vs_previous(
        current_benchmark,
        previous_summary_payload,
        limit=20,
        similarity_drop_pp_threshold=1.0,
    )
    return {
        "previous_generated_at": previous_generated_at,
        "current_generated_at": current_generated_at,
        "improved_count": improved_count,
        "degraded_count": degraded_count,
        "unchanged_count": unchanged_count,
        "metrics": rows,
        "degraded_functions": degraded_functions,
        "overall": (
            "improved"
            if improved_count > degraded_count
            else "degraded"
            if degraded_count > improved_count
            else "mixed"
        ),
    }


def write_previous_comparison_files(
    output_dir: Path,
    comparison: dict[str, Any],
) -> tuple[Path, Path]:
    compare_json_path = output_dir / "benchmark_delta_vs_previous.json"
    compare_md_path = output_dir / "benchmark_delta_vs_previous.md"

    with compare_json_path.open("w", encoding="utf-8") as fh:
        json.dump(comparison, fh, indent=2)

    compare_md_path.write_text(
        render_previous_comparison_markdown(comparison),
        encoding="utf-8",
    )

    return compare_json_path, compare_md_path


def write_baseline_regression_files(
    output_dir: Path,
    report: dict[str, Any],
) -> tuple[Path, Path]:
    report_json_path = output_dir / "benchmark_regression_gate.json"
    report_md_path = output_dir / "benchmark_regression_gate.md"

    with report_json_path.open("w", encoding="utf-8") as fh:
        json.dump(report, fh, indent=2)

    report_md_path.write_text(
        render_baseline_regression_markdown(report),
        encoding="utf-8",
    )

    return report_json_path, report_md_path


def print_previous_comparison_summary(comparison: dict[str, Any]) -> None:
    improved = int(comparison.get("improved_count", 0) or 0)
    degraded = int(comparison.get("degraded_count", 0) or 0)
    unchanged = int(comparison.get("unchanged_count", 0) or 0)
    print(
        "Previous comparison: "
        f"overall={comparison.get('overall', 'mixed')}, "
        f"improved={improved}, degraded={degraded}, unchanged={unchanged}"
    )
    degraded_rows = [row for row in comparison.get("metrics", []) if row.get("status") == "degraded"]
    if degraded_rows:
        top = degraded_rows[:3]
        print("  Top degraded metrics:")
        for row in top:
            print(
                "   - "
                f"{row.get('label', row.get('key', 'metric'))}: "
                f"{_safe_float(row.get('previous', 0.0), 0.0):.6f} -> "
                f"{_safe_float(row.get('current', 0.0), 0.0):.6f} "
                f"(Δ={_safe_float(row.get('delta', 0.0), 0.0):+.6f})"
            )
    degraded_functions = comparison.get("degraded_functions", {})
    if isinstance(degraded_functions, dict):
        print(
            "  Degraded functions: "
            f"{int(degraded_functions.get('degraded_function_count', 0) or 0)} "
            "(vs previous run)"
        )


def resolve_ghidra_dir(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value.expanduser().resolve())

    env_dir = os.environ.get("GHIDRA_INSTALL_DIR")
    if env_dir:
        candidates.append(Path(env_dir).expanduser().resolve())

    candidates.extend(path.resolve() for path in DEFAULT_GHIDRA_DIRS)

    for candidate in candidates:
        if candidate.exists():
            return candidate

    checked = ", ".join(str(path) for path in candidates if path)
    raise FileNotFoundError(
        "Ghidra installation directory not found. "
        f"Checked: {checked if checked else '(none)'}"
    )


def resolve_fission_bin(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value.expanduser().resolve())

    candidates.extend(
        [
            (ROOT_DIR / "target" / "debug" / "fission_cli").resolve(),
            (ROOT_DIR / "target" / "release" / "fission_cli").resolve(),
        ]
    )

    for candidate in candidates:
        if candidate.is_file():
            return candidate

    checked = ", ".join(str(path) for path in candidates if path)
    raise FileNotFoundError(
        "fission_cli binary not found. "
        "Build it first (Rust-only path is supported). "
        f"Checked: {checked if checked else '(none)'}"
    )


def ensure_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def add_library_search_path(env: dict[str, str], key: str, value: str) -> None:
    current = env.get(key, "")
    env[key] = value if not current else f"{value}{os.pathsep}{current}"


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"

    text = str(value).strip()
    if not text:
        return "0x0"

    if text.lower().startswith("0x"):
        return f"0x{int(text, 16):x}"

    return f"0x{int(text, 16):x}"


def build_seeded_function_set(
    binary_path: Path,
    fission_bin: Path,
    limit: int | None,
    timeout_sec: int,
) -> list[tuple[str, str]]:
    discovered = list_functions_with_fission(
        ROOT_DIR,
        binary_path,
        fission_bin,
        timeout_sec=timeout_sec,
    )
    if not discovered:
        return []
    sampled = sample_functions(
        binary_path.name,
        discovered,
        limit or len(discovered),
        {},
    )
    return [(canonical_address(address), name) for address, name in sampled]


def build_address_alignment_summary(
    left_addrs: list[str] | set[str],
    right_addrs: list[str] | set[str],
) -> dict[str, Any]:
    left_norm = {canonical_address(addr) for addr in left_addrs}
    right_norm = {canonical_address(addr) for addr in right_addrs}
    shared = sorted(left_norm & right_norm, key=lambda addr: int(addr, 16))
    left_only = sorted(left_norm - right_norm, key=lambda addr: int(addr, 16))
    right_only = sorted(right_norm - left_norm, key=lambda addr: int(addr, 16))
    return {
        "left_total_count": len(left_norm),
        "right_total_count": len(right_norm),
        "shared_count": len(shared),
        "left_only_count": len(left_only),
        "right_only_count": len(right_only),
        "shared_of_left_pct": round((len(shared) / max(len(left_norm), 1)) * 100.0, 3),
        "shared_of_right_pct": round((len(shared) / max(len(right_norm), 1)) * 100.0, 3),
        "coverage_ratio_pct": round((len(shared) / max(len(left_norm), len(right_norm), 1)) * 100.0, 3),
        "shared_addresses": shared,
        "left_only": left_only,
        "right_only": right_only,
    }


def normalize_code(code: str | None) -> str:
    if not code:
        return ""

    text = code.replace("\r\n", "\n")
    text = BLOCK_COMMENT_RE.sub(" ", text)
    text = LINE_COMMENT_RE.sub("", text)
    text = AUTO_FUNC_RE.sub("FUNC", text)
    text = AUTO_SYMBOL_RE.sub("SYM", text)
    text = AUTO_VAR_RE.sub("VAR", text)
    text = HEX_RE.sub("HEX", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text


def classify_decompilation_result(
    code: str | None,
    error: str | None,
    reported_success: bool,
) -> tuple[bool, str | None]:
    if error:
        return False, "explicit_error"
    if code and code.lstrip().startswith(SYNTHETIC_FAILURE_PREFIX):
        return False, "synthetic_failure"
    if reported_success:
        return True, None
    return False, "unknown_failure"


def similarity_percent(left: str, right: str) -> float:
    if not left and not right:
        return 100.0
    if not left or not right:
        return 0.0
    if _rapidfuzz_fuzz is not None:
        return round(float(_rapidfuzz_fuzz.ratio(left, right)), 2)
    return round(difflib.SequenceMatcher(None, left, right).ratio() * 100.0, 2)


NATIVE_TIMING_PHASE_KEYS = (
    "follow_flow_ms",
    "main_perform_ms",
    "analysis_passes_ms",
    "callee_preanalysis_ms",
    "callgraph_reanalysis_ms",
    "print_ms",
    "postprocess_ms",
    "smart_constant_replace_ms",
    "cfg_structurizer_ms",
    "loop_normalize_ms",
    "stage1_rerun_ms",
    "stage2_rerun_ms",
)


def summarize_native_hot_paths(
    entries: dict[str, dict[str, Any]],
    limit: int = 10,
) -> list[dict[str, Any]]:
    hot_rows = sorted(
        (entry for entry in entries.values() if entry.get("native_timing")),
        key=lambda entry: float(entry.get("decomp_sec", 0.0) or 0.0),
        reverse=True,
    )[:limit]

    summary_rows: list[dict[str, Any]] = []
    for entry in hot_rows:
        native_timing = entry.get("native_timing") or {}
        phase_rows = sorted(
            (
                {
                    "phase": phase,
                    "ms": round(float(native_timing.get(phase, 0.0) or 0.0), 3),
                }
                for phase in NATIVE_TIMING_PHASE_KEYS
            ),
            key=lambda row: row["ms"],
            reverse=True,
        )
        summary_rows.append(
            {
                "address": entry.get("address"),
                "name": entry.get("name"),
                "decomp_sec": round(float(entry.get("decomp_sec", 0.0) or 0.0), 6),
                "callee_preanalysis_count": int(
                    native_timing.get("callee_preanalysis_count", 0) or 0
                ),
                "callgraph_reanalysis_count": int(
                    native_timing.get("callgraph_reanalysis_count", 0) or 0
                ),
                "top_native_phases": phase_rows[:3],
            }
        )

    return summary_rows


def run_fission_full(
    binary_path: Path,
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    profile: str,
    function_discovery_profile: str | None,
    compiler_id: str | None,
    struct_ptr_aliases: dict[str, str],
    public_engine: str = "fission",
    limit: int | None = None,
    seeded_functions: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    cli_engine = "mlil_preview"
    raw_output_path = output_dir / f"{public_engine}_full.json"
    stdout_log_path = output_dir / f"{public_engine}_stdout.log"
    stderr_log_path = output_dir / f"{public_engine}_stderr.log"
    temp_output = tempfile.NamedTemporaryFile(prefix="fission-benchmark-", suffix=".json", delete=False)
    temp_output.close()
    temp_addresses: tempfile.NamedTemporaryFile | None = None

    cmd = [
        str(fission_bin),
        str(binary_path),
        "--decomp-all",
        "--engine",
        cli_engine,
        "--benchmark",
        "--ghidra-compat",
        "--profile",
        profile,
        "-o",
        temp_output.name,
    ]
    if function_discovery_profile:
        cmd.extend(["--function-discovery-profile", function_discovery_profile])
    if compiler_id:
        cmd.extend(["--compiler-id", compiler_id])
    if seeded_functions:
        temp_addresses = tempfile.NamedTemporaryFile(
            prefix="fission-benchmark-addresses-",
            suffix=".txt",
            delete=False,
            mode="w",
            encoding="utf-8",
        )
        for address, _name in seeded_functions:
            temp_addresses.write(f"{canonical_address(address)}\n")
        temp_addresses.close()
        cmd.extend(["--addresses-file", temp_addresses.name])
    elif limit is not None:
        cmd.extend(["--decomp-limit", str(limit)])

    env = os.environ.copy()
    bin_dir = str(fission_bin.parent)
    add_library_search_path(env, "DYLD_LIBRARY_PATH", bin_dir)
    add_library_search_path(env, "LD_LIBRARY_PATH", bin_dir)

    pre_activity = collect_macos_activity_snapshot()
    post_activity: dict[str, Any] | None = None
    wall_start = time.perf_counter()
    resources: dict[str, Any] = {}
    try:
        if HAS_PSUTIL:
            popen = subprocess.Popen(
                cmd,
                cwd=ROOT_DIR,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            completed, resources = run_popen_with_resource_monitor(
                popen, timeout_sec=timeout_sec, interval_sec=0.5
            )
        else:
            completed = subprocess.run(
                cmd,
                cwd=ROOT_DIR,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=timeout_sec,
                check=False,
            )
        wall_clock_sec = time.perf_counter() - wall_start
        post_activity = collect_macos_activity_snapshot()
        stdout_log_path.write_text(completed.stdout, encoding="utf-8")
        stderr_log_path.write_text(completed.stderr, encoding="utf-8")
        had_nonzero_exit = completed.returncode != 0
        payload_exists = Path(temp_output.name).exists() and Path(temp_output.name).stat().st_size > 0
        if had_nonzero_exit and not payload_exists:
            raise RuntimeError(
                f"fission_cli failed with exit code {completed.returncode}.\n"
                f"stdout log: {stdout_log_path}\n"
                f"stderr log: {stderr_log_path}\n"
                "tail stdout:\n"
                f"stdout:\n{completed.stdout[-4000:]}\n"
                "tail stderr:\n"
                f"stderr:\n{completed.stderr[-4000:]}"
            )
        shutil.copyfile(temp_output.name, raw_output_path)
        with raw_output_path.open("r", encoding="utf-8") as handle:
            payload = json.load(handle)
    except subprocess.TimeoutExpired as exc:
        post_activity = collect_macos_activity_snapshot()
        partial_stdout = getattr(exc, "stdout", None) or ""
        partial_stderr = getattr(exc, "stderr", None) or ""
        stdout_log_path.write_text(partial_stdout, encoding="utf-8")
        stderr_log_path.write_text(partial_stderr, encoding="utf-8")
        raise RuntimeError(
            f"fission_cli timed out after {timeout_sec}s.\n"
            f"stdout log: {stdout_log_path}\n"
            f"stderr log: {stderr_log_path}"
        ) from exc
    finally:
        if post_activity is None:
            post_activity = collect_macos_activity_snapshot()
        try:
            os.unlink(temp_output.name)
        except FileNotFoundError:
            pass
        if temp_addresses is not None:
            try:
                os.unlink(temp_addresses.name)
            except FileNotFoundError:
                pass

    entries: dict[str, dict[str, Any]] = {}
    for entry in payload.get("functions", []):
        address = canonical_address(entry.get("address", "0x0"))
        code = entry.get("code", "")
        error = entry.get("error")
        reported_success = "error" not in entry
        actual_success, failure_kind = classify_decompilation_result(
            code=code,
            error=error,
            reported_success=reported_success,
        )
        entries[address] = {
            "address": address,
            "name": entry.get("name", ""),
            "present": True,
            "success": actual_success,
            "reported_success": reported_success,
            "code": code,
            "normalized_code": normalize_code(code),
            "error": error,
            "failure_kind": failure_kind,
            "decomp_sec": float(entry.get("decomp_sec", 0.0) or 0.0),
            "native_timing": entry.get("native_timing"),
            "metrics": collect_code_metrics(code, struct_ptr_aliases) if actual_success else {},
            "engine_used": entry.get("engine_used", cli_engine),
            "fell_back": bool(entry.get("fell_back", False)),
            "fallback_kind": entry.get("fallback_reason"),
            "preview_build_stats": entry.get("preview_build_stats"),
        }

    meta = dict(payload.get("_meta", {}))
    meta["wall_clock_sec"] = round(wall_clock_sec, 6)
    meta["raw_output_path"] = str(raw_output_path)
    meta["public_engine"] = public_engine
    meta["cache_mode"] = "warm"
    meta["function_discovery_profile"] = function_discovery_profile
    meta["process_exit_code"] = int(completed.returncode)
    meta["nonzero_exit_with_payload"] = bool(completed.returncode != 0)
    if seeded_functions:
        meta["seeded_addresses"] = [canonical_address(address) for address, _ in seeded_functions]
        meta["seeded_function_count"] = len(seeded_functions)
        meta["independent_top_n_addresses"] = [canonical_address(address) for address, _ in seeded_functions]
    combined_resources = dict(resources) if resources else {}
    if pre_activity:
        combined_resources["macos_activity_pre"] = pre_activity
    if post_activity:
        combined_resources["macos_activity_post"] = post_activity
    activity_delta = summarize_macos_activity_delta(pre_activity, post_activity)
    if activity_delta:
        combined_resources["macos_activity_delta"] = activity_delta
    if combined_resources:
        meta["resources"] = combined_resources

    return {
        "meta": meta,
        "entries": entries,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def run_ghidra_full(
    binary_path: Path,
    ghidra_dir: Path,
    output_dir: Path,
    per_function_timeout_sec: int,
    skip_thunks: bool,
    struct_ptr_aliases: dict[str, str],
    limit: int | None = None,
    cache_dir: Path | None = None,
    use_cache: bool = False,
    save_cache: bool = False,
    seeded_functions: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    # ── Cache hit: skip live Ghidra run ──────────────────────────────────────
    if use_cache and cache_dir is not None:
        cache_path = resolve_ghidra_cache_path(cache_dir, binary_path, ghidra_dir, limit)
        if cache_path.is_file():
            print(f"[*] Ghidra cache hit: {cache_path.name}")
            return load_ghidra_cache(cache_path)
        else:
            print(f"[*] Ghidra cache miss ({cache_path.name}); running live Ghidra …")

    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)

    try:
        import pyghidra
    except ImportError as exc:
        raise RuntimeError("pyghidra is not installed. Run `pip install pyghidra`.") from exc

    if seeded_functions:
        os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)
        pyghidra.start()
        raw_output_path = output_dir / "ghidra_full.json"
        all_function_addrs: list[str] = []
        with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
            program = flat_api.getCurrentProgram()
            function_manager = program.getFunctionManager()
            functions = list(function_manager.getFunctions(True))
            functions.sort(key=lambda func: int(func.getEntryPoint().getOffset()))
            for func in functions:
                try:
                    if func.isExternal():
                        continue
                except Exception:
                    pass
                try:
                    if skip_thunks and func.isThunk():
                        continue
                except Exception:
                    pass
                all_function_addrs.append(canonical_address(int(func.getEntryPoint().getOffset())))
                if limit is not None and len(all_function_addrs) >= limit:
                    break

        meta, results = run_ghidra_binary_with_meta(
            binary_path,
            seeded_functions,
            ghidra_dir,
            per_function_timeout_sec,
            struct_ptr_aliases,
        )
        entries: dict[str, dict[str, Any]] = {}
        function_rows: list[dict[str, Any]] = []
        total_decomp_sec = 0.0
        for address, name in seeded_functions:
            normalized = normalize_address(address)
            result = results.get(normalized, {})
            code = str(result.get("code", "") or "")
            present = result.get("failure_kind") != "missing_function"
            entry = {
                "address": canonical_address(address),
                "name": result.get("name", name),
                "present": bool(present),
                "success": bool(result.get("success", False)),
                "reported_success": bool(result.get("success", False)),
                "code": code,
                "normalized_code": normalize_code(code),
                "error": result.get("error"),
                "failure_kind": result.get("failure_kind"),
                "decomp_sec": round(float(result.get("decomp_sec", 0.0) or 0.0), 6),
                "metrics": result.get("metrics", {}) if result.get("success") else {},
                "engine_used": "pyghidra",
                "fell_back": False,
            }
            total_decomp_sec += float(entry["decomp_sec"])
            entries[entry["address"]] = entry
            function_rows.append(entry)

        payload = {
            "_meta": {
                "seeded_addresses": [canonical_address(address) for address, _ in seeded_functions],
                "seeded_function_count": len(seeded_functions),
                "independent_top_n_addresses": all_function_addrs,
                "wall_clock_sec": round(float(meta.get("wall_sec", 0.0) or 0.0), 6),
                "init_sec": round(float(meta.get("init_sec", 0.0) or 0.0), 6),
                "total_decomp_sec": round(total_decomp_sec, 6),
                "public_engine": "pyghidra",
                "cache_mode": "warm",
                "resources": meta.get("resources", {}),
                "seeded_pairing": True,
            },
            "functions": function_rows,
        }
        raw_output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        return {
            "meta": payload["_meta"],
            "entries": entries,
            "stdout": "",
            "stderr": "",
        }

    pyghidra.start()

    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    monitor = ConsoleTaskMonitor()
    raw_output_path = output_dir / "ghidra_full.json"
    pre_activity = collect_macos_activity_snapshot()
    post_activity: dict[str, Any] | None = None
    wall_start = time.perf_counter()

    res_thread = None
    res_holder: dict[str, Any] = {}
    res_stop: threading.Event | None = None
    if HAS_PSUTIL:
        res_thread, res_holder, res_stop = start_self_resource_monitor(interval_sec=0.5)
    init_start = time.perf_counter()
    total_decomp_sec = 0.0
    entries: dict[str, dict[str, Any]] = {}

    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        decomp = DecompInterface()
        decomp.openProgram(program)
        init_sec = time.perf_counter() - init_start

        function_manager = program.getFunctionManager()
        functions = list(function_manager.getFunctions(True))
        functions.sort(key=lambda func: int(func.getEntryPoint().getOffset()))

        for func in functions:
            try:
                if func.isExternal():
                    continue
            except Exception:
                pass

            try:
                if skip_thunks and func.isThunk():
                    continue
            except Exception:
                pass

            address = canonical_address(int(func.getEntryPoint().getOffset()))
            name = str(func.getName())

            start = time.perf_counter()
            code = ""
            error: str | None = None
            success = False

            try:
                result = decomp.decompileFunction(func, per_function_timeout_sec, monitor)
                if result and result.decompileCompleted() and result.getDecompiledFunction():
                    code = str(result.getDecompiledFunction().getC())
                    success = True
                else:
                    if result is not None:
                        error = str(result.getErrorMessage() or "decompile did not complete")
                    else:
                        error = "null decompile result"
            except Exception as exc:
                error = str(exc)

            elapsed = time.perf_counter() - start
            total_decomp_sec += elapsed
            actual_success, failure_kind = classify_decompilation_result(
                code=code,
                error=error,
                reported_success=success,
            )

            entries[address] = {
                "address": address,
                "name": name,
                "success": actual_success,
                "reported_success": success,
                "code": code,
                "normalized_code": normalize_code(code),
                "error": error,
                "failure_kind": failure_kind,
                "decomp_sec": round(elapsed, 6),
                "metrics": collect_code_metrics(code, struct_ptr_aliases) if actual_success else {},
                "engine_used": "pyghidra",
                "fell_back": False,
            }

            if limit is not None and len(entries) >= limit:
                break

        try:
            decomp.dispose()
        except Exception:
            pass

    wall_clock_sec = time.perf_counter() - wall_start
    post_activity = collect_macos_activity_snapshot()

    if HAS_PSUTIL and res_stop is not None and res_thread is not None:
        res_stop.set()
        res_thread.join(timeout=3.0)

    ghidra_resources = dict(res_holder) if res_holder else {}
    if pre_activity:
        ghidra_resources["macos_activity_pre"] = pre_activity
    if post_activity:
        ghidra_resources["macos_activity_post"] = post_activity
    activity_delta = summarize_macos_activity_delta(pre_activity, post_activity)
    if activity_delta:
        ghidra_resources["macos_activity_delta"] = activity_delta

    meta_dict: dict[str, Any] = {
        "tool": "ghidra",
        "backend": "pyghidra",
        "ghidra_install_dir": str(ghidra_dir),
        "function_count": len(entries),
        "init_sec": round(init_sec, 6),
        "total_decomp_sec": round(total_decomp_sec, 6),
        "wall_clock_sec": round(wall_clock_sec, 6),
        "per_function_timeout_sec": per_function_timeout_sec,
        "skip_thunks": skip_thunks,
        "cache_mode": "warm",
    }
    if ghidra_resources:
        meta_dict["resources"] = ghidra_resources

    payload = {
        "_meta": meta_dict,
        "functions": list(entries.values()),
    }
    with raw_output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)

    # ── Optionally persist Ghidra output to cache ─────────────────────────────
    if save_cache and cache_dir is not None:
        cache_path = resolve_ghidra_cache_path(cache_dir, binary_path, ghidra_dir, limit)
        try:
            save_ghidra_cache(cache_path, raw_output_path)
            print(f"[*] Ghidra output cached: {cache_path.name}")
        except Exception as exc:
            print(f"[!] Failed to save Ghidra cache: {exc}", file=sys.stderr)

    return {
        "meta": payload["_meta"],
        "entries": entries,
        "raw_output_path": str(raw_output_path),
    }


def summarize_engine_failures(entries: dict[str, dict[str, Any]]) -> dict[str, Any]:
    success_count = sum(1 for entry in entries.values() if entry.get("success"))
    failure_count = max(len(entries) - success_count, 0)
    return {
        "reported_success_count": sum(1 for entry in entries.values() if entry.get("reported_success", False)),
        "success_count": success_count,
        "failure_count": failure_count,
        "timeout_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "timeout"),
        "explicit_error_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "explicit_error"),
        "synthetic_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "synthetic_failure"),
        "unknown_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "unknown_failure"),
    }


def is_fission_direct_success(entry: dict[str, Any]) -> bool:
    if not entry.get("success") or entry.get("fell_back"):
        return False
    engine_used = str(entry.get("engine_used") or "").strip()
    if not engine_used:
        return False
    return engine_used not in {"pyghidra", "pcode_dump", "legacy"}


def summarize_engine_quality(entries: dict[str, dict[str, Any]], *, fission: bool = False) -> dict[str, Any]:
    success_entries = [entry for entry in entries.values() if entry.get("success")]
    def preview_stat_total(key: str) -> int:
        if not fission:
            return 0
        return sum(
            _safe_int((entry.get("preview_build_stats") or {}).get(key), 0)
            for entry in entries.values()
        )
    goto_values = [int((entry.get("metrics") or {}).get("goto_count", 0)) for entry in success_entries]
    label_values = [int((entry.get("metrics") or {}).get("top_level_label_count", 0)) for entry in success_entries]
    fpu_values = [int((entry.get("metrics") or {}).get("fpu_op_count", 0)) for entry in success_entries]
    jump_table_values = [int((entry.get("metrics") or {}).get("jump_table_count", 0)) for entry in success_entries]
    generic_local_values = [
        int((entry.get("metrics") or {}).get("local_name_generic_count", 0) or 0)
        for entry in success_entries
    ]
    generic_param_values = [
        int((entry.get("metrics") or {}).get("param_name_generic_count", 0) or 0)
        for entry in success_entries
    ]
    avg_line_values = [
        float((entry.get("metrics") or {}).get("avg_line_length_chars", 0) or 0.0)
        for entry in success_entries
    ]
    nesting_values = [
        int((entry.get("metrics") or {}).get("max_brace_nesting_depth", 0) or 0)
        for entry in success_entries
    ]
    comment_ratio_values = [
        float((entry.get("metrics") or {}).get("comment_char_ratio", 0) or 0.0)
        for entry in success_entries
    ]
    local_share_values = [
        float((entry.get("metrics") or {}).get("local_identifier_share", 0) or 0.0)
        for entry in success_entries
    ]
    local_mention_values = [
        int((entry.get("metrics") or {}).get("local_mention_count", 0) or 0)
        for entry in success_entries
    ]
    anti_keys = (
        "nested_cast",
        "line_over_200_chars",
        "ternary_operator",
        "address_of_chain",
        "double_semicolon",
    )
    anti_pattern_totals: dict[str, int] = {k: 0 for k in anti_keys}
    for entry in success_entries:
        ap = (entry.get("metrics") or {}).get("anti_pattern_counts") or {}
        for k in anti_keys:
            anti_pattern_totals[k] += int(ap.get(k, 0) or 0)
    synthetic_helper_values = [
        int((entry.get("metrics") or {}).get("synthetic_helper_call_count", 0) or 0)
        for entry in success_entries
    ]

    return {
        "goto_total": sum(goto_values),
        "goto_median": round(statistics.median(goto_values), 3) if goto_values else 0.0,
        "top_level_label_total": sum(label_values),
        "top_level_label_median": round(statistics.median(label_values), 3) if label_values else 0.0,
        "empty_if_total": sum(int((entry.get("metrics") or {}).get("empty_if_count", 0)) for entry in success_entries),
        "constant_if_total": sum(int((entry.get("metrics") or {}).get("constant_if_count", 0)) for entry in success_entries),
        "direct_success_count": sum(1 for entry in success_entries if fission and is_fission_direct_success(entry)),
        "fpu_op_total": sum(fpu_values),
        "fpu_function_count": sum(1 for value in fpu_values if value > 0),
        "jump_table_total": sum(jump_table_values),
        "jump_table_function_count": sum(1 for value in jump_table_values if value > 0),
        "fell_back_count": sum(1 for entry in entries.values() if fission and entry.get("fell_back")),
        "unsupported_indirect_control_count": preview_stat_total("unsupported_indirect_control_count"),
        "unsupported_indirect_call_count": preview_stat_total("unsupported_indirect_call_count"),
        "unsupported_external_target_count": preview_stat_total("unsupported_external_target_count"),
        "indirect_surface_preserved_count": preview_stat_total("indirect_surface_preserved_count"),
        "indirect_target_set_refined_count": preview_stat_total("indirect_target_set_refined_count"),
        "dispatcher_shape_recovered_count": preview_stat_total("dispatcher_shape_recovered_count"),
        "materialization_stabilized_count": preview_stat_total("materialization_stabilized_count"),
        "proof_payload_direct_emit_count": preview_stat_total("proof_payload_direct_emit_count"),
        "region_proof_candidate_count": preview_stat_total("region_proof_candidate_count"),
        "region_proof_completed_count": preview_stat_total("region_proof_completed_count"),
        "region_emit_ready_failed_count": preview_stat_total("region_emit_ready_failed_count"),
        "switch_emit_ready_failed_count": preview_stat_total("switch_emit_ready_failed_count"),
        "pass_rerun_skipped_by_preservation_count": preview_stat_total("pass_rerun_skipped_by_preservation_count"),
        "dispatcher_proof_completed_count": preview_stat_total("dispatcher_proof_completed_count"),
        "dispatcher_proof_failed_count": preview_stat_total("dispatcher_proof_failed_count"),
        "conditional_region_candidate_count": preview_stat_total("conditional_region_candidate_count"),
        "conditional_region_promoted_count": preview_stat_total("conditional_region_promoted_count"),
        "blockgraph_region_candidate_count": preview_stat_total("blockgraph_region_candidate_count"),
        "blockgraph_region_complete_count": preview_stat_total("blockgraph_region_complete_count"),
        "blockgraph_region_rejected_missing_follow_count": preview_stat_total(
            "blockgraph_region_rejected_missing_follow_count"
        ),
        "blockgraph_region_rejected_must_emit_label_count": preview_stat_total(
            "blockgraph_region_rejected_must_emit_label_count"
        ),
        "blockgraph_region_rejected_middle_ref_count": preview_stat_total(
            "blockgraph_region_rejected_middle_ref_count"
        ),
        "blockgraph_region_rejected_external_ref_count": preview_stat_total(
            "blockgraph_region_rejected_external_ref_count"
        ),
        "blockgraph_region_rejected_join_owner_conflict_count": preview_stat_total(
            "blockgraph_region_rejected_join_owner_conflict_count"
        ),
        "blockgraph_region_rejected_nonterminal_join_count": preview_stat_total(
            "blockgraph_region_rejected_nonterminal_join_count"
        ),
        "blockgraph_region_rejected_follow_owner_conflict_count": preview_stat_total(
            "blockgraph_region_rejected_follow_owner_conflict_count"
        ),
        "blockgraph_region_rejected_emit_ready_count": preview_stat_total(
            "blockgraph_region_rejected_emit_ready_count"
        ),
        "blockgraph_region_rejected_irreducible_count": preview_stat_total(
            "blockgraph_region_rejected_irreducible_count"
        ),
        "guarded_tail_candidate_count": preview_stat_total("guarded_tail_candidate_count"),
        "guarded_tail_promoted_count": preview_stat_total("guarded_tail_promoted_count"),
        "guarded_tail_replacement_plan_rejected_missing_merge_count": preview_stat_total(
            "guarded_tail_replacement_plan_rejected_missing_merge_count"
        ),
        "guarded_tail_replacement_plan_rejected_unstable_read_count": preview_stat_total(
            "guarded_tail_replacement_plan_rejected_unstable_read_count"
        ),
        "guarded_tail_rejected_missing_terminal_join_count": preview_stat_total(
            "guarded_tail_rejected_missing_terminal_join_count"
        ),
        "guarded_tail_rejected_side_entry_conflict_count": preview_stat_total(
            "guarded_tail_rejected_side_entry_conflict_count"
        ),
        "guarded_tail_rejected_alias_interleave_conflict_count": preview_stat_total(
            "guarded_tail_rejected_alias_interleave_conflict_count"
        ),
        "canonicalization_failed_alias_has_nonlocal_ref_count": preview_stat_total(
            "canonicalization_failed_alias_has_nonlocal_ref_count"
        ),
        "canonicalization_failed_alias_has_nonlocal_ref_external_before_count": preview_stat_total(
            "canonicalization_failed_alias_has_nonlocal_ref_external_before_count"
        ),
        "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count": preview_stat_total(
            "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count"
        ),
        "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count": preview_stat_total(
            "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count"
        ),
        "canonicalization_failed_alias_not_fallthrough_count": preview_stat_total(
            "canonicalization_failed_alias_not_fallthrough_count"
        ),
        "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count": preview_stat_total(
            "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count"
        ),
        "canonicalization_failed_alias_not_fallthrough_nested_after_label_count": preview_stat_total(
            "canonicalization_failed_alias_not_fallthrough_nested_after_label_count"
        ),
        "canonicalization_failed_alias_has_multiple_internal_predecessors_count": preview_stat_total(
            "canonicalization_failed_alias_has_multiple_internal_predecessors_count"
        ),
        "canonicalization_failed_payload_crosses_join_count": preview_stat_total(
            "canonicalization_failed_payload_crosses_join_count"
        ),
        "guarded_tail_rejected_ambiguous_follow_count": preview_stat_total(
            "guarded_tail_rejected_ambiguous_follow_count"
        ),
        "candidate_scoped_jump_resolver_count": preview_stat_total("candidate_scoped_jump_resolver_count"),
        "sccp_skipped_by_admission_count": preview_stat_total("sccp_skipped_by_admission_count"),
        "memory_fact_prefilter_skip_count": preview_stat_total("memory_fact_prefilter_skip_count"),
        # Phase 4 pass quality indicators
        "undefined_return_type_total": sum(
            1 for entry in success_entries
            if bool((entry.get("metrics") or {}).get("undefined_return_type", False))
        ),
        "ptr_offset_total": sum(
            int((entry.get("metrics") or {}).get("ptr_offset_count", 0))
            for entry in success_entries
        ),
        "index_expr_total": sum(
            int((entry.get("metrics") or {}).get("index_expr_count", 0))
            for entry in success_entries
        ),
        "unknown_type_var_total": sum(
            int((entry.get("metrics") or {}).get("unknown_type_var_count", 0))
            for entry in success_entries
        ),
        "synthetic_helper_call_total": sum(synthetic_helper_values),
        "generic_local_name_sum": sum(generic_local_values),
        "generic_param_name_sum": sum(generic_param_values),
        "named_local_sum": sum(
            int((entry.get("metrics") or {}).get("named_local_count", 0) or 0)
            for entry in success_entries
        ),
        "named_param_sum": sum(
            int((entry.get("metrics") or {}).get("named_param_count", 0) or 0)
            for entry in success_entries
        ),
        "heuristic_avg_line_length_mean": round(statistics.fmean(avg_line_values), 3)
        if avg_line_values
        else 0.0,
        "heuristic_max_brace_nesting_mean": round(statistics.fmean(nesting_values), 3)
        if nesting_values
        else 0.0,
        "heuristic_max_brace_nesting_median": round(statistics.median(nesting_values), 3)
        if nesting_values
        else 0.0,
        "heuristic_comment_char_ratio_mean": round(statistics.fmean(comment_ratio_values), 6)
        if comment_ratio_values
        else 0.0,
        "heuristic_local_identifier_share_mean": round(statistics.fmean(local_share_values), 4)
        if local_share_values
        else 0.0,
        "heuristic_local_mention_total": sum(local_mention_values),
        "heuristic_anti_pattern_totals": anti_pattern_totals,
        "readability_control_flow_penalty": sum(goto_values) + sum(label_values),
        "readability_name_generic_total": sum(generic_local_values) + sum(generic_param_values),
    }


def percentile(values: list[float], percent: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    if len(sorted_values) == 1:
        return round(sorted_values[0], 6)
    rank = ((len(sorted_values) - 1) * percent) / 100.0
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return round(sorted_values[lower], 6)
    lower_value = sorted_values[lower]
    upper_value = sorted_values[upper]
    interp = lower_value + (upper_value - lower_value) * (rank - lower)
    return round(interp, 6)


def summarize_engine_quality_kpi(
    *,
    function_count: int,
    success_count: int,
    reported_success_count: int,
    timeout_count: int,
    fell_back_count: int = 0,
    direct_success_count: int = 0,
) -> dict[str, float]:
    total = max(function_count, 1)
    return {
        "success_rate_pct": round((success_count / total) * 100.0, 3),
        "failure_rate_pct": round(((function_count - success_count) / total) * 100.0, 3),
        "reported_success_rate_pct": round((reported_success_count / total) * 100.0, 3),
        "timeout_rate_pct": round((timeout_count / total) * 100.0, 3),
        "fallback_rate_pct": round((fell_back_count / total) * 100.0, 3),
        "direct_success_rate_pct": round((direct_success_count / total) * 100.0, 3),
    }


def summarize_engine_performance_kpi(entries: dict[str, dict[str, Any]], wall_sec: float) -> dict[str, float]:
    decomp_samples = [float(entry.get("decomp_sec", 0.0) or 0.0) for entry in entries.values()]
    function_count = len(entries)
    wall = max(float(wall_sec or 0.0), 1e-9)
    p50 = percentile(decomp_samples, 50.0)
    p95 = percentile(decomp_samples, 95.0)
    p99 = percentile(decomp_samples, 99.0)
    return {
        "wall_sec": round(float(wall_sec or 0.0), 6),
        "throughput_func_per_sec": round(function_count / wall, 3),
        "decomp_p50_sec": p50,
        "decomp_p95_sec": p95,
        "decomp_p99_sec": p99,
        "tail_ratio_p99_over_p50": round(p99 / max(p50, 1e-9), 3),
    }


def summarize_engine_memory_kpi(resources: dict[str, Any], function_count: int) -> dict[str, float]:
    total = max(function_count, 1)
    activity_delta = resources.get("macos_activity_delta", {}) if resources else {}
    peak_rss_mb = float(resources.get("max_rss_mb", 0.0) or 0.0)
    avg_rss_mb = float(resources.get("avg_rss_mb", 0.0) or 0.0)
    return {
        "peak_rss_mb": round(peak_rss_mb, 3),
        "avg_rss_mb": round(avg_rss_mb, 3),
        "peak_rss_mb_per_function": round(peak_rss_mb / total, 6),
        "avg_rss_mb_per_function": round(avg_rss_mb / total, 6),
        "vm_pageins_delta": round(float(activity_delta.get("vm_pageins", 0.0) or 0.0), 3),
        "vm_pageouts_delta": round(float(activity_delta.get("vm_pageouts", 0.0) or 0.0), 3),
        "vm_swapins_delta": round(float(activity_delta.get("vm_swapins", 0.0) or 0.0), 3),
        "vm_swapouts_delta": round(float(activity_delta.get("vm_swapouts", 0.0) or 0.0), 3),
        "vm_free_mb_delta": round(float(activity_delta.get("vm_free_mb", 0.0) or 0.0), 3),
    }


def summarize_engine_cpu_kpi(
    resources: dict[str, Any],
    wall_sec: float,
    function_count: int,
    meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    meta = meta or {}
    avg_cpu_pct = float(resources.get("avg_cpu_pct", 0.0) or 0.0)
    max_cpu_pct = float(resources.get("max_cpu_pct", 0.0) or 0.0)
    activity_delta = resources.get("macos_activity_delta", {}) if resources else {}
    estimated_cpu_seconds = max(float(wall_sec or 0.0) * (avg_cpu_pct / 100.0), 0.0)
    process_cpu_seconds = _safe_float(meta.get("cpu_total_sec"), 0.0)
    cpu_seconds_for_efficiency = process_cpu_seconds if process_cpu_seconds > 0.0 else estimated_cpu_seconds
    worker_count = _safe_int(meta.get("worker_count"), 0)
    available_parallelism = _safe_int(meta.get("available_parallelism"), 0)
    return {
        "avg_cpu_pct": round(avg_cpu_pct, 3),
        "max_cpu_pct": round(max_cpu_pct, 3),
        "estimated_cpu_seconds": round(estimated_cpu_seconds, 6),
        "process_cpu_seconds": round(process_cpu_seconds, 6),
        "process_cpu_user_sec": round(_safe_float(meta.get("cpu_user_sec"), 0.0), 6),
        "process_cpu_system_sec": round(_safe_float(meta.get("cpu_system_sec"), 0.0), 6),
        "process_cpu_utilization_pct": round(_safe_float(meta.get("cpu_utilization_pct"), 0.0), 3),
        "process_effective_parallelism": round(_safe_float(meta.get("effective_parallelism"), 0.0), 3),
        "func_per_cpu_second": round(function_count / max(cpu_seconds_for_efficiency, 1e-9), 3),
        "worker_count": worker_count,
        "available_parallelism": available_parallelism,
        "worker_fanout_enabled": bool(meta.get("worker_fanout_enabled", False)),
        "cpu_user_pct_delta": round(float(activity_delta.get("cpu_user_pct", 0.0) or 0.0), 3),
        "cpu_sys_pct_delta": round(float(activity_delta.get("cpu_sys_pct", 0.0) or 0.0), 3),
        "cpu_idle_pct_delta": round(float(activity_delta.get("cpu_idle_pct", 0.0) or 0.0), 3),
        "loadavg_1m_delta": round(float(activity_delta.get("loadavg_1m", 0.0) or 0.0), 3),
    }


def build_two_way_kpi_delta(
    pyghidra: dict[str, Any],
    fission: dict[str, Any],
) -> dict[str, Any]:
    return {
        "quality": {
            "success_rate_pct_diff": round(
                float(fission["quality_kpi"]["success_rate_pct"]) - float(pyghidra["quality_kpi"]["success_rate_pct"]),
                3,
            ),
            "timeout_rate_pct_diff": round(
                float(fission["quality_kpi"]["timeout_rate_pct"]) - float(pyghidra["quality_kpi"]["timeout_rate_pct"]),
                3,
            ),
            "fallback_rate_pct_diff": round(
                float(fission["quality_kpi"]["fallback_rate_pct"]) - float(pyghidra["quality_kpi"]["fallback_rate_pct"]),
                3,
            ),
        },
        "performance": {
            "throughput_speedup_vs_pyghidra": round(
                float(fission["performance_kpi"]["throughput_func_per_sec"])
                / max(float(pyghidra["performance_kpi"]["throughput_func_per_sec"]), 1e-9),
                3,
            ),
            "tail_ratio_diff": round(
                float(fission["performance_kpi"]["tail_ratio_p99_over_p50"])
                - float(pyghidra["performance_kpi"]["tail_ratio_p99_over_p50"]),
                3,
            ),
        },
        "memory": {
            "peak_rss_mb_ratio_fission_over_pyghidra": round(
                float(fission["memory_kpi"]["peak_rss_mb"]) / max(float(pyghidra["memory_kpi"]["peak_rss_mb"]), 1e-9),
                3,
            ),
            "avg_rss_mb_ratio_fission_over_pyghidra": round(
                float(fission["memory_kpi"]["avg_rss_mb"]) / max(float(pyghidra["memory_kpi"]["avg_rss_mb"]), 1e-9),
                3,
            ),
        },
        "cpu": {
            "avg_cpu_pct_diff": round(
                float(fission["cpu_kpi"]["avg_cpu_pct"]) - float(pyghidra["cpu_kpi"]["avg_cpu_pct"]),
                3,
            ),
            "func_per_cpu_second_ratio_fission_over_pyghidra": round(
                float(fission["cpu_kpi"]["func_per_cpu_second"])
                / max(float(pyghidra["cpu_kpi"]["func_per_cpu_second"]), 1e-9),
                3,
            ),
        },
    }


def select_sampled_addresses(shared_addrs: list[str], sample_size: int) -> list[str]:
    if sample_size <= 0 or len(shared_addrs) <= sample_size:
        return shared_addrs

    # Deterministic striding keeps runs reproducible across environments.
    last_idx = len(shared_addrs) - 1
    selected_idx = {
        round(i * last_idx / (sample_size - 1)) for i in range(sample_size)
    }
    return [addr for idx, addr in enumerate(shared_addrs) if idx in selected_idx]


def resolve_pairwise_mode(mode: str, shared_count: int, auto_shared_full_max: int) -> str:
    if mode != "auto":
        return mode
    # Keep auto mode conservative so post-processing doesn't dominate runtime.
    threshold = max(int(auto_shared_full_max or 0), 1)
    return "sampled" if shared_count > threshold else "shared-full"


def build_intersection_kpi_from_pair(pair: dict[str, Any]) -> dict[str, Any]:
    summary = pair.get("summary", {})
    rows = pair.get("comparisons", [])
    both_success_rows = [
        row
        for row in rows
        if row.get("both_success")
    ]
    norm_scores = [float(row.get("normalized_similarity", 0.0) or 0.0) for row in both_success_rows]

    high_agreement = sum(1 for score in norm_scores if score >= 90.0)
    medium_divergence = sum(1 for score in norm_scores if 50.0 <= score < 90.0)
    high_divergence = sum(1 for score in norm_scores if score < 50.0)
    denominator = max(len(norm_scores), 1)

    return {
        "shared_function_count": int(summary.get("shared_count", 0) or 0),
        "both_success_count": int(summary.get("both_success_count", 0) or 0),
        "both_success_rate_pct": round(
            (float(summary.get("both_success_count", 0) or 0) / max(float(summary.get("shared_count", 0) or 1), 1.0))
            * 100.0,
            3,
        ),
        "avg_normalized_similarity": round(statistics.fmean(norm_scores), 2) if norm_scores else 0.0,
        "median_normalized_similarity": round(statistics.median(norm_scores), 2) if norm_scores else 0.0,
        "aggregate_normalized_similarity": round(float(summary.get("aggregate_normalized_similarity", 0.0) or 0.0), 2),
        "high_agreement_pct": round((high_agreement / denominator) * 100.0, 3),
        "medium_divergence_pct": round((medium_divergence / denominator) * 100.0, 3),
        "high_divergence_pct": round((high_divergence / denominator) * 100.0, 3),
        "pairwise_mode": summary.get("pairwise_mode", "unknown"),
        "evaluated_shared_count": int(summary.get("evaluated_shared_count", 0) or 0),
    }


def build_row_fidelity_targets_snapshot(
    pair: dict[str, Any],
    row_targets: list[tuple[str, str]] | None = None,
    watchlist_source: str = "explicit",
    canonical_quality_rows: list[dict[str, Any]] | None = None,
    bootstrap_row_targets: list[dict[str, Any]] | None = None,
    dynamic_watchlist_rows: list[dict[str, Any]] | None = None,
    watchlist_diagnostics: dict[str, Any] | None = None,
) -> dict[str, Any]:
    rows = pair.get("comparisons", [])
    row_map = {
        str(row.get("address", "")): row
        for row in rows
        if isinstance(row, dict) and row.get("address")
    }
    target_rows: list[dict[str, Any]] = []
    targets = (
        list(ROW_FIDELITY_TARGETS)
        if row_targets is None
        else _normalize_row_target_pairs(row_targets)
    )
    selected_reason_by_address: dict[str, str] = {}
    for row in (
        list(canonical_quality_rows or [])
        + list(bootstrap_row_targets or [])
        + list(dynamic_watchlist_rows or [])
    ):
        if not isinstance(row, dict):
            continue
        address = row.get("address")
        if not address:
            continue
        selected_reason_by_address[canonical_address(str(address))] = str(
            row.get("selected_because", "") or ""
        )

    for address, role in targets:
        row = row_map.get(address)
        if row is None:
            target_rows.append(
                {
                    "address": address,
                    "role": role,
                    "present": False,
                    "selected_because": selected_reason_by_address.get(canonical_address(address)),
                }
            )
            continue
        target_rows.append(
            {
                "address": address,
                "role": role,
                "present": True,
                "selected_because": selected_reason_by_address.get(canonical_address(address)),
                "pyghidra_name": row.get("pyghidra_name", ""),
                "fission_name": row.get("fission_name", ""),
                "normalized_similarity": round(_safe_float(row.get("normalized_similarity", 0.0), 0.0), 3),
                "both_success": bool(row.get("both_success", False)),
                "fission_unsupported_indirect_control_count": _safe_int(
                    row.get("fission_unsupported_indirect_control_count"), 0
                ),
                "fission_indirect_surface_preserved_count": _safe_int(
                    row.get("fission_indirect_surface_preserved_count"), 0
                ),
                "fission_dispatcher_shape_recovered_count": _safe_int(
                    row.get("fission_dispatcher_shape_recovered_count"), 0
                ),
                "fission_dispatcher_proof_completed_count": _safe_int(
                    row.get("fission_dispatcher_proof_completed_count"), 0
                ),
                "fission_dispatcher_proof_failed_count": _safe_int(
                    row.get("fission_dispatcher_proof_failed_count"), 0
                ),
                "fission_proof_payload_direct_emit_count": _safe_int(
                    row.get("fission_proof_payload_direct_emit_count"), 0
                ),
                "fission_region_proof_candidate_count": _safe_int(
                    row.get("fission_region_proof_candidate_count"), 0
                ),
                "fission_region_proof_completed_count": _safe_int(
                    row.get("fission_region_proof_completed_count"), 0
                ),
                "fission_region_emit_ready_failed_count": _safe_int(
                    row.get("fission_region_emit_ready_failed_count"), 0
                ),
                "fission_switch_emit_ready_failed_count": _safe_int(
                    row.get("fission_switch_emit_ready_failed_count"), 0
                ),
                "fission_conditional_region_candidate_count": _safe_int(
                    row.get("fission_conditional_region_candidate_count"), 0
                ),
                "fission_conditional_region_promoted_count": _safe_int(
                    row.get("fission_conditional_region_promoted_count"), 0
                ),
                "fission_guarded_tail_candidate_count": _safe_int(
                    row.get("fission_guarded_tail_candidate_count"), 0
                ),
                "fission_guarded_tail_promoted_count": _safe_int(
                    row.get("fission_guarded_tail_promoted_count"), 0
                ),
                "fission_guarded_tail_rejected_missing_terminal_join_count": _safe_int(
                    row.get("fission_guarded_tail_rejected_missing_terminal_join_count"), 0
                ),
                "fission_guarded_tail_rejected_side_entry_conflict_count": _safe_int(
                    row.get("fission_guarded_tail_rejected_side_entry_conflict_count"), 0
                ),
                "fission_guarded_tail_rejected_alias_interleave_conflict_count": _safe_int(
                    row.get("fission_guarded_tail_rejected_alias_interleave_conflict_count"), 0
                ),
                "fission_guarded_tail_rejected_ambiguous_follow_count": _safe_int(
                    row.get("fission_guarded_tail_rejected_ambiguous_follow_count"), 0
                ),
            }
        )
    return {
        "watchlist_source": watchlist_source,
        "canonical_quality_rows": list(canonical_quality_rows or []),
        "bootstrap_row_targets": list(bootstrap_row_targets or []),
        "dynamic_watchlist_rows": list(dynamic_watchlist_rows or []),
        "watchlist_diagnostics": dict(watchlist_diagnostics or {}),
        "target_count": len(targets),
        "present_count": sum(1 for row in target_rows if row.get("present")),
        "rows": target_rows,
    }


def build_proof_fidelity_summary(pair: dict[str, Any]) -> dict[str, Any]:
    rows = [
        row for row in pair.get("comparisons", [])
        if isinstance(row, dict) and row.get("both_success", False)
    ]

    def _subset(predicate: Any) -> tuple[int, float]:
        matched = [row for row in rows if predicate(row)]
        sims = [_safe_float(row.get("normalized_similarity", 0.0), 0.0) for row in matched]
        return len(matched), round(statistics.fmean(sims), 3) if sims else 0.0

    proof_completed_count, proof_completed_avg = _subset(
        lambda row: _safe_int(row.get("fission_dispatcher_proof_completed_count"), 0) > 0
    )
    proof_failed_count, proof_failed_avg = _subset(
        lambda row: _safe_int(row.get("fission_dispatcher_proof_failed_count"), 0) > 0
    )
    direct_emit_count, direct_emit_avg = _subset(
        lambda row: _safe_int(row.get("fission_proof_payload_direct_emit_count"), 0) > 0
    )
    emit_ready_failed_count, emit_ready_failed_avg = _subset(
        lambda row: _safe_int(row.get("fission_switch_emit_ready_failed_count"), 0) > 0
    )
    guarded_tail_promoted_count, guarded_tail_promoted_avg = _subset(
        lambda row: _safe_int(row.get("fission_guarded_tail_promoted_count"), 0) > 0
    )
    dispatcher_count, dispatcher_avg = _subset(
        lambda row: bool(row.get("fission_has_dispatcher_recovery", False))
    )
    return {
        "proof_completed_row_count": proof_completed_count,
        "proof_completed_row_avg_normalized_similarity": proof_completed_avg,
        "proof_failed_row_count": proof_failed_count,
        "proof_failed_row_avg_normalized_similarity": proof_failed_avg,
        "proof_direct_emit_row_count": direct_emit_count,
        "proof_direct_emit_row_avg_normalized_similarity": direct_emit_avg,
        "switch_emit_ready_failed_row_count": emit_ready_failed_count,
        "switch_emit_ready_failed_row_avg_normalized_similarity": emit_ready_failed_avg,
        "guarded_tail_promoted_row_count": guarded_tail_promoted_count,
        "guarded_tail_promoted_row_avg_normalized_similarity": guarded_tail_promoted_avg,
        "dispatcher_row_count": dispatcher_count,
        "dispatcher_row_avg_normalized_similarity": dispatcher_avg,
    }


def build_residue_family_summary(pair: dict[str, Any]) -> dict[str, Any]:
    rows = [row for row in pair.get("comparisons", []) if isinstance(row, dict)]
    return {
        "unsupported_indirect_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_unresolved_unsupported_indirect", False))
        ),
        "preserved_indirect_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_preserved_indirect_surface", False))
        ),
        "dispatcher_recovery_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_dispatcher_recovery", False))
        ),
        "target_proof_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_indirect_target_proof", False))
        ),
        "proof_failed_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_dispatcher_proof_failed_count"), 0) > 0
        ),
        "proof_completed_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_dispatcher_proof_completed_count"), 0) > 0
        ),
        "switch_emit_ready_failed_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_switch_emit_ready_failed_count"), 0) > 0
        ),
        "guarded_tail_promoted_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_guarded_tail_promoted_count"), 0) > 0
        ),
        "guarded_tail_side_entry_conflict_row_count": sum(
            1
            for row in rows
            if _safe_int(row.get("fission_guarded_tail_rejected_side_entry_conflict_count"), 0) > 0
        ),
        "guarded_tail_alias_interleave_conflict_row_count": sum(
            1
            for row in rows
            if _safe_int(row.get("fission_guarded_tail_rejected_alias_interleave_conflict_count"), 0) > 0
        ),
        "guarded_tail_ambiguous_follow_row_count": sum(
            1
            for row in rows
            if _safe_int(row.get("fission_guarded_tail_rejected_ambiguous_follow_count"), 0) > 0
        ),
        "materialization_stabilized_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_materialization_stabilized_count"), 0) > 0
        ),
    }


def build_perf_admission_summary(fission_summary: dict[str, Any]) -> dict[str, Any]:
    return {
        "candidate_scoped_jump_resolver_count": _safe_int(
            fission_summary.get("candidate_scoped_jump_resolver_count"), 0
        ),
        "sccp_skipped_by_admission_count": _safe_int(
            fission_summary.get("sccp_skipped_by_admission_count"), 0
        ),
        "wide_dead_assignment_rerun_admitted_count": _safe_int(
            fission_summary.get("wide_dead_assignment_rerun_admitted_count"), 0
        ),
        "wide_dead_assignment_rerun_skipped_by_admission_count": _safe_int(
            fission_summary.get("wide_dead_assignment_rerun_skipped_by_admission_count"), 0
        ),
        "memory_fact_prefilter_skip_count": _safe_int(
            fission_summary.get("memory_fact_prefilter_skip_count"), 0
        ),
        "pass_rerun_skipped_by_preservation_count": _safe_int(
            fission_summary.get("pass_rerun_skipped_by_preservation_count"), 0
        ),
    }


def build_layered_quality_from_pair(pair: dict[str, Any]) -> dict[str, Any]:
    rows = pair.get("comparisons", [])
    left_label = str(pair.get("left_label", "left"))
    right_label = str(pair.get("right_label", "right"))
    left_sec_key = f"{left_label}_decomp_sec"
    right_sec_key = f"{right_label}_decomp_sec"

    both_success_rows = [row for row in rows if row.get("both_success")]
    if not both_success_rows:
        return {
            "layering": "max_decomp_sec_terciles",
            "q33_sec": 0.0,
            "q66_sec": 0.0,
            "layers": {},
        }

    complexity_values = [
        max(
            _safe_float(row.get(left_sec_key, 0.0), 0.0),
            _safe_float(row.get(right_sec_key, 0.0), 0.0),
        )
        for row in both_success_rows
    ]
    q33 = percentile(complexity_values, 33.333)
    q66 = percentile(complexity_values, 66.667)

    bucket_rows: dict[str, list[dict[str, Any]]] = {
        "small": [],
        "medium": [],
        "large": [],
    }
    for row in both_success_rows:
        complexity = max(
            _safe_float(row.get(left_sec_key, 0.0), 0.0),
            _safe_float(row.get(right_sec_key, 0.0), 0.0),
        )
        if complexity <= q33:
            bucket_rows["small"].append(row)
        elif complexity <= q66:
            bucket_rows["medium"].append(row)
        else:
            bucket_rows["large"].append(row)

    layers: dict[str, Any] = {}
    for name, layer_rows in bucket_rows.items():
        norms = [_safe_float(r.get("normalized_similarity", 0.0), 0.0) for r in layer_rows]
        high_div = sum(1 for score in norms if score < 50.0)
        layers[name] = {
            "count": len(layer_rows),
            "avg_normalized_similarity": round(statistics.fmean(norms), 3) if norms else 0.0,
            "median_normalized_similarity": round(statistics.median(norms), 3) if norms else 0.0,
            "high_divergence_pct": round((high_div / max(len(norms), 1)) * 100.0, 3),
        }

    return {
        "layering": "max_decomp_sec_terciles",
        "q33_sec": round(float(q33), 6),
        "q66_sec": round(float(q66), 6),
        "layers": layers,
    }


def build_pairwise_engine_comparison(
    left_label: str,
    left: dict[str, Any],
    right_label: str,
    right: dict[str, Any],
    pairwise_similarity_mode: str,
    pairwise_sample_size: int,
    pairwise_auto_shared_full_max: int,
    compute_raw_similarity: bool,
    aggregate_similarity_mode: str,
) -> dict[str, Any]:
    left_entries = left["entries"]
    right_entries = right["entries"]
    left_addrs = {
        address for address, entry in left_entries.items() if entry.get("present", True)
    }
    right_addrs = {
        address for address, entry in right_entries.items() if entry.get("present", True)
    }
    shared_addrs = sorted(left_addrs & right_addrs, key=lambda addr: int(addr, 16))
    left_only = sorted(left_addrs - right_addrs, key=lambda addr: int(addr, 16))
    right_only = sorted(right_addrs - left_addrs, key=lambda addr: int(addr, 16))
    effective_mode = resolve_pairwise_mode(
        pairwise_similarity_mode,
        len(shared_addrs),
        pairwise_auto_shared_full_max,
    )
    eval_addrs = (
        select_sampled_addresses(shared_addrs, pairwise_sample_size)
        if effective_mode == "sampled"
        else shared_addrs
    )

    rows: list[dict[str, Any]] = []
    successful_rows: list[dict[str, Any]] = []
    aggregate_left_parts: list[str] = []
    aggregate_right_parts: list[str] = []
    weighted_norm_sum = 0.0
    weighted_norm_weight = 0

    for address in eval_addrs:
        left_entry = left_entries[address]
        right_entry = right_entries[address]
        raw_similarity = (
            similarity_percent(left_entry.get("code", ""), right_entry.get("code", ""))
            if compute_raw_similarity
            else None
        )
        left_norm = left_entry.get("normalized_code", "")
        right_norm = right_entry.get("normalized_code", "")
        normalized_similarity = similarity_percent(left_norm, right_norm)
        both_success = bool(left_entry.get("success") and right_entry.get("success"))
        row = {
            "address": address,
            f"{left_label}_name": left_entry.get("name", ""),
            f"{right_label}_name": right_entry.get("name", ""),
            f"{left_label}_success": left_entry.get("success", False),
            f"{right_label}_success": right_entry.get("success", False),
            f"{left_label}_failure_kind": left_entry.get("failure_kind"),
            f"{right_label}_failure_kind": right_entry.get("failure_kind"),
            f"{left_label}_decomp_sec": left_entry.get("decomp_sec", 0.0),
            f"{right_label}_decomp_sec": right_entry.get("decomp_sec", 0.0),
            "raw_similarity": raw_similarity,
            "normalized_similarity": normalized_similarity,
            f"{left_label}_error": left_entry.get("error"),
            f"{right_label}_error": right_entry.get("error"),
            "both_success": both_success,
        }
        for label, entry in ((left_label, left_entry), (right_label, right_entry)):
            preview_build_stats = entry.get("preview_build_stats") or {}
            row[f"{label}_unsupported_indirect_control_count"] = _safe_int(
                preview_build_stats.get("unsupported_indirect_control_count"), 0
            )
            row[f"{label}_unsupported_indirect_call_count"] = _safe_int(
                preview_build_stats.get("unsupported_indirect_call_count"), 0
            )
            row[f"{label}_unsupported_external_target_count"] = _safe_int(
                preview_build_stats.get("unsupported_external_target_count"), 0
            )
            row[f"{label}_indirect_surface_preserved_count"] = _safe_int(
                preview_build_stats.get("indirect_surface_preserved_count"), 0
            )
            row[f"{label}_indirect_target_set_refined_count"] = _safe_int(
                preview_build_stats.get("indirect_target_set_refined_count"), 0
            )
            row[f"{label}_dispatcher_shape_recovered_count"] = _safe_int(
                preview_build_stats.get("dispatcher_shape_recovered_count"), 0
            )
            row[f"{label}_dispatcher_proof_completed_count"] = _safe_int(
                preview_build_stats.get("dispatcher_proof_completed_count"), 0
            )
            row[f"{label}_dispatcher_proof_failed_count"] = _safe_int(
                preview_build_stats.get("dispatcher_proof_failed_count"), 0
            )
            row[f"{label}_proof_payload_direct_emit_count"] = _safe_int(
                preview_build_stats.get("proof_payload_direct_emit_count"), 0
            )
            row[f"{label}_region_proof_candidate_count"] = _safe_int(
                preview_build_stats.get("region_proof_candidate_count"), 0
            )
            row[f"{label}_region_proof_completed_count"] = _safe_int(
                preview_build_stats.get("region_proof_completed_count"), 0
            )
            row[f"{label}_region_emit_ready_failed_count"] = _safe_int(
                preview_build_stats.get("region_emit_ready_failed_count"), 0
            )
            row[f"{label}_switch_emit_ready_failed_count"] = _safe_int(
                preview_build_stats.get("switch_emit_ready_failed_count"), 0
            )
            row[f"{label}_materialization_stabilized_count"] = _safe_int(
                preview_build_stats.get("materialization_stabilized_count"), 0
            )
            row[f"{label}_conditional_region_candidate_count"] = _safe_int(
                preview_build_stats.get("conditional_region_candidate_count"), 0
            )
            row[f"{label}_conditional_region_promoted_count"] = _safe_int(
                preview_build_stats.get("conditional_region_promoted_count"), 0
            )
            row[f"{label}_guarded_tail_candidate_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_candidate_count"), 0
            )
            row[f"{label}_guarded_tail_promoted_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_promoted_count"), 0
            )
            row[f"{label}_guarded_tail_replacement_plan_rejected_missing_merge_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_replacement_plan_rejected_missing_merge_count"), 0
            )
            row[f"{label}_guarded_tail_replacement_plan_rejected_unstable_read_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_replacement_plan_rejected_unstable_read_count"), 0
            )
            row[f"{label}_guarded_tail_rejected_missing_terminal_join_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_rejected_missing_terminal_join_count"), 0
            )
            row[f"{label}_guarded_tail_rejected_side_entry_conflict_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_rejected_side_entry_conflict_count"), 0
            )
            row[f"{label}_guarded_tail_rejected_alias_interleave_conflict_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_rejected_alias_interleave_conflict_count"), 0
            )
            row[f"{label}_guarded_tail_rejected_ambiguous_follow_count"] = _safe_int(
                preview_build_stats.get("guarded_tail_rejected_ambiguous_follow_count"), 0
            )
            row[f"{label}_pass_rerun_skipped_by_preservation_count"] = _safe_int(
                preview_build_stats.get("pass_rerun_skipped_by_preservation_count"), 0
            )
            row[f"{label}_candidate_scoped_jump_resolver_count"] = _safe_int(
                preview_build_stats.get("candidate_scoped_jump_resolver_count"), 0
            )
            row[f"{label}_sccp_skipped_by_admission_count"] = _safe_int(
                preview_build_stats.get("sccp_skipped_by_admission_count"), 0
            )
            row[f"{label}_memory_fact_prefilter_skip_count"] = _safe_int(
                preview_build_stats.get("memory_fact_prefilter_skip_count"), 0
            )
            for flag_name, flag_value in _canonical_indirect_flags(preview_build_stats).items():
                row[f"{label}_{flag_name}"] = flag_value
        rows.append(row)
        if both_success:
            successful_rows.append(row)
            if aggregate_similarity_mode == "sequence":
                aggregate_left_parts.append(left_norm)
                aggregate_right_parts.append(right_norm)
            weight = max(len(left_norm), len(right_norm), 1)
            weighted_norm_sum += normalized_similarity * weight
            weighted_norm_weight += weight

    normalized_scores = [row["normalized_similarity"] for row in successful_rows]
    raw_scores = [
        row["raw_similarity"]
        for row in successful_rows
        if row.get("raw_similarity") is not None
    ]
    if aggregate_similarity_mode == "sequence":
        aggregate_normalized_similarity = similarity_percent(
            "\n".join(aggregate_left_parts),
            "\n".join(aggregate_right_parts),
        )
    else:
        aggregate_normalized_similarity = round(
            weighted_norm_sum / max(weighted_norm_weight, 1),
            2,
        )

    return {
        "left_label": left_label,
        "right_label": right_label,
        "comparisons": rows,
        "left_only": left_only,
        "right_only": right_only,
        "summary": {
            "left_total_count": len(left_addrs),
            "right_total_count": len(right_addrs),
            "shared_count": len(shared_addrs),
            "left_only_count": len(left_only),
            "right_only_count": len(right_only),
            "shared_of_left_pct": round((len(shared_addrs) / max(len(left_addrs), 1)) * 100.0, 3),
            "shared_of_right_pct": round((len(shared_addrs) / max(len(right_addrs), 1)) * 100.0, 3),
            "coverage_ratio_pct": round((len(shared_addrs) / max(len(left_addrs), len(right_addrs), 1)) * 100.0, 3),
            "both_success_count": len(successful_rows),
            "aggregate_normalized_similarity": aggregate_normalized_similarity,
            "aggregate_normalized_similarity_method": aggregate_similarity_mode,
            "avg_normalized_similarity": round(statistics.fmean(normalized_scores), 2) if normalized_scores else 0.0,
            "median_normalized_similarity": round(statistics.median(normalized_scores), 2) if normalized_scores else 0.0,
            "avg_raw_similarity": round(statistics.fmean(raw_scores), 2) if raw_scores else 0.0,
            "pairwise_mode": effective_mode,
            "evaluated_shared_count": len(eval_addrs),
            "raw_similarity_enabled": bool(compute_raw_similarity),
        },
    }


def build_comparison(
    binary_path: Path,
    pyghidra: dict[str, Any],
    fission: dict[str, Any],
    pairwise_similarity_mode: str,
    pairwise_sample_size: int,
    pairwise_auto_shared_full_max: int,
    raw_similarity: bool,
    aggregate_similarity_mode: str,
    row_fidelity_targets_filter: list[tuple[str, str]] | None = None,
    watchlist_source: str = "explicit",
    canonical_quality_rows: list[dict[str, Any]] | None = None,
    bootstrap_row_targets: list[dict[str, Any]] | None = None,
    dynamic_watchlist_rows: list[dict[str, Any]] | None = None,
    watchlist_diagnostics: list[dict[str, Any]] | dict[str, Any] | None = None,
) -> dict[str, Any]:
    pair_py_fission = build_pairwise_engine_comparison(
        "pyghidra",
        pyghidra,
        "fission",
        fission,
        pairwise_similarity_mode=pairwise_similarity_mode,
        pairwise_sample_size=pairwise_sample_size,
        pairwise_auto_shared_full_max=pairwise_auto_shared_full_max,
        compute_raw_similarity=raw_similarity,
        aggregate_similarity_mode=aggregate_similarity_mode,
    )

    py_failures = summarize_engine_failures(pyghidra["entries"])
    py_quality = summarize_engine_quality(pyghidra["entries"])
    py_resources = pyghidra["meta"].get("resources", {})
    py_perf_kpi = summarize_engine_performance_kpi(
        pyghidra["entries"],
        float(pyghidra["meta"].get("wall_clock_sec", 0.0) or 0.0),
    )
    py_mem_kpi = summarize_engine_memory_kpi(py_resources, len(pyghidra["entries"]))
    py_cpu_kpi = summarize_engine_cpu_kpi(
        py_resources,
        float(pyghidra["meta"].get("wall_clock_sec", 0.0) or 0.0),
        len(pyghidra["entries"]),
        pyghidra["meta"],
    )
    py_quality_kpi = summarize_engine_quality_kpi(
        function_count=len(pyghidra["entries"]),
        success_count=int(py_failures["success_count"]),
        reported_success_count=int(py_failures["reported_success_count"]),
        timeout_count=int(py_failures["timeout_count"]),
    )

    fission_failures = summarize_engine_failures(fission["entries"])
    fission_quality = summarize_engine_quality(fission["entries"], fission=True)
    fission_resources = fission["meta"].get("resources", {})
    fission_perf_kpi = summarize_engine_performance_kpi(
        fission["entries"],
        float(fission["meta"].get("wall_clock_sec", 0.0) or 0.0),
    )
    fission_mem_kpi = summarize_engine_memory_kpi(fission_resources, len(fission["entries"]))
    fission_cpu_kpi = summarize_engine_cpu_kpi(
        fission_resources,
        float(fission["meta"].get("wall_clock_sec", 0.0) or 0.0),
        len(fission["entries"]),
        fission["meta"],
    )
    fission_quality_kpi = summarize_engine_quality_kpi(
        function_count=len(fission["entries"]),
        success_count=int(fission_failures["success_count"]),
        reported_success_count=int(fission_failures["reported_success_count"]),
        timeout_count=int(fission_failures["timeout_count"]),
        fell_back_count=int(fission_quality.get("fell_back_count", 0)),
        direct_success_count=int(fission_quality.get("direct_success_count", 0)),
    )
    owner_metrics = _extract_owner_metrics_from_engine_summary(fission_quality)
    shape_drift_metrics = _extract_shape_drift_metrics_from_engine_summary(fission_quality)
    normalize_pass_metrics = _flatten_selected_normalize_pass_metrics(
        _extract_selected_normalize_pass_metrics(fission["meta"].get("preview_build_stats", {}))
    )
    ghidra_action_metrics = _normalize_metric_map_for_json(
        _extract_ghidra_action_metrics(fission["meta"].get("preview_build_stats", {}))
    )
    if not any(ghidra_action_metrics.values()):
        ghidra_action_metrics = _normalize_metric_map_for_json(
            _aggregate_ghidra_action_metrics_from_entries(fission["entries"])
        )
    mir_metrics = _normalize_metric_map_for_json(
        _extract_mir_metrics(fission["meta"].get("preview_build_stats", {}))
    )
    if not any(mir_metrics.values()):
        mir_metrics = _normalize_metric_map_for_json(
            _aggregate_mir_metrics_from_entries(fission["entries"])
        )
    blockgraph_region_metrics = _normalize_metric_map_for_json(
        _extract_blockgraph_region_metrics(fission["meta"].get("preview_build_stats", {}))
    )
    if not any(blockgraph_region_metrics.values()):
        blockgraph_region_metrics = _normalize_metric_map_for_json(
            _aggregate_blockgraph_region_metrics_from_entries(fission["entries"])
        )
    alias_interleave_metrics = _normalize_metric_map_for_json(
        _extract_alias_interleave_metrics(fission["meta"].get("preview_build_stats", {}))
    )
    if not any(alias_interleave_metrics.values()):
        alias_interleave_metrics = _normalize_metric_map_for_json(
            _aggregate_alias_interleave_metrics_from_entries(fission["entries"])
        )
    giant_function_diagnostics = _build_giant_function_diagnostics(fission["entries"])
    target_structuring_rows = _annotate_target_structuring_rows(
        _build_target_structuring_rows(fission["entries"], binary_id=binary_path.stem),
        pairwise_comparisons=pair_py_fission.get("comparisons"),
        engine_entries=fission["entries"],
    )
    unchanged_target_rows = [
        dict(row)
        for row in target_structuring_rows
        if str(row.get("row_gate_status") or "").strip() == "unchanged"
    ]

    engine_kpi = {
        "pyghidra": {
            "quality_kpi": py_quality_kpi,
            "performance_kpi": py_perf_kpi,
            "memory_kpi": py_mem_kpi,
            "cpu_kpi": py_cpu_kpi,
        },
        "fission": {
            "quality_kpi": fission_quality_kpi,
            "performance_kpi": fission_perf_kpi,
            "memory_kpi": fission_mem_kpi,
            "cpu_kpi": fission_cpu_kpi,
        },
    }
    kpi_delta = build_two_way_kpi_delta(engine_kpi["pyghidra"], engine_kpi["fission"])
    intersection_kpi = {
        "pyghidra_vs_fission": build_intersection_kpi_from_pair(pair_py_fission),
    }
    layered_quality = {
        "pyghidra_vs_fission": build_layered_quality_from_pair(pair_py_fission),
    }
    proof_fidelity = {
        "pyghidra_vs_fission": build_proof_fidelity_summary(pair_py_fission),
    }
    residue_families = {
        "pyghidra_vs_fission": build_residue_family_summary(pair_py_fission),
    }
    row_fidelity_targets = {
        "pyghidra_vs_fission": build_row_fidelity_targets_snapshot(
            pair_py_fission,
            row_targets=row_fidelity_targets_filter,
            watchlist_source=watchlist_source,
            canonical_quality_rows=canonical_quality_rows,
            bootstrap_row_targets=bootstrap_row_targets,
            dynamic_watchlist_rows=dynamic_watchlist_rows,
            watchlist_diagnostics=watchlist_diagnostics if isinstance(watchlist_diagnostics, dict) else None,
        ),
    }
    independent_top_n_coverage = build_address_alignment_summary(
        pyghidra["meta"].get("independent_top_n_addresses", list(pyghidra["entries"].keys())),
        fission["meta"].get("independent_top_n_addresses", list(fission["entries"].keys())),
    )

    summary = {
        "binary": str(binary_path),
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "cache_mode": "warm",
        "pairwise_similarity_backend": SIMILARITY_BACKEND,
        "pairwise_raw_similarity_enabled": bool(raw_similarity),
        "pairwise_aggregate_similarity_mode": aggregate_similarity_mode,
        "pairwise_auto_shared_full_max": int(pairwise_auto_shared_full_max),
        "row_fidelity_role_filter": "all",
        "engines": {
            "pyghidra": {
                **pyghidra["meta"],
                "function_count": len(pyghidra["entries"]),
                **py_failures,
                **py_quality,
                **engine_kpi["pyghidra"],
            },
            "fission": {
                **fission["meta"],
                "function_count": len(fission["entries"]),
                **fission_failures,
                **fission_quality,
                **engine_kpi["fission"],
            },
        },
        "owner_metrics": {
            "fission": _normalize_metric_map_for_json(owner_metrics),
        },
        "shape_drift_metrics": {
            "fission": _normalize_metric_map_for_json(shape_drift_metrics),
        },
        "normalize_pass_metrics": {
            "fission": normalize_pass_metrics,
        },
        "ghidra_action_metrics": {
            "fission": ghidra_action_metrics,
        },
        "mir_metrics": {
            "fission": mir_metrics,
        },
        "blockgraph_region_metrics": {
            "fission": blockgraph_region_metrics,
        },
        "alias_interleave_metrics": {
            "fission": alias_interleave_metrics,
        },
        "cpu_metrics": {
            "fission": fission_cpu_kpi,
        },
        "target_structuring_rows": target_structuring_rows,
        "unchanged_target_rows": unchanged_target_rows,
        "giant_function_candidates": giant_function_diagnostics["giant_function_candidates"],
        "giant_function_speed_family_counts": giant_function_diagnostics[
            "giant_function_speed_family_counts"
        ],
        "max_rendered_code_len": giant_function_diagnostics["max_rendered_code_len"],
        "max_structuring_scc_component_count": giant_function_diagnostics[
            "max_structuring_scc_component_count"
        ],
        "max_replacement_plan_candidate_count": giant_function_diagnostics[
            "max_replacement_plan_candidate_count"
        ],
        "max_materialization_stabilized_count": giant_function_diagnostics[
            "max_materialization_stabilized_count"
        ],
        "max_pathological_examples": giant_function_diagnostics["max_pathological_examples"],
        "coverage": {
            "pyghidra_vs_fission": pair_py_fission["summary"],
            "independent_top_n_pyghidra_vs_fission": independent_top_n_coverage,
        },
        "quality": {
            "pyghidra_vs_fission": pair_py_fission["summary"],
        },
        "quality_layers": layered_quality,
        "proof_fidelity": proof_fidelity,
        "residue_families": residue_families,
        "row_fidelity_targets": row_fidelity_targets,
        "resources": {
            "pyghidra": py_resources,
            "fission": fission_resources,
        },
        "kpi": {
            "engines": engine_kpi,
            "delta": kpi_delta,
            "intersection": intersection_kpi,
        },
        "speed": {
            "pyghidra": {
                "init_sec": round(float(pyghidra["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(pyghidra["meta"].get("total_decomp_sec", 0.0)), 6),
                "wall_sec": round(float(pyghidra["meta"].get("wall_clock_sec", 0.0)), 6),
            },
            "fission": {
                "init_sec": round(float(fission["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(fission["meta"].get("total_decomp_sec", 0.0)), 6),
                "postprocess_sec": round(float(fission["meta"].get("total_postprocess_sec", 0.0)), 6),
                "wall_sec": round(float(fission["meta"].get("wall_clock_sec", 0.0)), 6),
                "cpu_total_sec": round(_safe_float(fission["meta"].get("cpu_total_sec"), 0.0), 6),
                "cpu_user_sec": round(_safe_float(fission["meta"].get("cpu_user_sec"), 0.0), 6),
                "cpu_system_sec": round(_safe_float(fission["meta"].get("cpu_system_sec"), 0.0), 6),
                "cpu_utilization_pct": round(_safe_float(fission["meta"].get("cpu_utilization_pct"), 0.0), 3),
                "effective_parallelism": round(_safe_float(fission["meta"].get("effective_parallelism"), 0.0), 3),
                "worker_count": _safe_int(fission["meta"].get("worker_count"), 0),
                "available_parallelism": _safe_int(fission["meta"].get("available_parallelism"), 0),
                "worker_fanout_enabled": bool(fission["meta"].get("worker_fanout_enabled", False)),
                "wall_speedup_vs_pyghidra": round(
                    float(pyghidra["meta"].get("wall_clock_sec", 0.0))
                    / max(float(fission["meta"].get("wall_clock_sec", 0.0)), 1e-9),
                    3,
                ),
            },
        },
        "samples": {
            "pyghidra_vs_fission_lowest_similarity": sorted(pair_py_fission["comparisons"], key=lambda row: row["normalized_similarity"])[:20],
            "fission_hot_path_phases": summarize_native_hot_paths(fission["entries"]),
        },
        "admission_and_preservation": {
            "fission": build_perf_admission_summary(
                {
                    **fission_quality,
                    **engine_kpi["fission"],
                }
            ),
        },
        "public_summary_line": "",
    }

    summary["public_summary_line"] = (
        f"fission vs pyghidra wall speedup {summary['speed']['fission']['wall_speedup_vs_pyghidra']}x; "
        f"throughput speedup {summary['kpi']['delta']['performance']['throughput_speedup_vs_pyghidra']}x; "
        f"seeded shared coverage {summary['coverage']['pyghidra_vs_fission']['coverage_ratio_pct']:.2f}%; "
        f"independent top-N coverage {summary['coverage']['independent_top_n_pyghidra_vs_fission']['coverage_ratio_pct']:.2f}%; "
        f"fission direct-success {summary['engines']['fission']['direct_success_count']}/{summary['engines']['fission']['function_count']}; "
        f"fission unsupported indirect {summary['engines']['fission']['unsupported_indirect_control_count']}; "
        f"fission indirect-surface preserved {summary['engines']['fission']['indirect_surface_preserved_count']}; "
        f"fission jump-table functions {summary['engines']['fission']['jump_table_function_count']}; "
        f"fission dispatcher recovered {summary['engines']['fission']['dispatcher_shape_recovered_count']}"
    )

    return {
        "summary": summary,
        "pairwise": {
            "pyghidra_vs_fission": pair_py_fission,
        },
        "engines": {
            "pyghidra": pyghidra,
            "fission": fission,
        },
    }


def _binary_hot_pass_summary(benchmark: dict[str, Any], limit: int = 8) -> dict[str, Any]:
    hot_rows = _lookup_path(benchmark, ("summary", "samples", "fission_hot_path_phases"), [])
    phase_totals: dict[str, dict[str, float]] = {}
    if isinstance(hot_rows, list):
        for row in hot_rows:
            if not isinstance(row, dict):
                continue
            for phase in row.get("top_native_phases", []):
                if not isinstance(phase, dict):
                    continue
                name = str(phase.get("phase", "")).strip()
                if not name:
                    continue
                slot = phase_totals.setdefault(
                    name,
                    {"total_ms": 0.0, "max_ms": 0.0, "count": 0.0},
                )
                ms = _safe_float(phase.get("ms", 0.0), 0.0)
                slot["total_ms"] += ms
                slot["max_ms"] = max(slot["max_ms"], ms)
                slot["count"] += 1.0
    ranked = sorted(
        (
            {
                "phase": phase,
                "total_ms": round(values["total_ms"], 3),
                "avg_ms": round(values["total_ms"] / max(values["count"], 1.0), 3),
                "max_ms": round(values["max_ms"], 3),
                "observations": int(values["count"]),
            }
            for phase, values in phase_totals.items()
        ),
        key=lambda item: (-item["total_ms"], item["phase"]),
    )
    return {
        "hot_function_count": len(hot_rows) if isinstance(hot_rows, list) else 0,
        "top_phases": ranked[:limit],
    }


def _binary_failure_family_distribution(benchmark: dict[str, Any]) -> dict[str, int]:
    fission_summary = _lookup_path(benchmark, ("summary", "engines", "fission"), {})
    keys = (
        "unsupported_indirect_control_count",
        "indirect_surface_preserved_count",
        "dispatcher_shape_recovered_count",
        "dispatcher_proof_completed_count",
        "dispatcher_proof_failed_count",
        "proof_payload_direct_emit_count",
        "region_proof_candidate_count",
        "region_proof_completed_count",
        "region_emit_ready_failed_count",
        "switch_emit_ready_failed_count",
        "conditional_region_candidate_count",
        "conditional_region_promoted_count",
        "guarded_tail_candidate_count",
        "guarded_tail_promoted_count",
        "guarded_tail_rejected_missing_terminal_join_count",
        "guarded_tail_rejected_side_entry_conflict_count",
        "guarded_tail_rejected_alias_interleave_conflict_count",
        "guarded_tail_rejected_ambiguous_follow_count",
        "materialization_stabilized_count",
        "representative_downgrade_count",
        "representative_downgrade_no_aliassafe_source_count",
        "representative_downgrade_join_conflict_count",
        "preserved_temp_prune_blocked_count",
        "preserved_temp_copyprop_skip_count",
        "gvn_join_preserved_count",
        "candidate_scoped_jump_resolver_count",
        "sccp_skipped_by_admission_count",
        "memory_fact_prefilter_skip_count",
        "pass_rerun_skipped_by_preservation_count",
    )
    distribution = {
        key: _safe_int(fission_summary.get(key), 0)
        for key in keys
    }
    distribution.update(
        {
            "canonical_replacement_incomplete_count": _safe_int(
                fission_summary.get("guarded_tail_replacement_plan_rejected_missing_merge_count"),
                0,
            ),
            "canonical_must_emit_label_conflict_count": _safe_int(
                fission_summary.get("guarded_tail_replacement_plan_rejected_unstable_read_count"),
                0,
            ),
            "canonical_alias_interleave_conflict_count": _safe_int(
                fission_summary.get("guarded_tail_rejected_alias_interleave_conflict_count"),
                0,
            ),
            "canonical_alias_has_nonlocal_ref_count": _safe_int(
                fission_summary.get("canonicalization_failed_alias_has_nonlocal_ref_count"),
                0,
            ),
            "canonical_alias_has_nonlocal_ref_external_before_count": _safe_int(
                fission_summary.get(
                    "canonicalization_failed_alias_has_nonlocal_ref_external_before_count"
                ),
                0,
            ),
            "canonical_alias_has_nonlocal_ref_nested_before_count": _safe_int(
                fission_summary.get(
                    "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count"
                ),
                0,
            ),
            "canonical_alias_has_nonlocal_ref_post_segment_ref_count": _safe_int(
                fission_summary.get(
                    "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count"
                ),
                0,
            ),
            "canonical_alias_not_fallthrough_count": _safe_int(
                fission_summary.get("canonicalization_failed_alias_not_fallthrough_count"),
                0,
            ),
            "canonical_alias_has_multiple_internal_predecessors_count": _safe_int(
                fission_summary.get(
                    "canonicalization_failed_alias_has_multiple_internal_predecessors_count"
                ),
                0,
            ),
            "canonical_payload_crosses_join_count": _safe_int(
                fission_summary.get("canonicalization_failed_payload_crosses_join_count"),
                0,
            ),
            "canonical_side_entry_conflict_count": _safe_int(
                fission_summary.get("guarded_tail_rejected_side_entry_conflict_count"),
                0,
            ),
            "canonical_emit_ready_failed_count": _safe_int(
                fission_summary.get("region_emit_ready_failed_count"),
                0,
            )
            + _safe_int(fission_summary.get("switch_emit_ready_failed_count"), 0),
        }
    )
    return distribution


def _build_arch_summary(
    binaries_payload: list[dict[str, Any]],
    eligibility_by_binary: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    arch_summary: dict[str, Any] = {}
    for arch in ("x86", "x64"):
        arch_items = [item for item in binaries_payload if item.get("arch") == arch]
        weighted_items = [
            item
            for item in arch_items
            if eligibility_by_binary.get(item["id"], {}).get("weighted_for_release")
        ]
        total_weight = sum(int(item.get("weight", 0) or 0) for item in weighted_items)
        weighted_avg = (
            sum(
                _safe_float(item.get("avg_normalized_similarity", 0.0), 0.0)
                * int(item.get("weight", 0) or 0)
                for item in weighted_items
            )
            / max(total_weight, 1)
            if weighted_items
            else 0.0
        )
        owner_metric_totals = _merge_named_metric_totals(
            {
                item["id"]: {
                    key: _safe_float(value, 0.0)
                    for key, value in (item.get("owner_metrics", {}) or {}).items()
                }
                for item in arch_items
            }
        )
        shape_drift_totals = _merge_named_metric_totals(
            {
                item["id"]: {
                    key: _safe_float(value, 0.0)
                    for key, value in (item.get("shape_drift_metrics", {}) or {}).items()
                }
                for item in arch_items
            }
        )
        arch_summary[arch] = {
            "binary_count": len(arch_items),
            "release_candidate_count": sum(
                1 for item in arch_items if eligibility_by_binary.get(item["id"], {}).get("release_scoped")
            ),
            "weighted_avg_normalized_similarity": round(weighted_avg, 3),
            "coverage_non_worse_count": sum(
                1 for item in weighted_items if item.get("coverage_non_worse_vs_baseline") is True
            ),
            "direct_success_non_worse_count": sum(
                1 for item in weighted_items if item.get("direct_success_non_worse_vs_baseline") is True
            ),
            "failed_binary_ids": sorted(
                item["id"] for item in arch_items if item.get("non_worse_vs_baseline") is False
            ),
            "owner_metric_totals": owner_metric_totals,
            "shape_drift_totals": shape_drift_totals,
        }
    return arch_summary


def _classify_row_regression_reasons(row: dict[str, Any]) -> list[str]:
    reasons: list[str] = []
    if _safe_int(row.get("current_representative_downgrade_count"), 0) > _safe_int(
        row.get("previous_representative_downgrade_count"), 0
    ):
        reasons.append("materialization_drift")
    replacement_incomplete = _safe_int(
        row.get("current_guarded_tail_replacement_plan_rejected_missing_merge_count"), 0
    ) > _safe_int(row.get("previous_guarded_tail_replacement_plan_rejected_missing_merge_count"), 0)
    must_emit_label_conflict = _safe_int(
        row.get("current_guarded_tail_replacement_plan_rejected_unstable_read_count"), 0
    ) > _safe_int(row.get("previous_guarded_tail_replacement_plan_rejected_unstable_read_count"), 0)
    if replacement_incomplete:
        reasons.append("replacement_incomplete")
    if must_emit_label_conflict:
        reasons.append("must_emit_label_conflict")
    if _safe_int(
        row.get("current_representative_downgrade_join_conflict_count"), 0
    ) > _safe_int(row.get("previous_representative_downgrade_join_conflict_count"), 0):
        reasons.append("join_alias_drift")
    if _safe_int(row.get("current_region_emit_ready_failed_count"), 0) > _safe_int(
        row.get("previous_region_emit_ready_failed_count"), 0
    ) or _safe_int(row.get("current_switch_emit_ready_failed_count"), 0) > _safe_int(
        row.get("previous_switch_emit_ready_failed_count"), 0
    ):
        if not replacement_incomplete and not must_emit_label_conflict:
            reasons.append("emit_ready_failed")
    if _safe_int(
        row.get("current_guarded_tail_rejected_side_entry_conflict_count"), 0
    ) > _safe_int(row.get("previous_guarded_tail_rejected_side_entry_conflict_count"), 0):
        reasons.append("side_entry_conflict")
    if _safe_int(
        row.get("current_guarded_tail_rejected_alias_interleave_conflict_count"), 0
    ) > _safe_int(row.get("previous_guarded_tail_rejected_alias_interleave_conflict_count"), 0):
        reasons.append("alias_interleave_conflict")
    if _safe_int(
        row.get("current_guarded_tail_rejected_ambiguous_follow_count"), 0
    ) > _safe_int(row.get("previous_guarded_tail_rejected_ambiguous_follow_count"), 0):
        reasons.append("ambiguous_follow")
    if (
        _safe_int(row.get("current_unsupported_indirect_control_count"), 0)
        > _safe_int(row.get("previous_unsupported_indirect_control_count"), 0)
        or _safe_int(row.get("current_indirect_surface_preserved_count"), 0)
        > _safe_int(row.get("previous_indirect_surface_preserved_count"), 0)
        or _safe_int(row.get("current_dispatcher_proof_completed_count"), 0)
        < _safe_int(row.get("previous_dispatcher_proof_completed_count"), 0)
        or _safe_int(row.get("current_proof_payload_direct_emit_count"), 0)
        < _safe_int(row.get("previous_proof_payload_direct_emit_count"), 0)
    ):
        reasons.append("dispatcher_residue")
    if "new_failure" in row.get("failure_reasons", []):
        reasons.append("semantic_divergence")
    if not reasons:
        reasons.append("semantic_divergence")
    return list(dict.fromkeys(reasons))


def _aggregate_row_regression_reasons(
    row_gates: dict[str, dict[str, Any]],
) -> dict[str, int]:
    counts: dict[str, int] = {
        "semantic_divergence": 0,
        "materialization_drift": 0,
        "replacement_incomplete": 0,
        "must_emit_label_conflict": 0,
        "join_alias_drift": 0,
        "dispatcher_residue": 0,
        "emit_ready_failed": 0,
        "side_entry_conflict": 0,
        "alias_interleave_conflict": 0,
        "ambiguous_follow": 0,
        "perf_only_nonsemantic": 0,
    }
    for gate in row_gates.values():
        for row in gate.get("rows", []):
            if not isinstance(row, dict) or row.get("status") != "degraded":
                continue
            for reason in _classify_row_regression_reasons(row):
                counts[reason] = counts.get(reason, 0) + 1
    return counts


def _baseline_reason_distribution(
    baseline_summary_json: dict[str, Any] | None,
) -> dict[str, int]:
    if not isinstance(baseline_summary_json, dict):
        return {}
    corpus_summary = baseline_summary_json.get("corpus_summary")
    if not isinstance(corpus_summary, dict):
        return {}
    reasons = corpus_summary.get("row_regression_reasons", {})
    return reasons if isinstance(reasons, dict) else {}


def _merge_failure_family_distribution(
    per_binary: dict[str, dict[str, int]],
) -> dict[str, int]:
    merged: dict[str, int] = {}
    for counters in per_binary.values():
        for key, value in counters.items():
            merged[key] = merged.get(key, 0) + _safe_int(value, 0)
    return dict(sorted(merged.items()))


def _default_binary_weight(entry: dict[str, Any]) -> int:
    return max(
        _safe_int(
            entry.get("weight"),
            2 if entry.get("role") == "primary_canary" else 1,
        ),
        1,
    )


def _derive_binary_row_targets(
    manifest_entry: dict[str, Any],
    baseline_summary_json: dict[str, Any] | None,
) -> list[tuple[str, str]]:
    return _resolve_binary_watchlist(
        manifest_entry=manifest_entry,
        baseline_summary_json=baseline_summary_json,
        default_row_targets=[],
        dynamic_watchlist_limit=_safe_int(
            manifest_entry.get("dynamic_watchlist_limit"),
            DEFAULT_DYNAMIC_WATCHLIST_LIMIT,
        ),
    )["rows"]


def _entry_is_release_scoped(role: str) -> bool:
    return role in CORPUS_RELEASE_WEIGHT_ROLES


def _evaluate_corpus_entry_eligibility(
    manifest_entry: dict[str, Any],
    benchmark: dict[str, Any],
) -> dict[str, Any]:
    role = _normalize_corpus_role(manifest_entry.get("role"))
    coverage = _lookup_path(
        benchmark,
        ("summary", "coverage", "pyghidra_vs_fission"),
        {},
    )
    coverage = coverage if isinstance(coverage, dict) else {}
    py_engine = _lookup_path(benchmark, ("summary", "engines", "pyghidra"), {})
    fi_engine = _lookup_path(benchmark, ("summary", "engines", "fission"), {})
    py_engine = py_engine if isinstance(py_engine, dict) else {}
    fi_engine = fi_engine if isinstance(fi_engine, dict) else {}

    shared_count = _safe_int(coverage.get("shared_count"), 0)
    left_total_count = _safe_int(coverage.get("left_total_count"), 0)
    right_total_count = _safe_int(coverage.get("right_total_count"), 0)
    seeded_expected = max(
        _safe_int(py_engine.get("seeded_function_count"), 0),
        _safe_int(fi_engine.get("seeded_function_count"), 0),
    )
    direct_success_comparable = (
        _safe_int(py_engine.get("function_count"), 0) > 0
        and _safe_int(fi_engine.get("function_count"), 0) > 0
    )
    min_seed_shared_satisfied = seeded_expected <= 0 or shared_count > 0
    identity_stable = (
        min_seed_shared_satisfied
        and left_total_count > 0
        and right_total_count > 0
    )
    release_scoped = _entry_is_release_scoped(role)
    release_eligible = (
        release_scoped
        and identity_stable
        and direct_success_comparable
    )
    if not release_scoped:
        reason = "role_excluded"
    elif not min_seed_shared_satisfied:
        reason = "seeded_shared_coverage_zero"
    elif not identity_stable:
        reason = "identity_unstable"
    elif not direct_success_comparable:
        reason = "direct_success_not_comparable"
    else:
        reason = "eligible"
    return {
        "release_eligible": release_eligible,
        "release_scoped": release_scoped,
        "weighted_for_release": release_eligible,
        "identity_stable": identity_stable,
        "min_seed_shared_satisfied": min_seed_shared_satisfied,
        "direct_success_comparable": direct_success_comparable,
        "reason": reason,
        "shared_count": shared_count,
        "left_total_count": left_total_count,
        "right_total_count": right_total_count,
        "seeded_function_count": seeded_expected,
    }


def build_corpus_assessment(
    manifest: dict[str, Any],
    binary_results: list[dict[str, Any]],
    baseline_summary_json: dict[str, Any] | None = None,
    baseline_artifact: str | None = None,
) -> dict[str, Any]:
    binaries_payload: list[dict[str, Any]] = []
    row_gates: dict[str, dict[str, Any]] = {}
    degraded_by_binary: dict[str, dict[str, Any]] = {}
    hot_pass_summary: dict[str, dict[str, Any]] = {}
    failure_family_distribution_per_binary: dict[str, dict[str, int]] = {}
    owner_metric_totals_per_binary: dict[str, dict[str, float]] = {}
    shape_drift_totals_per_binary: dict[str, dict[str, float]] = {}
    normalize_pass_metrics_per_binary: dict[str, dict[str, float]] = {}
    ghidra_action_metrics_per_binary: dict[str, dict[str, float]] = {}
    mir_metrics_per_binary: dict[str, dict[str, float]] = {}
    blockgraph_region_metrics_per_binary: dict[str, dict[str, float]] = {}
    alias_interleave_metrics_per_binary: dict[str, dict[str, float]] = {}
    cpu_metrics_per_binary: dict[str, dict[str, float]] = {}
    giant_function_speed_family_counts_per_binary: dict[str, dict[str, int]] = {}
    watchlist_source_per_binary: dict[str, str] = {}
    watchlist_reason_counts: dict[str, int] = {}
    watchlist_reason_by_binary_address: dict[str, dict[str, str]] = {}
    eligibility_by_binary: dict[str, dict[str, Any]] = {}
    total_weight = 0
    weighted_avg_sum = 0.0
    per_binary_non_worse = 0
    coverage_non_worse = 0
    direct_success_non_worse = 0
    release_candidate_count = 0
    release_eligible_count = 0
    suite_tier = _normalize_corpus_suite_tier(manifest.get("suite_tier"))
    gate_mode = _normalize_corpus_gate_mode(manifest.get("gate_mode"))
    comparable_to_baseline = bool(
        isinstance(baseline_summary_json, dict)
        and baseline_summary_json.get("mode") == "corpus"
    )

    for result in binary_results:
        entry = result["manifest_entry"]
        benchmark = result["benchmark"]
        binary_id = str(entry["id"])
        arch = _derive_binary_arch(entry)
        weight = _default_binary_weight(entry)
        avg_norm = _safe_float(
            _lookup_path(
                benchmark,
                ("summary", "quality", "pyghidra_vs_fission", "avg_normalized_similarity"),
                0.0,
            ),
            0.0,
        )
        fission_engine_summary = _lookup_path(benchmark, ("summary", "engines", "fission"), {})
        if not isinstance(fission_engine_summary, dict):
            fission_engine_summary = {}
        owner_metrics = _lookup_path(benchmark, ("summary", "owner_metrics", "fission"), {})
        if not isinstance(owner_metrics, dict):
            owner_metrics = _extract_owner_metrics_from_engine_summary(fission_engine_summary)
        shape_drift_metrics = _lookup_path(
            benchmark,
            ("summary", "shape_drift_metrics", "fission"),
            {},
        )
        if not isinstance(shape_drift_metrics, dict):
            shape_drift_metrics = _extract_shape_drift_metrics_from_engine_summary(
                fission_engine_summary
            )
        owner_metrics = _normalize_metric_map_for_json(
            {
                alias: _safe_float(owner_metrics.get(alias, 0.0), 0.0)
                for _key, alias in OWNER_METRIC_SPECS
            }
        )
        shape_drift_metrics = _normalize_metric_map_for_json(
            {
                alias: _safe_float(shape_drift_metrics.get(alias, 0.0), 0.0)
                for _key, alias in SHAPE_DRIFT_METRIC_SPECS
            }
        )
        normalize_pass_metrics = _flatten_selected_normalize_pass_metrics(
            _extract_selected_normalize_pass_metrics(
                _lookup_path(
                    benchmark,
                    ("summary", "engines", "fission", "preview_build_stats"),
                    {},
                )
            )
        )
        ghidra_action_metrics = _normalize_metric_map_for_json(
            _lookup_path(benchmark, ("summary", "ghidra_action_metrics", "fission"), {})
            if isinstance(
                _lookup_path(benchmark, ("summary", "ghidra_action_metrics", "fission"), {}),
                dict,
            )
            else {}
        )
        if not any(ghidra_action_metrics.values()):
            ghidra_action_metrics = _normalize_metric_map_for_json(
                _extract_ghidra_action_metrics(
                    _lookup_path(
                        benchmark,
                        ("summary", "engines", "fission", "preview_build_stats"),
                        {},
                    )
                )
            )
        if not any(ghidra_action_metrics.values()):
            ghidra_action_metrics = _normalize_metric_map_for_json(
                _aggregate_ghidra_action_metrics_from_entries(
                    _lookup_path(benchmark, ("engines", "fission", "entries"), {})
                )
            )
        mir_metrics = _normalize_metric_map_for_json(
            _lookup_path(benchmark, ("summary", "mir_metrics", "fission"), {})
            if isinstance(
                _lookup_path(benchmark, ("summary", "mir_metrics", "fission"), {}),
                dict,
            )
            else {}
        )
        if not any(mir_metrics.values()):
            mir_metrics = _normalize_metric_map_for_json(
                _extract_mir_metrics(
                    _lookup_path(
                        benchmark,
                        ("summary", "engines", "fission", "preview_build_stats"),
                        {},
                    )
                )
            )
        if not any(mir_metrics.values()):
            mir_metrics = _normalize_metric_map_for_json(
                _aggregate_mir_metrics_from_entries(
                    _lookup_path(benchmark, ("engines", "fission", "entries"), {})
                )
            )
        blockgraph_region_metrics = _normalize_metric_map_for_json(
            _lookup_path(benchmark, ("summary", "blockgraph_region_metrics", "fission"), {})
            if isinstance(
                _lookup_path(benchmark, ("summary", "blockgraph_region_metrics", "fission"), {}),
                dict,
            )
            else {}
        )
        if not any(blockgraph_region_metrics.values()):
            blockgraph_region_metrics = _normalize_metric_map_for_json(
                _extract_blockgraph_region_metrics(
                    _lookup_path(
                        benchmark,
                        ("summary", "engines", "fission", "preview_build_stats"),
                        {},
                    )
                )
            )
        if not any(blockgraph_region_metrics.values()):
            blockgraph_region_metrics = _normalize_metric_map_for_json(
                _aggregate_blockgraph_region_metrics_from_entries(
                    _lookup_path(benchmark, ("engines", "fission", "entries"), {})
                )
            )
        alias_interleave_metrics = _normalize_metric_map_for_json(
            _lookup_path(benchmark, ("summary", "alias_interleave_metrics", "fission"), {})
            if isinstance(
                _lookup_path(benchmark, ("summary", "alias_interleave_metrics", "fission"), {}),
                dict,
            )
            else {}
        )
        if not any(alias_interleave_metrics.values()):
            alias_interleave_metrics = _normalize_metric_map_for_json(
                _extract_alias_interleave_metrics(
                    _lookup_path(
                        benchmark,
                        ("summary", "engines", "fission", "preview_build_stats"),
                        {},
                    )
                )
            )
        if not any(alias_interleave_metrics.values()):
            alias_interleave_metrics = _normalize_metric_map_for_json(
                _aggregate_alias_interleave_metrics_from_entries(
                    _lookup_path(benchmark, ("engines", "fission", "entries"), {})
                )
            )
        cpu_metrics = _normalize_metric_map_for_json(
            _lookup_path(benchmark, ("summary", "kpi", "engines", "fission", "cpu_kpi"), {})
            if isinstance(
                _lookup_path(benchmark, ("summary", "kpi", "engines", "fission", "cpu_kpi"), {}),
                dict,
            )
            else {}
        )
        target_structuring_rows = list(
            _lookup_path(benchmark, ("summary", "target_structuring_rows"), []) or []
        )
        if not target_structuring_rows:
            target_structuring_rows = _build_target_structuring_rows(
                _lookup_path(benchmark, ("engines", "fission", "entries"), {}),
                binary_id=binary_id,
            )
        giant_function_diagnostics = _lookup_path(
            benchmark,
            ("summary",),
            {},
        )
        if not isinstance(giant_function_diagnostics, dict):
            giant_function_diagnostics = {}
        giant_function_diagnostics = {
            "giant_function_candidates": _safe_int(
                giant_function_diagnostics.get("giant_function_candidates"),
                0,
            ),
            "giant_function_speed_family_counts": dict(
                sorted(
                    (
                        giant_function_diagnostics.get(
                            "giant_function_speed_family_counts", {}
                        )
                        if isinstance(
                            giant_function_diagnostics.get(
                                "giant_function_speed_family_counts", {}
                            ),
                            dict,
                        )
                        else {}
                    ).items()
                )
            ),
            "max_rendered_code_len": _safe_int(
                giant_function_diagnostics.get("max_rendered_code_len"),
                0,
            ),
            "max_structuring_scc_component_count": _safe_int(
                giant_function_diagnostics.get("max_structuring_scc_component_count"),
                0,
            ),
            "max_replacement_plan_candidate_count": _safe_int(
                giant_function_diagnostics.get("max_replacement_plan_candidate_count"),
                0,
            ),
            "max_materialization_stabilized_count": _safe_int(
                giant_function_diagnostics.get("max_materialization_stabilized_count"),
                0,
            ),
            "max_pathological_examples": list(
                giant_function_diagnostics.get("max_pathological_examples", []) or []
            ),
        }
        if (
            giant_function_diagnostics["giant_function_candidates"] <= 0
            and not giant_function_diagnostics["giant_function_speed_family_counts"]
        ):
            giant_function_diagnostics = _build_giant_function_diagnostics(
                _lookup_path(benchmark, ("engines", "fission", "entries"), {}),
                binary_id=binary_id,
            )
        owner_metric_totals_per_binary[binary_id] = owner_metrics
        shape_drift_totals_per_binary[binary_id] = shape_drift_metrics
        normalize_pass_metrics_per_binary[binary_id] = normalize_pass_metrics
        ghidra_action_metrics_per_binary[binary_id] = ghidra_action_metrics
        mir_metrics_per_binary[binary_id] = mir_metrics
        blockgraph_region_metrics_per_binary[binary_id] = blockgraph_region_metrics
        alias_interleave_metrics_per_binary[binary_id] = alias_interleave_metrics
        cpu_metrics_per_binary[binary_id] = cpu_metrics
        giant_function_speed_family_counts_per_binary[binary_id] = dict(
            giant_function_diagnostics["giant_function_speed_family_counts"]
        )
        row_watchlist = _lookup_path(
            benchmark,
            ("summary", "row_fidelity_targets", "pyghidra_vs_fission"),
            {},
        )
        if not isinstance(row_watchlist, dict):
            row_watchlist = {}
        watchlist_diagnostics = row_watchlist.get("watchlist_diagnostics", {})
        if not isinstance(watchlist_diagnostics, dict):
            watchlist_diagnostics = {}
        watchlist_source = str(
            watchlist_diagnostics.get("watchlist_source")
            or row_watchlist.get("watchlist_source")
            or "explicit"
        )
        watchlist_source_per_binary[binary_id] = watchlist_source
        selected_because_counts = watchlist_diagnostics.get("selected_because_counts", {})
        if isinstance(selected_because_counts, dict):
            for reason, count in selected_because_counts.items():
                if not reason:
                    continue
                watchlist_reason_counts[str(reason)] = (
                    watchlist_reason_counts.get(str(reason), 0)
                    + _safe_int(count, 0)
                )
        selected_reason_by_address: dict[str, str] = {}
        for row in list(row_watchlist.get("bootstrap_row_targets", []) or []) + list(
            row_watchlist.get("dynamic_watchlist_rows", []) or []
        ):
            if not isinstance(row, dict):
                continue
            address = row.get("address")
            if not address:
                continue
            reason = str(row.get("selected_because", "") or "")
            if reason:
                selected_reason_by_address[canonical_address(str(address))] = reason
        watchlist_reason_by_binary_address[binary_id] = selected_reason_by_address
        hot_pass_summary[binary_id] = _binary_hot_pass_summary(benchmark)
        failure_family_distribution_per_binary[binary_id] = _binary_failure_family_distribution(benchmark)
        eligibility = _evaluate_corpus_entry_eligibility(entry, benchmark)
        eligibility_by_binary[binary_id] = eligibility
        if eligibility.get("release_scoped"):
            release_candidate_count += 1
        if eligibility.get("weighted_for_release"):
            total_weight += weight
            weighted_avg_sum += avg_norm * weight
            release_eligible_count += 1

        baseline_binary_summary = result.get("baseline_summary")

        row_targets = _derive_binary_row_targets(entry, baseline_binary_summary)
        if baseline_binary_summary is not None:
            row_gate = _build_row_fidelity_gate(
                benchmark,
                baseline_binary_summary,
                row_targets=row_targets or None,
            )
            degraded = collect_top_degraded_functions_vs_previous(
                benchmark,
                baseline_binary_summary,
                limit=10,
                similarity_drop_pp_threshold=0.0,
            )
            if not eligibility.get("release_scoped"):
                row_gate = {
                    **row_gate,
                    "status": "report_only",
                    "report_only": True,
                    "base_status": row_gate.get("status", "unknown"),
                }
        else:
            row_gate = (
                {
                    "status": "report_only",
                    "report_only": True,
                    "base_status": "no_baseline",
                    "failed_target_count": 0,
                    "failed_targets": [],
                    "rows": [],
                }
                if not eligibility.get("release_scoped")
                else {
                    "status": "no_baseline",
                    "failed_target_count": 0,
                    "failed_targets": [],
                    "rows": [],
                }
            )
            degraded = {
                "status": "no_baseline",
                "degraded_function_count": 0,
                "top_degraded": [],
                "similarity_drop_pp_threshold": 0.0,
            }
        row_gates[binary_id] = row_gate
        degraded_by_binary[binary_id] = degraded

        coverage = _lookup_path(
            benchmark,
            ("summary", "coverage", "pyghidra_vs_fission", "coverage_ratio_pct"),
            0.0,
        )
        direct_success = (
            f"{_safe_int(_lookup_path(benchmark, ('summary', 'engines', 'fission', 'direct_success_count'), 0), 0)}/"
            f"{_safe_int(_lookup_path(benchmark, ('summary', 'engines', 'fission', 'function_count'), 0), 0)}"
        )
        non_worse = None
        coverage_non_worse_vs_baseline = None
        direct_success_non_worse_vs_baseline = None
        if baseline_binary_summary is not None and eligibility.get("weighted_for_release"):
            baseline_avg = _safe_float(
                _lookup_path(
                    baseline_binary_summary,
                    ("summary", "quality", "pyghidra_vs_fission", "avg_normalized_similarity"),
                    0.0,
                ),
                0.0,
            )
            if avg_norm >= baseline_avg - 1e-9:
                per_binary_non_worse += 1
                non_worse = True
            else:
                non_worse = False

            if _safe_float(
                _lookup_path(
                    baseline_binary_summary,
                    ("summary", "coverage", "pyghidra_vs_fission", "coverage_ratio_pct"),
                    0.0,
                ),
                0.0,
            ) <= _safe_float(coverage, 0.0) + 1e-9:
                coverage_non_worse += 1
                coverage_non_worse_vs_baseline = True
            else:
                coverage_non_worse_vs_baseline = False

            baseline_direct_success = (
                f"{_safe_int(_lookup_path(baseline_binary_summary, ('summary', 'engines', 'fission', 'direct_success_count'), 0), 0)}/"
                f"{_safe_int(_lookup_path(baseline_binary_summary, ('summary', 'engines', 'fission', 'function_count'), 0), 0)}"
            )
            if baseline_direct_success == direct_success:
                direct_success_non_worse += 1
                direct_success_non_worse_vs_baseline = True
            else:
                direct_success_non_worse_vs_baseline = False

        target_structuring_rows = _annotate_target_structuring_rows(
            list(benchmark.get("summary", {}).get("target_structuring_rows", []) or []),
            row_gate=row_gate,
        )

        binaries_payload.append(
            {
                "id": binary_id,
                "binary": str(entry["binary_path"]),
                "arch": arch,
                "ghidra_project_key": entry["ghidra_project_key"],
                "role": entry["role"],
                "tags": list(entry.get("tags", [])),
                "weight": weight,
                "weighted_for_release": bool(eligibility.get("weighted_for_release")),
                "seed_limit": entry.get("seed_limit"),
                "benchmark_summary_path": str(result["summary_json_path"]),
                "benchmark_summary_markdown_path": str(result["summary_md_path"]),
                "output_dir": str(result["output_dir"]),
                "avg_normalized_similarity": round(avg_norm, 3),
                "coverage_ratio_pct": round(_safe_float(coverage, 0.0), 3),
                "direct_success": direct_success,
                "row_fidelity_gate_status": row_gate.get("status", "unknown"),
                "watchlist_source": watchlist_source,
                "bootstrap_row_targets": row_watchlist.get("bootstrap_row_targets", []),
                "dynamic_watchlist_rows": row_watchlist.get("dynamic_watchlist_rows", []),
                "watchlist_diagnostics": {
                    "watchlist_source": watchlist_source,
                    "canonical_quality_row_count": _safe_int(
                        watchlist_diagnostics.get(
                            "canonical_quality_row_count",
                            len(row_watchlist.get("canonical_quality_rows", []) or []),
                        ),
                        len(row_watchlist.get("canonical_quality_rows", []) or []),
                    ),
                    "bootstrap_row_target_count": _safe_int(
                        watchlist_diagnostics.get(
                            "bootstrap_row_target_count",
                            len(row_watchlist.get("bootstrap_row_targets", []) or []),
                        ),
                        len(row_watchlist.get("bootstrap_row_targets", []) or []),
                    ),
                    "dynamic_watchlist_row_count": _safe_int(
                        watchlist_diagnostics.get(
                            "dynamic_watchlist_row_count",
                            len(row_watchlist.get("dynamic_watchlist_rows", []) or []),
                        ),
                        len(row_watchlist.get("dynamic_watchlist_rows", []) or []),
                    ),
                    "selected_because_counts": dict(
                        sorted(
                            (
                                selected_because_counts
                                if isinstance(selected_because_counts, dict)
                                else {}
                            ).items()
                        )
                    ),
                },
                "owner_metrics": owner_metrics,
                "shape_drift_metrics": shape_drift_metrics,
                "normalize_pass_metrics": normalize_pass_metrics,
                "ghidra_action_metrics": ghidra_action_metrics,
                "mir_metrics": mir_metrics,
                "blockgraph_region_metrics": blockgraph_region_metrics,
                "alias_interleave_metrics": alias_interleave_metrics,
                "cpu_metrics": cpu_metrics,
                "target_structuring_rows": target_structuring_rows,
                "giant_function_candidates": giant_function_diagnostics[
                    "giant_function_candidates"
                ],
                "giant_function_speed_family_counts": giant_function_diagnostics[
                    "giant_function_speed_family_counts"
                ],
                "max_rendered_code_len": giant_function_diagnostics[
                    "max_rendered_code_len"
                ],
                "max_structuring_scc_component_count": giant_function_diagnostics[
                    "max_structuring_scc_component_count"
                ],
                "max_replacement_plan_candidate_count": giant_function_diagnostics[
                    "max_replacement_plan_candidate_count"
                ],
                "max_materialization_stabilized_count": giant_function_diagnostics[
                    "max_materialization_stabilized_count"
                ],
                "max_pathological_examples": giant_function_diagnostics[
                    "max_pathological_examples"
                ],
                "non_worse_vs_baseline": non_worse,
                "coverage_non_worse_vs_baseline": coverage_non_worse_vs_baseline,
                "direct_success_non_worse_vs_baseline": direct_success_non_worse_vs_baseline,
                "eligibility": eligibility,
            }
        )

    binaries_payload.sort(key=lambda item: (item["role"] != "primary_canary", item["id"]))
    weighted_avg = weighted_avg_sum / max(total_weight, 1)
    owner_metric_totals = _merge_named_metric_totals(owner_metric_totals_per_binary)
    shape_drift_totals = _merge_named_metric_totals(shape_drift_totals_per_binary)
    normalize_pass_metric_totals = _merge_normalize_pass_metric_totals(
        normalize_pass_metrics_per_binary
    )
    ghidra_action_metric_totals = _merge_named_metric_totals(ghidra_action_metrics_per_binary)
    mir_metric_totals = _merge_named_metric_totals(mir_metrics_per_binary)
    blockgraph_region_metric_totals = _merge_named_metric_totals(
        blockgraph_region_metrics_per_binary
    )
    alias_interleave_metric_totals = _merge_named_metric_totals(
        alias_interleave_metrics_per_binary
    )
    cpu_metric_totals = _merge_cpu_metric_totals(cpu_metrics_per_binary)
    giant_function_speed_family_totals = _merge_count_maps(
        giant_function_speed_family_counts_per_binary
    )
    failure_family_distribution = _merge_failure_family_distribution(
        failure_family_distribution_per_binary
    )
    row_regression_reasons = _aggregate_row_regression_reasons(row_gates)
    cross_binary_degraded = sorted(
        (
            {
                "binary_id": binary_id,
                "selected_because": watchlist_reason_by_binary_address.get(binary_id, {}).get(
                    canonical_address(str(row.get("address", "")))
                ),
                **row,
            }
            for binary_id, degraded in degraded_by_binary.items()
            for row in degraded.get("top_degraded", [])
            if isinstance(row, dict)
        ),
        key=lambda row: (
            _safe_float(row.get("normalized_similarity_delta", 0.0), 0.0),
            row.get("binary_id", ""),
            row.get("address", ""),
        ),
    )[:20]
    max_pathological_examples = sorted(
        (
            {
                **example,
                "binary_id": example.get("binary_id") or item.get("id"),
            }
            for item in binaries_payload
            for example in list(item.get("max_pathological_examples", []) or [])
            if isinstance(example, dict)
        ),
        key=lambda row: (
            -_safe_float(row.get("build_duration_ms"), 0.0),
            -_safe_int(row.get("rendered_code_len"), 0),
            str(row.get("binary_id", "")),
            str(row.get("address", "")),
        ),
    )[:MAX_PATHOLOGICAL_EXAMPLES]
    target_structuring_rows = sorted(
        (
            {
                **row,
                "binary_id": row.get("binary_id") or item.get("id"),
            }
            for item in binaries_payload
            for row in list(item.get("target_structuring_rows", []) or [])
            if isinstance(row, dict)
        ),
        key=lambda row: (
            -_safe_float(row.get("structuring_duration_ms"), 0.0),
            str(row.get("binary_id", "")),
            str(row.get("address", "")),
        ),
    )[:10]
    unchanged_target_rows = [
        dict(row)
        for row in target_structuring_rows
        if str(row.get("row_gate_status") or "").strip() == "unchanged"
    ]
    arch_summary = _build_arch_summary(binaries_payload, eligibility_by_binary)

    status = "passed"
    regressions: list[str] = []
    ineligible_release_binaries = sorted(
        binary_id
        for binary_id, eligibility in eligibility_by_binary.items()
        if eligibility.get("release_scoped") and not eligibility.get("release_eligible")
    )
    if total_weight <= 0:
        status = "failed"
        regressions.append("no release-eligible binaries contributed to weighted corpus gate")
    if ineligible_release_binaries:
        status = "failed"
        regressions.append(
            "release eligibility failed for " + ", ".join(ineligible_release_binaries)
        )
    if baseline_summary_json is not None:
        baseline_weighted = _safe_float(
            _lookup_path(
                baseline_summary_json,
                ("corpus_summary", "weighted_avg_normalized_similarity"),
                0.0,
            ),
            0.0,
        )
        if weighted_avg < baseline_weighted - 1e-9:
            status = "failed"
            regressions.append(
                f"weighted_avg_normalized_similarity: {baseline_weighted:.3f}% -> {weighted_avg:.3f}%"
            )
        failed_binaries = sorted(
            binary_id
            for binary_id, gate in row_gates.items()
            if gate.get("status") == "failed"
            and eligibility_by_binary.get(binary_id, {}).get("release_eligible")
        )
        if failed_binaries:
            status = "failed"
            regressions.append(
                "per-binary row_fidelity_gate failed for " + ", ".join(failed_binaries)
            )
        for item in binaries_payload:
            if item.get("non_worse_vs_baseline") is False:
                status = "failed"
                regressions.append(f"{item['id']} avg_normalized_similarity regressed")
        if coverage_non_worse < len(
            [
                result
                for result in binary_results
                if result.get("baseline_summary") is not None
                and eligibility_by_binary.get(result["manifest_entry"]["id"], {}).get(
                    "weighted_for_release"
                )
            ]
        ):
            status = "failed"
            regressions.append("coverage_ratio_pct regressed for at least one binary")
        if direct_success_non_worse < len(
            [
                result
                for result in binary_results
                if result.get("baseline_summary") is not None
                and eligibility_by_binary.get(result["manifest_entry"]["id"], {}).get(
                    "weighted_for_release"
                )
            ]
        ):
            status = "failed"
            regressions.append("direct_success changed for at least one binary")
        baseline_failure_distribution = baseline_summary_json.get(
            "failure_family_distribution", {}
        )
        if isinstance(baseline_failure_distribution, dict):
            for key in (
                "unsupported_indirect_control_count",
                "representative_downgrade_count",
                "canonical_replacement_incomplete_count",
                "canonical_must_emit_label_conflict_count",
                "canonical_alias_interleave_conflict_count",
                "canonical_side_entry_conflict_count",
                "canonical_emit_ready_failed_count",
            ):
                if _safe_int(failure_family_distribution.get(key), 0) > _safe_int(
                    baseline_failure_distribution.get(key), 0
                ):
                    status = "failed"
                    regressions.append(
                        f"failure_family_distribution {key}: "
                        f"{_safe_int(baseline_failure_distribution.get(key), 0)} -> "
                        f"{_safe_int(failure_family_distribution.get(key), 0)}"
                    )
        baseline_row_reason_distribution = _baseline_reason_distribution(baseline_summary_json)
        for key in (
            "replacement_incomplete",
            "must_emit_label_conflict",
            "alias_interleave_conflict",
            "side_entry_conflict",
            "emit_ready_failed",
        ):
            if _safe_int(row_regression_reasons.get(key), 0) > _safe_int(
                baseline_row_reason_distribution.get(key), 0
            ):
                status = "failed"
                regressions.append(
                    f"row_regression_reasons {key}: "
                    f"{_safe_int(baseline_row_reason_distribution.get(key), 0)} -> "
                    f"{_safe_int(row_regression_reasons.get(key), 0)}"
                )
        baseline_owner_metric_totals = baseline_summary_json.get("owner_metric_totals", {})
        if isinstance(baseline_owner_metric_totals, dict):
            for _metric_key, alias in OWNER_METRIC_SPECS:
                current_value = _safe_float(owner_metric_totals.get(alias, 0.0), 0.0)
                baseline_value = _safe_float(
                    baseline_owner_metric_totals.get(alias, 0.0),
                    0.0,
                )
                if current_value > baseline_value + 1e-9:
                    status = "failed"
                    regressions.append(
                        f"owner_metric_totals {alias}: {baseline_value:.3f} -> {current_value:.3f}"
                    )
        baseline_shape_drift_totals = baseline_summary_json.get("shape_drift_totals", {})
        if isinstance(baseline_shape_drift_totals, dict):
            for _metric_key, alias in SHAPE_DRIFT_METRIC_SPECS:
                if alias not in CORPUS_GATE_SHAPE_KEYS:
                    continue
                current_value = _safe_float(shape_drift_totals.get(alias, 0.0), 0.0)
                baseline_value = _safe_float(
                    baseline_shape_drift_totals.get(alias, 0.0),
                    0.0,
                )
                if current_value > baseline_value + 1e-9:
                    status = "failed"
                    regressions.append(
                        f"shape_drift_totals {alias}: {baseline_value:.3f} -> {current_value:.3f}"
                    )
        baseline_arch_summary = baseline_summary_json.get("arch_summary", {})
        if isinstance(baseline_arch_summary, dict):
            for arch in ("x86", "x64"):
                current_value = _safe_float(
                    _lookup_path(
                        arch_summary,
                        (arch, "weighted_avg_normalized_similarity"),
                        0.0,
                    ),
                    0.0,
                )
                baseline_value = _safe_float(
                    _lookup_path(
                        baseline_arch_summary,
                        (arch, "weighted_avg_normalized_similarity"),
                        0.0,
                    ),
                    0.0,
                )
                if current_value < baseline_value - 1e-9:
                    status = "failed"
                    regressions.append(
                        f"arch_summary.{arch}.weighted_avg_normalized_similarity: "
                        f"{baseline_value:.3f}% -> {current_value:.3f}%"
                    )

    promotion_blockers: list[str] = []
    if gate_mode != "blocking":
        promotion_blockers.append("advisory_gate_mode")
    if not comparable_to_baseline:
        promotion_blockers.append("baseline_not_comparable")
    promotion_blockers.extend(regressions)
    release_promotion_allowed = (
        gate_mode == "blocking"
        and comparable_to_baseline
        and status == "passed"
        and not promotion_blockers
    )

    return {
        "mode": "corpus",
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "suite_tier": suite_tier,
        "gate_mode": gate_mode,
        "comparable_to_baseline": comparable_to_baseline,
        "baseline_artifact": baseline_artifact,
        "release_promotion_allowed": release_promotion_allowed,
        "promotion_blockers": promotion_blockers,
        "manifest": {
            "name": manifest["name"],
            "path": manifest["path"],
            "entry_count": len(manifest["entries"]),
            "suite_tier": suite_tier,
            "gate_mode": gate_mode,
            "dynamic_watchlist_limit": _safe_int(
                manifest.get("dynamic_watchlist_limit"),
                DEFAULT_DYNAMIC_WATCHLIST_LIMIT,
            ),
            "notes": str(manifest.get("notes", "") or ""),
        },
        "corpus_summary": {
            "binary_count": len(binary_results),
            "release_candidate_count": release_candidate_count,
            "release_eligible_count": release_eligible_count,
            "weighted_avg_normalized_similarity": round(weighted_avg, 3),
            "total_weight": total_weight,
            "per_binary_non_worse_count": per_binary_non_worse,
            "coverage_non_worse_count": coverage_non_worse,
            "direct_success_non_worse_count": direct_success_non_worse,
            "status": status,
            "regressions": regressions,
            "row_regression_reasons": row_regression_reasons,
            "suite_tier": suite_tier,
            "gate_mode": gate_mode,
            "comparable_to_baseline": comparable_to_baseline,
            "baseline_artifact": baseline_artifact,
            "release_promotion_allowed": release_promotion_allowed,
            "promotion_blockers": promotion_blockers,
            "owner_metric_totals": owner_metric_totals,
            "shape_drift_totals": shape_drift_totals,
            "normalize_pass_metric_totals": normalize_pass_metric_totals,
            "ghidra_action_metric_totals": ghidra_action_metric_totals,
            "mir_metric_totals": mir_metric_totals,
            "blockgraph_region_metric_totals": blockgraph_region_metric_totals,
            "alias_interleave_metric_totals": alias_interleave_metric_totals,
            "cpu_metric_totals": cpu_metric_totals,
            "giant_function_speed_family_totals": giant_function_speed_family_totals,
            "blockgraph_region_rejection_totals": {
                key: value
                for key, value in blockgraph_region_metric_totals.items()
                if key.startswith("rejected_")
            },
            "target_structuring_rows": target_structuring_rows,
            "unchanged_target_rows": unchanged_target_rows,
            "arch_summary": arch_summary,
            "watchlist_reason_counts": dict(sorted(watchlist_reason_counts.items())),
        },
        "binaries": binaries_payload,
        "eligibility_per_binary": eligibility_by_binary,
        "row_fidelity_gate_per_binary": row_gates,
        "top_degraded_functions_per_binary": degraded_by_binary,
        "failure_family_distribution": failure_family_distribution,
        "failure_family_distribution_per_binary": failure_family_distribution_per_binary,
        "hot_pass_summary_per_binary": hot_pass_summary,
        "owner_metric_totals": owner_metric_totals,
        "owner_metric_totals_per_binary": owner_metric_totals_per_binary,
        "shape_drift_totals": shape_drift_totals,
        "shape_drift_totals_per_binary": shape_drift_totals_per_binary,
        "normalize_pass_metric_totals": normalize_pass_metric_totals,
        "normalize_pass_metrics_per_binary": normalize_pass_metrics_per_binary,
        "ghidra_action_metric_totals": ghidra_action_metric_totals,
        "ghidra_action_metrics_per_binary": ghidra_action_metrics_per_binary,
        "mir_metric_totals": mir_metric_totals,
        "mir_metrics_per_binary": mir_metrics_per_binary,
        "blockgraph_region_metric_totals": blockgraph_region_metric_totals,
        "blockgraph_region_metrics_per_binary": blockgraph_region_metrics_per_binary,
        "alias_interleave_metric_totals": alias_interleave_metric_totals,
        "alias_interleave_metrics_per_binary": alias_interleave_metrics_per_binary,
        "cpu_metric_totals": cpu_metric_totals,
        "cpu_metrics_per_binary": cpu_metrics_per_binary,
        "blockgraph_region_rejection_totals": {
            key: value
            for key, value in blockgraph_region_metric_totals.items()
            if key.startswith("rejected_")
        },
        "target_structuring_rows": target_structuring_rows,
        "unchanged_target_rows": unchanged_target_rows,
        "giant_function_speed_family_totals": giant_function_speed_family_totals,
        "max_pathological_examples": max_pathological_examples,
        "arch_summary": arch_summary,
        "watchlist_source_per_binary": watchlist_source_per_binary,
        "watchlist_reason_counts": dict(sorted(watchlist_reason_counts.items())),
        "cross_binary_degraded_watchlist": cross_binary_degraded,
    }


def write_summary_files(
    output_dir: Path,
    benchmark: dict[str, Any],
) -> tuple[Path, Path]:
    summary_json_path = output_dir / "benchmark_summary.json"
    summary_md_path = output_dir / "benchmark_summary.md"

    with summary_json_path.open("w", encoding="utf-8") as handle:
        json.dump(benchmark, handle, indent=2)
    summary_md_path.write_text(
        render_single_benchmark_markdown(benchmark),
        encoding="utf-8",
    )

    return summary_json_path, summary_md_path


def write_corpus_summary_files(
    output_dir: Path,
    corpus_summary: dict[str, Any],
) -> tuple[Path, Path]:
    summary_json_path = output_dir / "benchmark_summary.json"
    summary_md_path = output_dir / "benchmark_summary.md"

    with summary_json_path.open("w", encoding="utf-8") as handle:
        json.dump(corpus_summary, handle, indent=2)
    summary_md_path.write_text(
        render_corpus_benchmark_markdown(corpus_summary),
        encoding="utf-8",
    )

    return summary_json_path, summary_md_path


def print_console_summary(
    summary: dict[str, Any],
    output_dir: Path,
    baseline_gate: dict[str, Any] | None = None,
) -> None:
    print_single_benchmark_console(summary, output_dir, baseline_gate)


def print_corpus_console_summary(
    corpus_summary: dict[str, Any],
    output_dir: Path,
) -> None:
    print_corpus_benchmark_console(corpus_summary, output_dir)


def _load_previous_summary_payload(output_dir: Path) -> dict[str, Any] | None:
    previous_summary_payload: dict[str, Any] | None = None
    previous_summary_path = output_dir / "benchmark_summary.json"
    if previous_summary_path.is_file():
        try:
            with previous_summary_path.open("r", encoding="utf-8") as fh:
                previous_summary_payload = json.load(fh)
            print(f"[*] Previous summary detected for auto-compare: {previous_summary_path}")
        except Exception as exc:
            print(f"[!] Failed to load previous summary for auto-compare: {exc}", file=sys.stderr)
    return previous_summary_payload


def _resolve_entry_baseline_summary(
    baseline_dir: Path | None,
    binary_id: str | None = None,
) -> dict[str, Any] | None:
    if baseline_dir is None:
        return None
    if binary_id:
        entry_dir = baseline_dir / binary_id
        entry_summary = load_baseline_summary(entry_dir)
        if entry_summary is not None:
            return entry_summary
    baseline_summary = load_baseline_summary(baseline_dir)
    if (
        isinstance(baseline_summary, dict)
        and baseline_summary.get("mode") == "corpus"
        and binary_id
    ):
        return None
    return baseline_summary


def run_single_benchmark(
    *,
    args: argparse.Namespace,
    binary_path: Path,
    ghidra_dir: Path,
    fission_bin: Path,
    output_dir: Path,
    baseline_summary_payload: dict[str, Any] | None = None,
    manifest_entry: dict[str, Any] | None = None,
    enforce_baseline_gate: bool = True,
) -> dict[str, Any]:
    output_dir = ensure_dir(output_dir)
    previous_summary_payload = _load_previous_summary_payload(output_dir)

    watchlist_metadata = _resolve_binary_watchlist(
        manifest_entry=manifest_entry,
        baseline_summary_json=baseline_summary_payload,
        default_row_targets=(
            select_row_fidelity_targets(
                str(getattr(args, "row_fidelity_role_filter", "all"))
            )
            if manifest_entry is not None
            else []
        ),
        dynamic_watchlist_limit=_safe_int(
            manifest_entry.get("dynamic_watchlist_limit") if manifest_entry is not None else None,
            DEFAULT_DYNAMIC_WATCHLIST_LIMIT,
        ),
    )
    row_fidelity_targets_filter = [
        (str(row.get("address", "")), str(row.get("role", "canary")))
        for row in list(watchlist_metadata["rows"])
        if isinstance(row, dict) and row.get("address")
    ]
    baseline_row_targets = row_fidelity_targets_filter or None

    print(f"[*] Binary: {binary_path}")
    print(f"[*] Fission CLI: {fission_bin}")
    print(f"[*] Ghidra dir: {ghidra_dir}")
    print(f"[*] Output dir: {output_dir}")
    if manifest_entry is not None:
        print(
            "[*] Corpus entry: "
            f"id={manifest_entry['id']}, role={manifest_entry['role']}, "
            f"tags={','.join(manifest_entry.get('tags', [])) or 'none'}"
        )

    print(
        "[*] Fission config: "
        f"profile={args.profile}, "
        f"function_discovery_profile={args.function_discovery_profile}"
    )
    print(
        "[*] Pairwise config: "
        f"mode={args.pairwise_similarity_mode}, "
        f"sample_size={args.pairwise_sample_size}, "
        f"auto_shared_full_max={args.pairwise_auto_shared_full_max}, "
        f"raw_similarity={args.raw_similarity}, "
        f"aggregate_mode={args.aggregate_similarity_mode}, "
        f"backend={SIMILARITY_BACKEND}"
    )
    print(
        "[*] Row-fidelity targets: "
        f"filter={getattr(args, 'row_fidelity_role_filter', 'manifest')}, "
        f"count={len(row_fidelity_targets_filter)}, "
        f"source={watchlist_metadata['watchlist_source']}"
    )
    if SIMILARITY_BACKEND != "rapidfuzz":
        print(
            "[!] rapidfuzz not found; falling back to difflib. "
            "For faster similarity matching: pip install rapidfuzz"
        )

    # ── Resolve effective function limit ──────────────────────────────────────
    explicit_limit = (
        args.limit
        if args.limit is not None
        else manifest_entry.get("seed_limit")
        if manifest_entry is not None and manifest_entry.get("seed_limit") is not None
        else None
    )
    effective_limit = resolve_effective_limit(
        binary_path=binary_path,
        fission_bin=fission_bin,
        explicit_limit=explicit_limit,
        auto_limit=getattr(args, "auto_limit_functions", None),
    )
    if effective_limit is not None:
        print(f"[*] Limit: first {effective_limit} canonical seed functions only")

    # ── Ghidra cache configuration ────────────────────────────────────────────
    ghidra_cache_dir: Path | None = getattr(args, "ghidra_cache_dir", None)
    if ghidra_cache_dir is not None:
        ghidra_cache_dir = ghidra_cache_dir.expanduser().resolve()
    use_ghidra_cache: bool = bool(getattr(args, "use_ghidra_cache", False))
    save_ghidra_cache: bool = bool(getattr(args, "save_ghidra_cache", False))
    if use_ghidra_cache and ghidra_cache_dir is None:
        ghidra_cache_dir = ROOT_DIR / "artifacts" / "ghidra_cache"
    if save_ghidra_cache and ghidra_cache_dir is None:
        ghidra_cache_dir = ROOT_DIR / "artifacts" / "ghidra_cache"

    struct_ptr_aliases = load_struct_pointer_aliases(BASE_TYPES_JSON)
    seeded_functions = build_seeded_function_set(
        binary_path=binary_path,
        fission_bin=fission_bin,
        limit=effective_limit,
        timeout_sec=args.timeout,
    )
    print(f"[*] Seeded function set: {len(seeded_functions)} canonical functions")

    fission = run_fission_full(
        binary_path=binary_path,
        fission_bin=fission_bin,
        output_dir=output_dir,
        timeout_sec=args.timeout,
        profile=args.profile,
        function_discovery_profile=args.function_discovery_profile,
        compiler_id=args.compiler_id,
        struct_ptr_aliases=struct_ptr_aliases,
        public_engine="fission",
        limit=effective_limit,
        seeded_functions=seeded_functions,
    )
    print(
        f"[*] Fission complete: functions={len(fission['entries'])}, "
        f"wall={float(fission['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    ghidra = run_ghidra_full(
        binary_path=binary_path,
        ghidra_dir=ghidra_dir,
        output_dir=output_dir,
        per_function_timeout_sec=args.ghidra_func_timeout,
        skip_thunks=args.skip_thunks,
        struct_ptr_aliases=struct_ptr_aliases,
        limit=effective_limit,
        cache_dir=ghidra_cache_dir,
        use_cache=use_ghidra_cache,
        save_cache=save_ghidra_cache,
        seeded_functions=seeded_functions,
    )
    print(
        f"[*] Ghidra complete: functions={len(ghidra['entries'])}, "
        f"wall={float(ghidra['meta'].get('wall_clock_sec', 0.0)):.3f}s"
        + (" (from cache)" if ghidra["meta"].get("from_cache") else "")
    )

    benchmark = build_comparison(
        binary_path,
        ghidra,
        fission,
        pairwise_similarity_mode=args.pairwise_similarity_mode,
        pairwise_sample_size=args.pairwise_sample_size,
        pairwise_auto_shared_full_max=args.pairwise_auto_shared_full_max,
        raw_similarity=bool(args.raw_similarity),
        aggregate_similarity_mode=args.aggregate_similarity_mode,
        row_fidelity_targets_filter=row_fidelity_targets_filter,
        watchlist_source=str(watchlist_metadata["watchlist_source"]),
        canonical_quality_rows=list(watchlist_metadata.get("canonical_quality_rows", [])),
        bootstrap_row_targets=list(watchlist_metadata["bootstrap_row_targets"]),
        dynamic_watchlist_rows=list(watchlist_metadata["dynamic_watchlist_rows"]),
        watchlist_diagnostics=dict(watchlist_metadata.get("watchlist_diagnostics", {})),
    )
    benchmark["summary"]["row_fidelity_role_filter"] = str(
        getattr(args, "row_fidelity_role_filter", "all")
    )
    if baseline_summary_payload is not None:
        benchmark["baseline_regression_gate"] = _build_baseline_regression_report(
            benchmark,
            baseline_summary_payload,
            float(getattr(args, "regression_threshold", 2.0)),
            row_targets=baseline_row_targets,
        )
        _refresh_single_summary_target_rows_from_row_gate(
            benchmark,
            benchmark["baseline_regression_gate"].get("row_fidelity_gate")
            if isinstance(benchmark.get("baseline_regression_gate"), dict)
            else None,
        )
    summary_json_path, summary_md_path = write_summary_files(output_dir, benchmark)
    print_console_summary(
        benchmark["summary"],
        output_dir,
        benchmark.get("baseline_regression_gate"),
    )

    compare_json_path: Path | None = None
    compare_md_path: Path | None = None
    if previous_summary_payload is not None:
        previous_comparison = compare_with_previous_summary(benchmark, previous_summary_payload)
        compare_json_path, compare_md_path = write_previous_comparison_files(output_dir, previous_comparison)
        print_previous_comparison_summary(previous_comparison)
        print(f"Previous delta artifacts: {compare_json_path}, {compare_md_path}")

    # ── Regression gate ───────────────────────────────────────────────────────
    report_json_path: Path | None = None
    report_md_path: Path | None = None
    if baseline_summary_payload is not None:
        report = benchmark.get("baseline_regression_gate")
        if isinstance(report, dict):
            report_json_path, report_md_path = write_baseline_regression_files(output_dir, report)
            print(f"Baseline gate artifacts: {report_json_path}, {report_md_path}")
        threshold = float(getattr(args, "regression_threshold", 2.0))
        baseline_failed = check_regression(
            benchmark,
            baseline_summary_payload,
            threshold,
            row_targets=baseline_row_targets,
        )
        if baseline_failed and enforce_baseline_gate:
            raise SystemExit(1)
    else:
        baseline_failed = False

    compact_summary = build_single_compact_summary(
        benchmark,
        delta_payload=previous_comparison if previous_summary_payload is not None else None,
        regression_gate_payload=benchmark.get("baseline_regression_gate")
        if isinstance(benchmark.get("baseline_regression_gate"), dict)
        else None,
        summary_json_path=summary_json_path,
    )
    compact_summary_path = write_compact_summary(output_dir, compact_summary)
    print(f"Compact summary artifact: {compact_summary_path}")

    maybe_generate_benchmark_llm_advisory(
        output_dir=output_dir,
        summary_json_path=summary_json_path,
        delta_json_path=compare_json_path,
        regression_gate_json_path=report_json_path,
    )
    return {
        "benchmark": benchmark,
        "output_dir": output_dir,
        "summary_json_path": summary_json_path,
        "summary_md_path": summary_md_path,
        "baseline_summary": baseline_summary_payload,
        "baseline_gate_failed": baseline_failed,
        "manifest_entry": manifest_entry or {
            "id": _canonical_corpus_entry_id(binary_path.stem),
            "binary_path": str(binary_path),
            "ghidra_project_key": _canonical_corpus_entry_id(binary_path.stem),
            "tags": [],
            "seed_limit": effective_limit,
            "role": "single",
            "weight": 1,
            "row_fidelity_targets": row_fidelity_targets_filter,
        },
    }


def main() -> int:
    args = parse_args()
    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    fission_bin = resolve_fission_bin(args.fission_bin)

    baseline_dir: Path | None = getattr(args, "baseline_dir", None)
    if baseline_dir is not None:
        baseline_dir = baseline_dir.expanduser().resolve()

    if args.corpus_manifest is not None:
        manifest = load_corpus_manifest(args.corpus_manifest)
        output_dir = ensure_dir(
            args.output_dir.resolve()
            if args.output_dir
            else DEFAULT_RESULTS_DIR
            / _default_corpus_output_name(
                manifest_name=str(manifest.get("name", "")).strip(),
                manifest_path=Path(manifest["path"]),
                profile=args.profile,
                timestamped=bool(args.timestamped_output),
            )
        )
        baseline_summary_payload = (
            load_baseline_summary(baseline_dir) if baseline_dir is not None else None
        )
        binary_results: list[dict[str, Any]] = []
        for entry in manifest["entries"]:
            print(f"\n=== Corpus Entry: {entry['id']} ===")
            entry_output_dir = output_dir / entry["id"]
            entry_baseline_summary = _resolve_entry_baseline_summary(
                baseline_dir,
                entry["id"],
            )
            result = run_single_benchmark(
                args=args,
                binary_path=resolve_binary(Path(entry["binary_path"])),
                ghidra_dir=ghidra_dir,
                fission_bin=fission_bin,
                output_dir=entry_output_dir,
                baseline_summary_payload=entry_baseline_summary,
                manifest_entry=entry,
                enforce_baseline_gate=False,
            )
            binary_results.append(result)

        corpus_summary = build_corpus_assessment(
            manifest,
            binary_results,
            baseline_summary_json=baseline_summary_payload,
            baseline_artifact=str(baseline_dir) if baseline_dir is not None else None,
        )
        summary_json_path, summary_md_path = write_corpus_summary_files(output_dir, corpus_summary)
        compact_summary = build_corpus_compact_summary(
            corpus_summary,
            summary_json_path=summary_json_path,
        )
        compact_summary_path = write_compact_summary(output_dir, compact_summary)
        print(f"Compact summary artifact: {compact_summary_path}")
        maybe_generate_benchmark_llm_advisory(
            output_dir=output_dir,
            summary_json_path=summary_json_path,
            delta_json_path=None,
            regression_gate_json_path=None,
        )
        print_corpus_console_summary(corpus_summary, output_dir)
        if (
            corpus_summary.get("gate_mode") == "blocking"
            and corpus_summary.get("corpus_summary", {}).get("status") == "failed"
        ):
            return 1
        return 0

    binary_path = resolve_binary(args.binary)
    output_dir = ensure_dir(
        args.output_dir.resolve()
        if args.output_dir
        else DEFAULT_RESULTS_DIR
        / _default_binary_output_name(
            binary_path,
            profile=args.profile,
            timestamped=bool(args.timestamped_output),
        )
    )
    baseline_summary_payload = (
        load_baseline_summary(baseline_dir) if baseline_dir is not None else None
    )
    try:
        run_single_benchmark(
            args=args,
            binary_path=binary_path,
            ghidra_dir=ghidra_dir,
            fission_bin=fission_bin,
            output_dir=output_dir,
            baseline_summary_payload=baseline_summary_payload,
        )
    except SystemExit as exc:
        return int(exc.code or 1)
    return 0
