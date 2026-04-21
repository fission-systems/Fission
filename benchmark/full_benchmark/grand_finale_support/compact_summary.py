from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

from .artifact_models import (
    CompactArchSummary,
    CompactBinaryRow,
    CompactCorpusBenchmarkSummary,
    CompactRowExample,
    CompactSingleBenchmarkSummary,
    VerboseCorpusBenchmarkArtifact,
    VerboseSingleBenchmarkArtifact,
)

COMPACT_SUMMARY_FILENAME = "benchmark_compact_summary.json"
MAX_COMPACT_ROWS = 10
SELECTED_SHAPE_DRIFT_KEYS = (
    "goto_total",
    "top_level_label_total",
    "generic_local_name_sum",
    "generic_param_name_sum",
    "heuristic_max_brace_nesting_mean",
    "synthetic_helper_call_total",
)
SELECTED_NORMALIZE_PASS_KEYS = (
    "wide_dead_assignment_total_time_ms",
    "wide_dead_assignment_total_invocations",
    "wide_dead_assignment_changed_count",
    "sccp_total_time_ms",
    "sccp_total_invocations",
    "sccp_changed_count",
    "jump_resolver_total_time_ms",
    "jump_resolver_total_invocations",
    "jump_resolver_changed_count",
    "break_continue_recovery_total_time_ms",
    "break_continue_recovery_total_invocations",
    "break_continue_recovery_changed_count",
)
SELECTED_GHIDRA_ACTION_KEYS = (
    "stage_count",
    "funcdata_build",
    "heritage_value_recovery",
    "normalize",
    "prototype_types",
    "blockgraph_structuring",
    "printc",
    "pipeline_complete",
)


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


def _nanmean(values: list[Any]) -> float:
    if not values:
        return 0.0
    arr = np.array([_safe_float(value, np.nan) for value in values], dtype=float)
    if np.isnan(arr).all():
        return 0.0
    return float(np.nanmean(arr))


def _weighted_average(values: list[Any], weights: list[Any]) -> float:
    if not values or not weights or len(values) != len(weights):
        return 0.0
    val_arr = np.array([_safe_float(value, np.nan) for value in values], dtype=float)
    weight_arr = np.array([_safe_float(weight, 0.0) for weight in weights], dtype=float)
    mask = (~np.isnan(val_arr)) & (weight_arr > 0)
    if not mask.any():
        return 0.0
    return float(np.average(val_arr[mask], weights=weight_arr[mask]))


def _normalize_metric_map(metrics: dict[str, Any], *, allowed_keys: tuple[str, ...] | None = None) -> dict[str, float]:
    items = metrics.items() if isinstance(metrics, dict) else []
    normalized: dict[str, float] = {}
    for key, value in items:
        if allowed_keys is not None and key not in allowed_keys:
            continue
        normalized[str(key)] = _safe_float(value, 0.0)
    return dict(sorted(normalized.items()))


def _compact_row_example(row: dict[str, Any]) -> CompactRowExample:
    return CompactRowExample(
        address=row.get("address"),
        binary_id=row.get("binary_id"),
        function_name=row.get("fission_name") or row.get("pyghidra_name") or row.get("function_name"),
        normalized_similarity_delta=(
            _safe_float(row.get("normalized_similarity_delta"), 0.0)
            if row.get("normalized_similarity_delta") is not None
            else None
        ),
        current_normalized_similarity=(
            _safe_float(row.get("current_normalized_similarity"), 0.0)
            if row.get("current_normalized_similarity") is not None
            else None
        ),
        previous_normalized_similarity=(
            _safe_float(row.get("previous_normalized_similarity"), 0.0)
            if row.get("previous_normalized_similarity") is not None
            else None
        ),
        selected_because=row.get("selected_because"),
        reason_tags=[
            str(reason)
            for reason in (row.get("reason_tags") or row.get("failure_reasons") or [])
            if isinstance(reason, str)
        ],
    )


def build_single_compact_summary(
    summary_payload: dict[str, Any],
    *,
    delta_payload: dict[str, Any] | None = None,
    regression_gate_payload: dict[str, Any] | None = None,
    summary_json_path: Path | None = None,
) -> CompactSingleBenchmarkSummary:
    artifact = VerboseSingleBenchmarkArtifact.model_validate(summary_payload)
    summary = artifact.summary
    quality = (summary.quality or {}).get("pyghidra_vs_fission", {}) or {}
    inter = (((summary_payload.get("summary") or {}).get("kpi") or {}).get("intersection") or {}).get(
        "pyghidra_vs_fission",
        {},
    )
    owner_metrics = _normalize_metric_map((summary.owner_metrics or {}).get("fission", {}))
    shape_drift = _normalize_metric_map(
        (summary.shape_drift_metrics or {}).get("fission", {}),
        allowed_keys=SELECTED_SHAPE_DRIFT_KEYS,
    )
    normalize_pass_metrics = _normalize_metric_map(
        (summary.normalize_pass_metrics or {}).get("fission", {}),
        allowed_keys=SELECTED_NORMALIZE_PASS_KEYS,
    )
    ghidra_action_metrics = _normalize_metric_map(
        (summary.ghidra_action_metrics or {}).get("fission", {}),
        allowed_keys=SELECTED_GHIDRA_ACTION_KEYS,
    )
    watchlist = ((summary.row_fidelity_targets or {}).get("pyghidra_vs_fission") or {})
    giant_function_speed_family_counts = {
        str(key): _safe_int(value, 0)
        for key, value in sorted((summary.giant_function_speed_family_counts or {}).items())
    }
    max_pathological_examples = [
        dict(row)
        for row in list(summary.max_pathological_examples or [])[:MAX_COMPACT_ROWS]
        if isinstance(row, dict)
    ]
    baseline_blockers = []
    if isinstance(regression_gate_payload, dict):
        baseline_blockers = [str(item) for item in regression_gate_payload.get("regressions", [])]

    top_regressions = []
    if isinstance(delta_payload, dict):
        degraded = ((delta_payload.get("degraded_functions") or {}).get("top_degraded") or [])
        top_regressions = [_compact_row_example(row) for row in degraded[:MAX_COMPACT_ROWS] if isinstance(row, dict)]

    top_examples = []
    row_gate_rows = []
    if isinstance(regression_gate_payload, dict):
        row_gate_rows = (((regression_gate_payload.get("row_fidelity_gate") or {}).get("rows")) or [])
    low_rows = ((summary.samples or {}).get("pyghidra_vs_fission_lowest_similarity") or [])
    for row in list(low_rows)[: MAX_COMPACT_ROWS // 2]:
        if isinstance(row, dict):
            top_examples.append(_compact_row_example(row))
    for row in list(row_gate_rows)[: MAX_COMPACT_ROWS - len(top_examples)]:
        if isinstance(row, dict):
            top_examples.append(_compact_row_example(row))

    return CompactSingleBenchmarkSummary(
        binary_path=summary.binary,
        generated_at=summary.generated_at,
        comparable_to_baseline=isinstance(regression_gate_payload, dict),
        baseline_artifact=str(summary_json_path.resolve()) if summary_json_path else None,
        avg_normalized_similarity=_safe_float(quality.get("avg_normalized_similarity"), 0.0),
        aggregate_normalized_similarity=_safe_float(quality.get("aggregate_normalized_similarity"), 0.0),
        both_success_rate_pct=_safe_float(inter.get("both_success_rate_pct"), 0.0),
        owner_metrics=owner_metrics,
        shape_drift_metrics=shape_drift,
        normalize_pass_metrics=normalize_pass_metrics,
        ghidra_action_metrics=ghidra_action_metrics,
        giant_function_speed_family_counts=giant_function_speed_family_counts,
        watchlist_diagnostics=dict((watchlist.get("watchlist_diagnostics") or {})),
        baseline_blockers=baseline_blockers,
        top_regressions=top_regressions,
        top_row_examples=top_examples[:MAX_COMPACT_ROWS],
        max_pathological_examples=max_pathological_examples,
    )


def _build_arch_compact_summary(
    arch_payload: dict[str, Any],
    *,
    arch_name: str,
    binary_rows: pd.DataFrame,
) -> CompactArchSummary:
    binary_ids = list(arch_payload.get("failed_binary_ids", []) or [])
    arch_binary_rows = (
        binary_rows[binary_rows["arch"] == arch_name]
        if not binary_rows.empty and "arch" in binary_rows
        else binary_rows.iloc[0:0]
    )
    owner_totals = (
        arch_binary_rows["owner_metrics"].apply(pd.Series).fillna(0.0).sum().to_dict()
        if not arch_binary_rows.empty
        else {}
    )
    shape_totals = (
        arch_binary_rows["shape_drift_metrics"].apply(pd.Series).fillna(0.0).sum().to_dict()
        if not arch_binary_rows.empty
        else {}
    )
    return CompactArchSummary(
        binary_count=_safe_int(arch_payload.get("binary_count"), 0),
        release_candidate_count=_safe_int(arch_payload.get("release_candidate_count"), 0),
        weighted_avg_normalized_similarity=_safe_float(
            arch_payload.get("weighted_avg_normalized_similarity"),
            0.0,
        ),
        coverage_non_worse_count=_safe_int(arch_payload.get("coverage_non_worse_count"), 0),
        direct_success_non_worse_count=_safe_int(
            arch_payload.get("direct_success_non_worse_count"),
            0,
        ),
        failed_binary_ids=binary_ids,
        owner_metric_totals=_normalize_metric_map(owner_totals),
        shape_drift_totals=_normalize_metric_map(shape_totals, allowed_keys=SELECTED_SHAPE_DRIFT_KEYS),
    )


def build_corpus_compact_summary(
    summary_payload: dict[str, Any],
    *,
    summary_json_path: Path | None = None,
) -> CompactCorpusBenchmarkSummary:
    artifact = VerboseCorpusBenchmarkArtifact.model_validate(summary_payload)
    binary_records: list[dict[str, Any]] = []
    for row in artifact.binaries:
        watchlist_diagnostics = row.watchlist_diagnostics or {}
        selected_reasons = sorted(
            str(reason)
            for reason in (watchlist_diagnostics.get("selected_because_counts") or {}).keys()
        )
        binary_records.append(
            {
                "id": row.id or "unknown",
                "arch": row.arch or "unknown",
                "role": row.role or "unknown",
                "avg_normalized_similarity": _safe_float(row.avg_normalized_similarity, 0.0),
                "coverage_ratio_pct": _safe_float(row.coverage_ratio_pct, 0.0),
                "direct_success": row.direct_success or "unknown",
                "row_fidelity_gate_status": row.row_fidelity_gate_status or "unknown",
                "watchlist_source": row.watchlist_source or "unknown",
                "selected_watchlist_reasons": selected_reasons,
                "owner_metrics": _normalize_metric_map(row.owner_metrics),
                "shape_drift_metrics": _normalize_metric_map(
                    row.shape_drift_metrics,
                    allowed_keys=SELECTED_SHAPE_DRIFT_KEYS,
                ),
                "normalize_pass_metrics": _normalize_metric_map(
                    row.normalize_pass_metrics,
                    allowed_keys=SELECTED_NORMALIZE_PASS_KEYS,
                ),
                "ghidra_action_metrics": _normalize_metric_map(
                    row.ghidra_action_metrics,
                    allowed_keys=SELECTED_GHIDRA_ACTION_KEYS,
                ),
                "eligibility_reason": str((row.eligibility or {}).get("reason") or "unknown"),
                "weight": _safe_float(row.weight, 0.0),
            }
        )
    binary_df = pd.DataFrame(binary_records)

    per_binary_rows: list[CompactBinaryRow] = []
    if not binary_df.empty:
        sort_df = binary_df.sort_values(
            by=["avg_normalized_similarity", "id"],
            ascending=[True, True],
        )
        for record in sort_df.head(MAX_COMPACT_ROWS).to_dict(orient="records"):
            per_binary_rows.append(
                CompactBinaryRow(
                    id=str(record["id"]),
                    arch=str(record["arch"]),
                    role=str(record["role"]),
                    avg_normalized_similarity=_safe_float(record["avg_normalized_similarity"], 0.0),
                    coverage_ratio_pct=_safe_float(record["coverage_ratio_pct"], 0.0),
                    direct_success=str(record["direct_success"]),
                    row_fidelity_gate_status=str(record["row_fidelity_gate_status"]),
                    watchlist_source=str(record["watchlist_source"]),
                    selected_watchlist_reasons=list(record["selected_watchlist_reasons"]),
                    owner_metrics=_normalize_metric_map(record["owner_metrics"]),
                    shape_drift_metrics=_normalize_metric_map(
                        record["shape_drift_metrics"],
                        allowed_keys=SELECTED_SHAPE_DRIFT_KEYS,
                    ),
                    normalize_pass_metrics=_normalize_metric_map(
                        record["normalize_pass_metrics"],
                        allowed_keys=SELECTED_NORMALIZE_PASS_KEYS,
                    ),
                    ghidra_action_metrics=_normalize_metric_map(
                        record["ghidra_action_metrics"],
                        allowed_keys=SELECTED_GHIDRA_ACTION_KEYS,
                    ),
                    eligibility_reason=str(record["eligibility_reason"]),
                )
            )

    degraded_rows = [
        _compact_row_example(row)
        for row in list(artifact.cross_binary_degraded_watchlist or [])[:MAX_COMPACT_ROWS]
        if isinstance(row, dict)
    ]

    arch_summary = artifact.arch_summary or {}
    x86_summary = None
    x64_summary = None
    if isinstance(arch_summary, dict):
        x86_payload = arch_summary.get("x86")
        if isinstance(x86_payload, dict):
            x86_summary = _build_arch_compact_summary(
                x86_payload,
                arch_name="x86",
                binary_rows=binary_df,
            )
        x64_payload = arch_summary.get("x64")
        if isinstance(x64_payload, dict):
            x64_summary = _build_arch_compact_summary(
                x64_payload,
                arch_name="x64",
                binary_rows=binary_df,
            )

    owner_totals = _normalize_metric_map(artifact.owner_metric_totals)
    shape_totals = _normalize_metric_map(
        artifact.shape_drift_totals,
        allowed_keys=SELECTED_SHAPE_DRIFT_KEYS,
    )
    normalize_pass_totals = _normalize_metric_map(
        artifact.normalize_pass_metric_totals,
        allowed_keys=SELECTED_NORMALIZE_PASS_KEYS,
    )
    ghidra_action_totals = _normalize_metric_map(
        artifact.ghidra_action_metric_totals,
        allowed_keys=SELECTED_GHIDRA_ACTION_KEYS,
    )
    giant_function_speed_family_totals = {
        str(key): _safe_int(value, 0)
        for key, value in sorted((artifact.giant_function_speed_family_totals or {}).items())
    }
    max_pathological_examples = [
        dict(row)
        for row in list(artifact.max_pathological_examples or [])[:MAX_COMPACT_ROWS]
        if isinstance(row, dict)
    ]
    if binary_df.empty:
        weighted_avg = _safe_float(artifact.corpus_summary.weighted_avg_normalized_similarity, 0.0)
    else:
        weighted_avg = _weighted_average(
            binary_df["avg_normalized_similarity"].tolist(),
            binary_df["weight"].tolist(),
        )

    return CompactCorpusBenchmarkSummary(
        manifest_name=str((artifact.manifest or {}).get("name") or "corpus"),
        generated_at=artifact.generated_at,
        suite_tier=artifact.suite_tier,
        gate_mode=artifact.gate_mode,
        comparable_to_baseline=artifact.comparable_to_baseline,
        baseline_artifact=artifact.baseline_artifact,
        release_promotion_allowed=artifact.release_promotion_allowed,
        promotion_blockers=list(artifact.promotion_blockers),
        weighted_avg_normalized_similarity=weighted_avg,
        x86_summary=x86_summary,
        x64_summary=x64_summary,
        owner_metric_totals=owner_totals,
        shape_drift_totals=shape_totals,
        normalize_pass_metric_totals=normalize_pass_totals,
        ghidra_action_metric_totals=ghidra_action_totals,
        giant_function_speed_family_totals=giant_function_speed_family_totals,
        watchlist_reason_counts={
            str(key): _safe_int(value, 0)
            for key, value in sorted((artifact.watchlist_reason_counts or {}).items())
        },
        top_degraded_rows=degraded_rows,
        per_binary_rows=per_binary_rows,
        max_pathological_examples=max_pathological_examples,
    )


def write_compact_summary(output_dir: Path, compact_summary: Any) -> Path:
    output_path = output_dir / COMPACT_SUMMARY_FILENAME
    payload = (
        compact_summary.model_dump(mode="json", exclude_none=True)
        if hasattr(compact_summary, "model_dump")
        else compact_summary
    )
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return output_path
