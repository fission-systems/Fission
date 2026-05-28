from __future__ import annotations
import datetime
import json
import os
import re
import shlex
import shutil
import sys
import subprocess
from collections import Counter
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.models import BenchmarkEntry
from benchmark.source_semantic_benchmark.cli import run_command_capture
from benchmark.source_semantic_benchmark.config import (
    ROOT_DIR,
    DEFAULT_ARTIFACT_ROOT,
    NIR_DEBT_METRIC_RE,
)
from benchmark.source_semantic_benchmark.utils import (
    utc_now,
    utc_isoformat,
    utc_timestamp_slug,
    dump_json_pretty,
    dump_json_line,
    load_json,
    rel,
    percent,
    numeric_distribution,
    sanitize_id,
    resolve_path,
    normalize_name,
)
from benchmark.source_semantic_benchmark.scoring import (
    STAGE_FAILURE_ORDER,
    STATIC_SIMILARITY_COMPONENTS,
    compare_to_baseline,
    comparison_outcome,
    improvement_summary,
    metric_delta,
    zero_credit_reason,
    row_zero_credit_reason,
    row_triage_priority,
    triage_row_summary,
    sleigh_template_source_gate,
    canonical_sleigh_template_source,
    stage_first_failure,
    furthest_ok_stage,
    complexity_bucket,
    feature_gap_bucket,
    cost_bucket,
    behavior_failure_owner,
    behavior_detail_signature,
    signedness_only_signature_gap,
)
from benchmark.source_semantic_benchmark.behavior import (
    behavior_artifact_dir_for_row,
)

def write_single_debug_bundle(path: Path, bundle: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        dump_json_pretty({"schema_version": 1, "functions": [bundle]}),
        encoding="utf-8",
    )




def debug_bundle_path_for_parts(
    output_dir: Path,
    entry_id: str | None,
    function_name: str | None,
    address: str | None,
) -> Path:
    entry = sanitize_id(str(entry_id or "entry"))
    function = sanitize_id(str(function_name or "function"))
    address = sanitize_id(str(address or "no-address"))
    return output_dir / "debug_decomp" / entry / f"{function}-{address}.json"




def debug_bundle_path_for_row(output_dir: Path, row: dict[str, Any]) -> Path:
    return debug_bundle_path_for_parts(
        output_dir,
        row.get("entry_id"),
        row.get("function_name"),
        row.get("address"),
    )




def debug_triage_path_for_row(output_dir: Path, row: dict[str, Any], kind: str, suffix: str) -> Path:
    stem = "-".join(
        [
            sanitize_id(str(row.get("entry_id") or "entry")),
            sanitize_id(str(row.get("function_name") or "function")),
            sanitize_id(str(row.get("address") or "unknown")),
        ]
    )
    return output_dir / "debug_triage" / kind / f"{stem}.{suffix}"




def decomp_debug_command_for_row(row: dict[str, Any], fission_bin: Path, output_dir: Path) -> dict[str, Any] | None:
    address = row.get("address")
    binary_path = row.get("binary_path")
    if not address or not binary_path:
        return None
    bundle_path = debug_bundle_path_for_row(output_dir, row)
    cmd = [
        fission_bin,
        "decomp",
        resolve_path(str(binary_path)),
        "--addr",
        str(address),
        "--json",
        "--no-header",
        "--no-warnings",
        "--debug-decomp",
        "--debug-decomp-bundle",
        bundle_path,
    ]
    return {
        "debug_decomp_bundle_path": rel(bundle_path),
        "debug_decomp_command": shell_command(cmd),
        "disasm_function_command": shell_command(
            [
                fission_bin,
                "disasm",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--function",
                "--json",
            ]
        ),
        "xrefs_function_command": shell_command(
            [
                fission_bin,
                "xrefs",
                resolve_path(str(binary_path)),
                "--function",
                str(address),
                "--json",
            ]
        ),
        "preview_candidate_command": None,
        "preview_candidate_note": "inventory preview-candidates is deprecated with native_decomp removal; use debug-decomp and function-facts",
        "function_facts_command": shell_command(
            [
                fission_bin,
                "inventory",
                "function-facts",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--output-jsonl",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.jsonl",
                "--summary-json",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.json",
            ]
        ),
    }




def attach_debug_repro_commands(rows: list[dict[str, Any]], fission_bin: Path, output_dir: Path) -> None:
    for row in rows:
        command = decomp_debug_command_for_row(row, fission_bin, output_dir)
        if command is not None:
            row.update(command)




def top_debug_commands(rows: list[dict[str, Any]], limit: int = 12) -> list[dict[str, Any]]:
    candidates = [
        row
        for row in rows
        if row.get("debug_decomp_command") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    candidates.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return [
        {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": row.get("address"),
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "behavior_artifact_dir": row.get("behavior", {}).get("artifact_dir"),
            "debug_decomp_bundle_path": row.get("debug_decomp_bundle_path"),
            "debug_decomp_command": row.get("debug_decomp_command"),
            "disasm_function_command": row.get("disasm_function_command"),
            "xrefs_function_command": row.get("xrefs_function_command"),
            "preview_candidate_command": row.get("preview_candidate_command"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "function_facts_command": row.get("function_facts_command"),
        }
        for row in candidates[:limit]
    ]




def materialize_debug_triage_for_rows(
    selected: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
) -> list[dict[str, Any]]:
    triage_rows: list[dict[str, Any]] = []
    for row in selected:
        binary_path = resolve_path(str(row["binary_path"]))
        address = str(row["address"])
        decomp_bundle_path = debug_bundle_path_for_row(output_dir, row)
        decomp_capture_path = debug_triage_path_for_row(output_dir, row, "debug_decomp", "command.json")
        disasm_capture_path = debug_triage_path_for_row(output_dir, row, "disasm", "command.json")
        xrefs_capture_path = debug_triage_path_for_row(output_dir, row, "xrefs", "command.json")
        facts_jsonl_path = debug_triage_path_for_row(output_dir, row, "function_facts", "jsonl")
        facts_summary_path = debug_triage_path_for_row(output_dir, row, "function_facts", "summary.json")
        facts_capture_path = debug_triage_path_for_row(output_dir, row, "function_facts", "command.json")
        decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        decomp_capture_path.parent.mkdir(parents=True, exist_ok=True)
        disasm_capture_path.parent.mkdir(parents=True, exist_ok=True)
        xrefs_capture_path.parent.mkdir(parents=True, exist_ok=True)
        facts_jsonl_path.parent.mkdir(parents=True, exist_ok=True)

        decomp_capture = run_command_capture(
            [
                fission_bin,
                "decomp",
                binary_path,
                "--addr",
                address,
                "--json",
                "--no-header",
                "--no-warnings",
                "--debug-decomp",
                "--debug-decomp-bundle",
                decomp_bundle_path,
            ],
            timeout_sec,
        )
        decomp_capture_path.write_text(dump_json_pretty(decomp_capture), encoding="utf-8")

        disasm_capture = run_command_capture(
            [
                fission_bin,
                "disasm",
                binary_path,
                "--addr",
                address,
                "--function",
                "--json",
            ],
            timeout_sec,
        )
        disasm_capture_path.write_text(dump_json_pretty(disasm_capture), encoding="utf-8")

        xrefs_capture = run_command_capture(
            [
                fission_bin,
                "xrefs",
                binary_path,
                "--function",
                address,
                "--json",
            ],
            timeout_sec,
        )
        xrefs_capture_path.write_text(dump_json_pretty(xrefs_capture), encoding="utf-8")

        facts = run_command_capture(
            [
                fission_bin,
                "inventory",
                "function-facts",
                binary_path,
                "--addr",
                address,
                "--output-jsonl",
                facts_jsonl_path,
                "--summary-json",
                facts_summary_path,
            ],
            timeout_sec,
        )
        facts_capture_path.write_text(dump_json_pretty(facts), encoding="utf-8")

        triage = {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": address,
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "baseline_regression": row.get("baseline_regression"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "debug_decomp_capture_path": rel(decomp_capture_path),
            "debug_decomp_bundle_path": rel(decomp_bundle_path),
            "debug_decomp_returncode": decomp_capture.get("returncode"),
            "disasm_capture_path": rel(disasm_capture_path),
            "disasm_returncode": disasm_capture.get("returncode"),
            "xrefs_capture_path": rel(xrefs_capture_path),
            "xrefs_returncode": xrefs_capture.get("returncode"),
            "function_facts_capture_path": rel(facts_capture_path),
            "function_facts_jsonl_path": rel(facts_jsonl_path),
            "function_facts_summary_path": rel(facts_summary_path),
            "function_facts_returncode": facts.get("returncode"),
        }
        row["debug_decomp_bundle_path"] = rel(decomp_bundle_path)
        row["debug_triage"] = triage
        triage_rows.append(triage)
    return triage_rows




def materialize_debug_triage(
    rows: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    selected = [
        row
        for row in rows
        if row.get("address") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    selected.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return materialize_debug_triage_for_rows(selected[: max(0, limit)], fission_bin, output_dir, timeout_sec)




def materialize_regression_debug_triage(
    rows: list[dict[str, Any]],
    comparison: dict[str, Any],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    rows_by_key = {row_key(row): row for row in rows if row.get("address")}
    selected: list[dict[str, Any]] = []
    seen: set[str] = set()
    for delta in comparison.get("top_regressions") or []:
        key = str(delta.get("key") or "")
        if not key or key in seen:
            continue
        row = rows_by_key.get(key)
        if row is None:
            continue
        row["baseline_regression"] = {
            "baseline_score_percent": delta.get("baseline_score_percent"),
            "current_score_percent": delta.get("current_score_percent"),
            "delta_percent": delta.get("delta_percent"),
            "baseline_behavior": delta.get("baseline_behavior"),
            "current_behavior": delta.get("current_behavior"),
        }
        selected.append(row)
        seen.add(key)
        if len(selected) >= max(0, limit):
            break
    return materialize_debug_triage_for_rows(selected, fission_bin, output_dir, timeout_sec)




def numeric_items(payload: Any) -> list[tuple[str, float]]:
    if not isinstance(payload, dict):
        return []
    return [
        (str(key), float(value))
        for key, value in payload.items()
        if isinstance(value, int | float) and not isinstance(value, bool)
    ]




def is_debt_metric_name(name: str) -> bool:
    return bool(NIR_DEBT_METRIC_RE.search(name))




def add_numeric_debug_pipeline_values(values: dict[str, list[float]], pipeline: Any) -> None:
    if not isinstance(pipeline, dict):
        return
    for key in [
        "decode_attempt_count",
        "raw_pcode_block_count",
        "raw_pcode_op_count",
        "raw_pcode_edge_count",
        "instruction_limit",
        "max_bytes",
    ]:
        value = pipeline.get(key)
        if isinstance(value, int | float) and not isinstance(value, bool):
            values.setdefault(key, []).append(float(value))


ROADMAP_PRIORITY_ORDER = [
    "p1_sleigh_lift_correctness",
    "p2_type_data_abstraction",
    "p3_structuring_hard_cases",
    "p4_fid_name_recovery",
    "p5_architecture_breadth",
]




def add_priority_bucket_row(
    buckets: dict[str, dict[str, Any]],
    priority: str,
    row: dict[str, Any],
    behavior_status: str,
    first_stage: str,
    score: float,
) -> None:
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    bucket = buckets.setdefault(
        priority,
        {
            "row_count": 0,
            "score_sum": 0.0,
            "lost_score_sum": 0.0,
            "missing_feature_total": 0.0,
            "extra_feature_total": 0.0,
            "behavior_status_counts": Counter(),
            "stage_first_failure_counts": Counter(),
            "top_rows": [],
        },
    )
    bucket["row_count"] += 1
    bucket["score_sum"] += score
    bucket["lost_score_sum"] += max(0.0, 1.0 - score)
    bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
    bucket["extra_feature_total"] += float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
    bucket["behavior_status_counts"][behavior_status] += 1
    bucket["stage_first_failure_counts"][first_stage] += 1
    bucket["top_rows"].append(triage_row_summary(row))




def metric_bucket_export(metrics: dict[str, Any], total: int, top_limit: int = 12) -> dict[str, Any]:
    row_count = int(metrics.get("row_count", 0) or 0)
    top_rows = sorted(
        metrics.get("top_rows") or [],
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:top_limit]
    return {
        "row_count": row_count,
        "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
        "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
        if row_count
        else 0.0,
        "avg_semantic_score_percent": percent(
            round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
        ) if row_count else 0.0,
        "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
        "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
        "extra_feature_total": round(float(metrics.get("extra_feature_total", 0.0) or 0.0), 6),
        "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
        "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
        "top_rows": top_rows,
    }




def summarize(rows: list[dict[str, Any]], manifest_name: str, entries: list[BenchmarkEntry]) -> dict[str, Any]:
    total = len(rows)
    mapped = sum(1 for row in rows if row["mapping_status"] == "matched")
    decomp_ok = sum(1 for row in rows if row.get("decomp_success"))
    compile_ok = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    behavior_pass = sum(1 for row in rows if row.get("behavior", {}).get("status") == "pass")
    behavior_expected = sum(1 for row in rows if row.get("behavior", {}).get("eligible") is True)
    behavior_executed = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    score_values = [float(row.get("semantic_score", 0.0) or 0.0) for row in rows]
    readability_score_values = [float(row.get("readability_score", 0.0) or 0.0) for row in rows]
    semantic_correctness_score_values = [float(row.get("semantic_correctness_score", 0.0) or 0.0) for row in rows]

    # Compiler and Optimization Option Matrix
    matrix_groups = {}
    for row in rows:
        tags = row.get("tags") or []
        compiler = "Other"
        optimization = "Unknown"
        for tag in tags:
            tag_lower = tag.lower()
            if "clang" in tag_lower:
                compiler = "Clang"
            elif "gcc" in tag_lower or "mingw" in tag_lower:
                compiler = "GCC"
            
            if re.match(r"^O[0123sg]$", tag):
                optimization = tag
        
        group_key = (compiler, optimization)
        if group_key not in matrix_groups:
            matrix_groups[group_key] = []
        matrix_groups[group_key].append(row)
        
    compiler_opt_matrix_data = {}
    for (compiler, opt), group_rows in sorted(matrix_groups.items()):
        g_total = len(group_rows)
        g_mapped = sum(1 for r in group_rows if r.get("mapping_status") == "matched")
        g_decomp_ok = sum(1 for r in group_rows if r.get("decomp_success"))
        g_compile_ok = sum(1 for r in group_rows if r.get("behavior", {}).get("status") in {"pass", "mismatch"})
        g_behavior_pass = sum(1 for r in group_rows if r.get("behavior", {}).get("status") == "pass")
        g_avg_semantic = sum(float(r.get("semantic_score", 0.0) or 0.0) for r in group_rows) / g_total
        g_avg_readability = sum(float(r.get("readability_score", 0.0) or 0.0) for r in group_rows) / g_total
        g_avg_correctness = sum(float(r.get("semantic_correctness_score", 0.0) or 0.0) for r in group_rows) / g_total
        
        if compiler not in compiler_opt_matrix_data:
            compiler_opt_matrix_data[compiler] = {}
        compiler_opt_matrix_data[compiler][opt] = {
            "row_count": g_total,
            "mapped_rate": round(g_mapped / g_total, 6),
            "decomp_success_rate": round(g_decomp_ok / g_total, 6),
            "candidate_compile_rate": round(g_compile_ok / g_total, 6),
            "behavior_pass_rate": round(g_behavior_pass / g_total, 6),
            "avg_semantic_score": round(g_avg_semantic, 6),
            "avg_readability_score": round(g_avg_readability, 6),
            "avg_semantic_correctness_score": round(g_avg_correctness, 6),
        }

    # AI Remedy Hints Aggregation
    ai_remedy_summary = {
        "total_sanitizer_errors": 0,
        "total_tle_errors": 0,
        "total_mle_errors": 0,
        "excessive_nesting_count": 0,
        "cfg_mismatch_counters": Counter(),
        "suggested_action_counts": Counter(),
        "avg_cfg_similarity": 0.0,
        "readability_vs_correctness_discrepancy_counts": Counter(),
    }
    cfg_sims = []
    for row in rows:
        hints = row.get("ai_remedy_hints")
        if not isinstance(hints, dict):
            continue
        if hints.get("has_sanitizer_error"):
            ai_remedy_summary["total_sanitizer_errors"] += 1
        status = hints.get("status")
        if status == "run_tle":
            ai_remedy_summary["total_tle_errors"] += 1
        elif status == "run_mle":
            ai_remedy_summary["total_mle_errors"] += 1
        if hints.get("excessive_nesting"):
            ai_remedy_summary["excessive_nesting_count"] += 1
        
        cfg_sim = hints.get("cfg_similarity")
        if cfg_sim is not None:
            cfg_sims.append(float(cfg_sim))
            
        for mismatch in hints.get("cfg_mismatches", []):
            ai_remedy_summary["cfg_mismatch_counters"][mismatch] += 1
            
        for action in hints.get("suggested_actions", []):
            ai_remedy_summary["suggested_action_counts"][action] += 1
            
        assessment = hints.get("readability_vs_correctness_assessment")
        if isinstance(assessment, dict):
            discrepancy_type = assessment.get("discrepancy_type")
            if discrepancy_type:
                ai_remedy_summary["readability_vs_correctness_discrepancy_counts"][discrepancy_type] += 1
            
    if cfg_sims:
        ai_remedy_summary["avg_cfg_similarity"] = round(sum(cfg_sims) / len(cfg_sims), 6)
    
    ai_remedy_summary["cfg_mismatch_counters"] = dict(ai_remedy_summary["cfg_mismatch_counters"].most_common(20))
    ai_remedy_summary["suggested_action_counts"] = dict(ai_remedy_summary["suggested_action_counts"].most_common(20))
    ai_remedy_summary["readability_vs_correctness_discrepancy_counts"] = dict(
        ai_remedy_summary["readability_vs_correctness_discrepancy_counts"]
    )

    mapping_status_counts = Counter(row.get("mapping_status", "unknown") for row in rows)
    decomp_failure_counts = Counter(
        row.get("decomp_failure_kind", "unknown")
        for row in rows
        if not row.get("decomp_success")
    )
    behavior_status_counts = Counter(row.get("behavior", {}).get("status", "unknown") for row in rows)
    behavior_cache_status_counts = Counter(
        status
        for row in rows
        for status in [
            row.get("behavior", {}).get("oracle_cache_status"),
            row.get("behavior", {}).get("candidate_cache_status"),
        ]
        if status
    )
    behavior_case_source_counts = Counter(
        row.get("behavior", {}).get("case_source", "unknown")
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_unsupported_reason_counts = Counter(
        row.get("behavior", {}).get("reason", "unknown")
        for row in rows
        if row.get("behavior", {}).get("status") == "unsupported_signature"
    )
    decomp_cache_status_counts = Counter(row.get("decomp_cache_status", "not_requested") for row in rows)
    zero_credit_breakdown = Counter(
        row_zero_credit_reason(row)
        for row in rows
        if float(row.get("semantic_score", 0.0) or 0.0) == 0.0
    )
    stage_first_failure_counts = Counter(
        row.get("stage_first_failure") or "none"
        for row in rows
        if row.get("mapping_status") == "matched"
    )
    static_component_sums: Counter[str] = Counter()
    static_gap_totals: Counter[str] = Counter()
    static_gap_component_totals: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_gap_component_missing_features: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_gap_component_extra_features: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_missing_feature_counts: Counter[str] = Counter()
    static_extra_feature_counts: Counter[str] = Counter()
    score_distribution = Counter()
    debug_owner_bucket_counts: Counter[str] = Counter()
    debug_stage_status_counts: Counter[str] = Counter()
    debug_stage_status_matrix: dict[str, Counter[str]] = {
        stage: Counter() for stage in STAGE_FAILURE_ORDER
    }
    debug_quality_evidence_totals: Counter[str] = Counter()
    debug_quality_evidence_nonzero_rows: Counter[str] = Counter()
    debug_template_source_totals: Counter[str] = Counter()
    behavior_first_mismatch_index_counts: Counter[str] = Counter()
    behavior_output_length_delta_counts: Counter[str] = Counter()
    behavior_mismatch_kind_counts: Counter[str] = Counter()
    behavior_case_pass_rates: list[float] = []
    behavior_missing_candidate_line_total = 0
    behavior_extra_candidate_line_total = 0
    behavior_partial_progress_rows: list[dict[str, Any]] = []
    behavior_status_by_stage_first_failure: dict[str, Counter[str]] = {}
    behavior_status_by_zero_credit_reason: dict[str, Counter[str]] = {}
    score_values_by_behavior_status: dict[str, list[float]] = {}
    score_values_by_stage_first_failure: dict[str, list[float]] = {}
    behavior_score_values: list[float] = []
    static_score_values: list[float] = []
    component_loss_hot_rows: list[dict[str, Any]] = []
    source_body_line_counts: list[float] = []
    decomp_line_counts: list[float] = []
    source_body_byte_counts: list[float] = []
    decomp_byte_counts: list[float] = []
    decomp_to_source_line_ratios: list[float] = []
    decomp_to_source_byte_ratios: list[float] = []
    source_feature_total_values: list[float] = []
    source_feature_total_direct_values: list[float] = []
    source_feature_total_inline_expanded_values: list[float] = []
    decomp_feature_total_values: list[float] = []
    static_intersection_feature_total_values: list[float] = []
    static_union_feature_total_values: list[float] = []
    static_component_source_feature_values: dict[str, list[float]] = {
        component: [] for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_decomp_feature_values: dict[str, list[float]] = {
        component: [] for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_absence_counts: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_absence_rows: dict[str, dict[str, list[dict[str, Any]]]] = {
        component: {
            "source_only_rows": [],
            "decomp_only_rows": [],
            "zero_intersection_rows": [],
        }
        for component in STATIC_SIMILARITY_COMPONENTS
    }
    source_decomp_size_hot_rows: list[dict[str, Any]] = []
    source_feature_rows = 0
    decomp_feature_rows = 0
    static_missing_feature_rows = 0
    static_extra_feature_rows = 0
    static_zero_similarity_rows = 0
    static_decomp_absent_feature_rows = 0
    static_component_missing_row_counts: Counter[str] = Counter()
    static_component_zero_similarity_row_counts: Counter[str] = Counter()
    missing_feature_count_values: list[float] = []
    extra_feature_count_values: list[float] = []
    semantic_loss_by_behavior_status: Counter[str] = Counter()
    semantic_loss_by_stage_first_failure: Counter[str] = Counter()
    semantic_loss_by_zero_credit_reason: Counter[str] = Counter()
    semantic_loss_hot_rows: list[dict[str, Any]] = []
    cost_hot_rows: list[dict[str, Any]] = []
    debug_decomp_row_count = 0
    debug_stage_status_row_count = 0
    stage_status_metrics: dict[str, Counter[str]] = {
        stage: Counter() for stage in STAGE_FAILURE_ORDER
    }
    nir_build_stats_row_count = 0
    nir_build_stats_numeric_totals: Counter[str] = Counter()
    nir_build_stats_nonzero_rows: Counter[str] = Counter()
    nir_build_stats_values: dict[str, list[float]] = {}
    nir_build_stats_debt_hot_rows: list[dict[str, Any]] = []
    nir_debt_row_count = 0
    nir_debt_score_values: list[float] = []
    nir_no_debt_score_values: list[float] = []
    nir_debt_behavior_status_counts: Counter[str] = Counter()
    nir_debt_stage_first_failure_counts: Counter[str] = Counter()
    debug_pipeline_numeric_values: dict[str, list[float]] = {}
    improvement_axis_metrics: dict[str, dict[str, Any]] = {}
    complexity_buckets: dict[str, dict[str, Any]] = {}
    cost_values_by_behavior_status: dict[str, list[float]] = {}
    cost_values_by_stage_first_failure: dict[str, list[float]] = {}
    cost_values_by_score_bucket: dict[str, list[float]] = {}
    scores_by_cost_bucket: dict[str, list[float]] = {}
    lost_score_by_cost_bucket: Counter[str] = Counter()
    stage_funnel_counts: Counter[str] = Counter()
    stage_furthest_ok_counts: Counter[str] = Counter()
    stage_first_blocker_lost_score: Counter[str] = Counter()
    admission_gate_counts: Counter[str] = Counter()
    behavior_failure_owner_counts: Counter[str] = Counter()
    behavior_failure_detail_counts: Counter[str] = Counter()
    behavior_failure_detail_rows: dict[str, list[dict[str, Any]]] = {}
    static_gap_density_rows: dict[str, dict[str, Any]] = {}
    static_missing_density_values: list[float] = []
    static_extra_density_values: list[float] = []
    static_score_by_missing_gap_bucket: dict[str, list[float]] = {}
    static_gap_hot_rows: list[dict[str, Any]] = []
    hard_function_rows: list[dict[str, Any]] = []
    static_source_variant_counts: Counter[str] = Counter()
    inline_expanded_static_score_deltas: list[float] = []
    inline_expanded_static_hot_rows: list[dict[str, Any]] = []
    semantic_quality_quadrants: dict[str, dict[str, Any]] = {}
    semantic_readiness_counts: Counter[str] = Counter()
    sleigh_blocker_rows: list[dict[str, Any]] = []
    coverage_blind_spot_rows: dict[str, list[dict[str, Any]]] = {}
    coverage_blind_spot_counts: Counter[str] = Counter()
    focus_area_metrics: dict[str, dict[str, Any]] = {}
    roadmap_priority_metrics: dict[str, dict[str, Any]] = {}
    signature_gap_rows: list[dict[str, Any]] = []
    memory_gap_rows: list[dict[str, Any]] = []
    call_gap_rows: list[dict[str, Any]] = []
    control_flow_gap_rows: list[dict[str, Any]] = []
    signedness_only_signature_gap_rows: list[dict[str, Any]] = []
    signedness_only_signature_gap_totals: Counter[str] = Counter()
    signature_return_pair_counts: Counter[str] = Counter()
    signature_param_pair_counts: Counter[str] = Counter()
    signature_pair_gap_rows: list[dict[str, Any]] = []
    signature_param_arity_mismatch_rows: list[dict[str, Any]] = []
    name_recovery_rows: list[dict[str, Any]] = []
    architecture_stage_metrics: dict[str, dict[str, Any]] = {}
    outcome_matrix_counts: Counter[str] = Counter()
    outcome_matrix_lost_score: Counter[str] = Counter()
    outcome_matrix_rows: dict[str, list[dict[str, Any]]] = {}
    by_language: dict[str, dict[str, Any]] = {}
    by_arch: dict[str, dict[str, Any]] = {}
    by_source_return_kind: dict[str, dict[str, Any]] = {}
    by_source_param_shape: dict[str, dict[str, Any]] = {}
    by_tag: dict[str, dict[str, Any]] = {}
    by_entry: dict[str, dict[str, Any]] = {}

    def add_bucket(bucket: dict[str, Any], row: dict[str, Any]) -> None:
        bucket["row_count"] += 1
        bucket["mapped"] += int(row["mapping_status"] == "matched")
        bucket["decomp_success"] += int(bool(row.get("decomp_success")))
        bucket["behavior_pass"] += int(row.get("behavior", {}).get("status") == "pass")
        bucket["score_sum"] += float(row.get("semantic_score", 0.0) or 0.0)

    def improvement_axis_for(row: dict[str, Any], behavior: dict[str, Any], first_stage: str) -> str:
        if row.get("mapping_status") != "matched":
            return "mapping"
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            return "sleigh_decode_lift"
        if first_stage.startswith("nir_build:") or first_stage.startswith("normalize:"):
            return "nir_build_normalize"
        if first_stage.startswith("structuring:") or first_stage.startswith("render:"):
            return "structuring_render"
        if not row.get("decomp_success"):
            return "decompile_orchestration"
        behavior_status = str(behavior.get("status", "unknown"))
        if behavior_status == "unsupported_signature":
            return "behavior_coverage"
        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
        }:
            return "behavior_harness"
        if behavior_status == "mismatch":
            return "dynamic_semantics"
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        if float(static_gaps.get("missing_feature_total", 0.0) or 0.0) > 0.0:
            return "static_semantic_gaps"
        preview_stats = row.get("preview_build_stats")
        if isinstance(preview_stats, dict) and any(
            is_debt_metric_name(key) and value != 0
            for key, value in numeric_items(preview_stats)
        ):
            return "nir_telemetry_debt"
        if float(row.get("semantic_score", 0.0) or 0.0) < 1.0:
            return "partial_quality"
        return "passing"

    def focus_areas_for(
        row: dict[str, Any],
        behavior: dict[str, Any],
        first_stage: str,
        preview_stats: dict[str, Any] | None,
        debug_decomp: dict[str, Any] | None,
    ) -> set[str]:
        areas: set[str] = set()
        behavior_status = str(behavior.get("status", "unknown"))
        static_components = (
            row.get("static_similarity_gap_components")
            if isinstance(row.get("static_similarity_gap_components"), dict)
            else {}
        )
        quality = (
            debug_decomp.get("quality_evidence")
            if isinstance(debug_decomp, dict) and isinstance(debug_decomp.get("quality_evidence"), dict)
            else {}
        )
        owner_buckets = {
            str(bucket)
            for bucket in (debug_decomp.get("owner_buckets") if isinstance(debug_decomp, dict) else []) or []
        }

        if row.get("mapping_status") != "matched":
            areas.add("mapping_name_recovery")
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            areas.add("sleigh_runtime_lift")
        if any("sleigh" in bucket or "raw_pcode" in bucket or "decode" in bucket for bucket in owner_buckets):
            areas.add("sleigh_runtime_lift")
        if first_stage.startswith("nir_build:") or first_stage.startswith("normalize:"):
            areas.add("nir_builder_dataflow")
        if isinstance(preview_stats, dict) and any(
            is_debt_metric_name(key) and value != 0 for key, value in numeric_items(preview_stats)
        ):
            areas.add("nir_builder_dataflow")
        if first_stage.startswith("structuring:") or first_stage.startswith("render:"):
            areas.add("structuring_render")
        if any("structuring" in bucket or "region" in bucket for bucket in owner_buckets):
            areas.add("structuring_render")
        if any(
            isinstance(value, int | float) and value != 0
            for key, value in quality.items()
            if key.startswith("structuring_") or key.startswith("region_")
        ):
            areas.add("structuring_render")

        type_component_missing = 0.0
        for component in ["memory", "signature", "call"]:
            details = static_components.get(component)
            if isinstance(details, dict):
                type_component_missing += float(details.get("missing_feature_total", 0.0) or 0.0)
        if type_component_missing > 0.0:
            areas.add("type_data_abstraction")
        if any(
            isinstance(value, int | float) and value != 0
            for key, value in quality.items()
            if key.startswith("typed_") or key.startswith("call_") or "prototype" in key
        ):
            areas.add("type_data_abstraction")

        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
            "unsupported_signature",
        }:
            areas.add("behavior_harness_coverage")
        if behavior_status == "mismatch":
            areas.add("dynamic_semantics")
        if not areas and float(row.get("semantic_score", 0.0) or 0.0) < 1.0:
            areas.add("unclassified_quality_loss")
        if not areas:
            areas.add("passing")
        return areas

    def add_focus_area_row(
        area: str,
        row: dict[str, Any],
        behavior_status: str,
        first_stage: str,
        score: float,
    ) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        bucket = focus_area_metrics.setdefault(
            area,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "missing_feature_total": 0.0,
                "top_rows": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["lost_score_sum"] += max(0.0, 1.0 - score)
        bucket["behavior_status_counts"][behavior_status] += 1
        bucket["stage_first_failure_counts"][first_stage] += 1
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["top_rows"].append(triage_row_summary(row))

    def add_axis_row(axis: str, row: dict[str, Any], behavior_status: str, first_stage: str, score: float) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        metrics = improvement_axis_metrics.setdefault(
            axis,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "missing_feature_total": 0.0,
                "top_rows": [],
            },
        )
        metrics["row_count"] += 1
        metrics["score_sum"] += score
        metrics["lost_score_sum"] += max(0.0, 1.0 - score)
        metrics["behavior_status_counts"][behavior_status] += 1
        metrics["stage_first_failure_counts"][first_stage] += 1
        metrics["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        metrics["top_rows"].append(triage_row_summary(row))

    def add_complexity_row(bucket_name: str, row: dict[str, Any], score: float, behavior_status: str) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        bucket = complexity_buckets.setdefault(
            bucket_name,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "behavior_pass_count": 0,
                "missing_feature_total": 0.0,
                "zero_score_count": 0,
                "source_line_counts": [],
                "source_feature_counts": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["behavior_pass_count"] += int(behavior_status == "pass")
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["zero_score_count"] += int(score == 0.0)
        source_lines = row.get("source_body_line_count")
        source_features = row.get("source_static_feature_count")
        if isinstance(source_lines, int | float):
            bucket["source_line_counts"].append(float(source_lines))
        if isinstance(source_features, int | float):
            bucket["source_feature_counts"].append(float(source_features))

    def dynamic_semantic_axis(behavior_status: str) -> str:
        if behavior_status == "pass":
            return "dynamic_pass"
        if behavior_status == "mismatch":
            return "dynamic_mismatch"
        if behavior_status == "unsupported_signature":
            return "dynamic_unsupported"
        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
            "decomp_failed",
        }:
            return "dynamic_harness_or_decomp_blocked"
        return "dynamic_unknown"

    def static_semantic_axis(row: dict[str, Any]) -> str:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        source_total = float(static_gaps.get("source_feature_total", 0.0) or 0.0)
        decomp_total = float(static_gaps.get("decomp_feature_total", 0.0) or 0.0)
        if source_total == 0.0:
            return "static_no_source_features"
        if decomp_total == 0.0:
            return "static_no_decomp_features"
        if float(row.get("static_semantic_score", 0.0) or 0.0) >= 1.0:
            return "static_perfect"
        return "static_gap"

    def add_semantic_quality_quadrant(row: dict[str, Any], behavior_status: str, score: float) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        quadrant = f"{dynamic_semantic_axis(behavior_status)}|{static_semantic_axis(row)}"
        bucket = semantic_quality_quadrants.setdefault(
            quadrant,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "missing_feature_total": 0.0,
                "extra_feature_total": 0.0,
                "top_rows": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["lost_score_sum"] += max(0.0, 1.0 - score)
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["extra_feature_total"] += float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
        if score < 1.0:
            bucket["top_rows"].append(triage_row_summary(row))

    def add_coverage_blind_spot(kind: str, row: dict[str, Any]) -> None:
        coverage_blind_spot_counts[kind] += 1
        coverage_blind_spot_rows.setdefault(kind, []).append(triage_row_summary(row))

    for row in rows:
        score = float(row.get("semantic_score", 0.0) or 0.0)
        behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
        behavior_status = str(behavior.get("status", "unknown"))
        raw_behavior_score = behavior.get("score")
        behavior_score = (
            float(raw_behavior_score)
            if isinstance(raw_behavior_score, int | float)
            else (1.0 if behavior_status == "pass" else 0.0)
        )
        static_score = float(row.get("static_semantic_score", 0.0) or 0.0)
        behavior_score_values.append(behavior_score)
        static_score_values.append(static_score)
        behavior_component_loss = round(0.65 * max(0.0, 1.0 - behavior_score), 6)
        static_component_loss = round(0.35 * max(0.0, 1.0 - static_score), 6)
        if behavior_component_loss > 0.0 or static_component_loss > 0.0:
            component_loss_hot_rows.append(
                {
                    **triage_row_summary(row),
                    "behavior_score": round(behavior_score, 6),
                    "static_score": round(static_score, 6),
                    "behavior_component_loss": behavior_component_loss,
                    "static_component_loss": static_component_loss,
                    "total_component_loss": round(behavior_component_loss + static_component_loss, 6),
                }
            )
        static_source_variant = str(row.get("static_similarity_source_variant") or "direct_source")
        static_source_variant_counts[static_source_variant] += 1
        direct_static_score = row.get("static_semantic_score_direct")
        expanded_static_score = row.get("static_semantic_score_inline_expanded")
        if isinstance(direct_static_score, int | float) and isinstance(expanded_static_score, int | float):
            delta = round(float(expanded_static_score) - float(direct_static_score), 6)
            if delta > 0.0:
                inline_expanded_static_score_deltas.append(delta)
                inline_expanded_static_hot_rows.append(
                    {
                        "entry_id": row.get("entry_id"),
                        "function_name": row.get("function_name"),
                        "address": row.get("address"),
                        "direct_static_semantic_score_percent": percent(float(direct_static_score)),
                        "inline_expanded_static_semantic_score_percent": percent(float(expanded_static_score)),
                        "static_score_delta_percent": percent(delta),
                        "semantic_score_percent": row.get("semantic_score_percent"),
                    }
                )
        first_stage = str(row.get("stage_first_failure") or "none")
        score_loss = round(max(0.0, 1.0 - score), 6)
        preview_stats = row.get("preview_build_stats")
        preview_stats_dict = preview_stats if isinstance(preview_stats, dict) else None
        debug_decomp = row.get("debug_decomp")
        debug_decomp_dict = debug_decomp if isinstance(debug_decomp, dict) else None
        stage_status_for_readiness = (
            debug_decomp.get("stage_status")
            if isinstance(debug_decomp, dict) and isinstance(debug_decomp.get("stage_status"), dict)
            else {}
        )
        all_pipeline_stages_ok = bool(stage_status_for_readiness) and all(
            stage_status_for_readiness.get(stage) == "ok"
            for stage in STAGE_FAILURE_ORDER
            if stage != "load"
        )
        semantic_readiness_counts["manifest_rows"] += 1
        semantic_readiness_counts["fully_perfect_rows"] += int(score == 1.0)
        semantic_readiness_counts["partial_credit_rows"] += int(0.0 < score < 1.0)
        semantic_readiness_counts["zero_credit_rows"] += int(score == 0.0)
        semantic_readiness_counts["behavior_pass_static_perfect_rows"] += int(
            behavior_status == "pass" and static_score == 1.0
        )
        semantic_readiness_counts["behavior_pass_static_gap_rows"] += int(
            behavior_status == "pass" and static_score < 1.0
        )
        semantic_readiness_counts["static_perfect_behavior_nonpass_rows"] += int(
            behavior_status != "pass" and static_score == 1.0
        )
        semantic_readiness_counts["pipeline_ok_behavior_nonpass_rows"] += int(
            all_pipeline_stages_ok and behavior_status != "pass"
        )
        semantic_readiness_counts["pipeline_blocked_rows"] += int(first_stage != "none")
        add_semantic_quality_quadrant(row, behavior_status, score)
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            sleigh_blocker_rows.append(triage_row_summary(row))
        if row.get("mapping_status") != "matched":
            add_coverage_blind_spot("unmapped_source_function", row)
        if row.get("mapping_status") == "matched" and not row.get("decomp_success"):
            add_coverage_blind_spot("mapped_but_decompile_failed", row)
        if not isinstance(debug_decomp, dict):
            add_coverage_blind_spot("missing_debug_decomp_evidence", row)
        if behavior_status == "unsupported_signature":
            add_coverage_blind_spot("unsupported_behavior_signature", row)
        if row.get("behavior", {}).get("eligible") is True and behavior_status not in {"pass", "mismatch"}:
            add_coverage_blind_spot("eligible_behavior_not_executed", row)
        if score_loss > 0.0:
            loss_reason = row_zero_credit_reason(row) if score == 0.0 else "partial_credit"
            semantic_loss_by_behavior_status[behavior_status] += score_loss
            semantic_loss_by_stage_first_failure[first_stage] += score_loss
            semantic_loss_by_zero_credit_reason[loss_reason] += score_loss
            semantic_loss_hot_rows.append(
                {
                    "entry_id": row.get("entry_id"),
                    "function_name": row.get("function_name"),
                    "address": row.get("address"),
                    "semantic_score_percent": row.get("semantic_score_percent"),
                    "lost_score": score_loss,
                    "behavior_status": behavior_status,
                    "stage_first_failure": first_stage,
                    "zero_credit_reason": loss_reason,
                }
            )
        static_gaps_for_outcome = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        static_missing_for_outcome = float(static_gaps_for_outcome.get("missing_feature_total", 0.0) or 0.0)
        static_extra_for_outcome = float(static_gaps_for_outcome.get("extra_feature_total", 0.0) or 0.0)
        static_gap_bucket_for_outcome = (
            "static_perfect"
            if static_missing_for_outcome == 0.0 and static_extra_for_outcome == 0.0
            else f"missing:{feature_gap_bucket(static_missing_for_outcome)}|extra:{feature_gap_bucket(static_extra_for_outcome)}"
        )
        outcome_key = (
            f"mapping:{row.get('mapping_status', 'unknown')}|"
            f"stage:{first_stage}|behavior:{behavior_status}|static:{static_gap_bucket_for_outcome}"
        )
        outcome_matrix_counts[outcome_key] += 1
        outcome_matrix_lost_score[outcome_key] += score_loss
        if score_loss > 0.0:
            outcome_matrix_rows.setdefault(outcome_key, []).append(triage_row_summary(row))
        score_values_by_behavior_status.setdefault(behavior_status, []).append(score)
        score_values_by_stage_first_failure.setdefault(first_stage, []).append(score)
        axis = improvement_axis_for(row, behavior, first_stage)
        add_axis_row(axis, row, behavior_status, first_stage, score)
        areas = focus_areas_for(row, behavior, first_stage, preview_stats_dict, debug_decomp_dict)
        for area in areas:
            add_focus_area_row(area, row, behavior_status, first_stage, score)
        if "sleigh_runtime_lift" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p1_sleigh_lift_correctness",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "type_data_abstraction" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p2_type_data_abstraction",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "structuring_render" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p3_structuring_hard_cases",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "mapping_name_recovery" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p4_fid_name_recovery",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if row.get("binary_arch") not in {None, "unknown"} and score < 1.0:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p5_architecture_breadth",
                row,
                behavior_status,
                first_stage,
                score,
            )
        source_complexity_value = float(row.get("source_static_feature_count") or 0.0)
        add_complexity_row(complexity_bucket(source_complexity_value), row, score, behavior_status)
        if isinstance(row.get("decomp_wall_sec"), int | float):
            decompile_sec = float(row.get("decomp_wall_sec") or 0.0)
            cost_values_by_behavior_status.setdefault(behavior_status, []).append(decompile_sec)
            cost_values_by_stage_first_failure.setdefault(first_stage, []).append(decompile_sec)
            score_bucket_name = (
                "perfect"
                if score == 1.0
                else "zero"
                if score == 0.0
                else "low"
                if score < 0.25
                else "medium"
                if score < 0.75
                else "high"
            )
            cost_values_by_score_bucket.setdefault(score_bucket_name, []).append(decompile_sec)
            cost_bucket_name = cost_bucket(decompile_sec)
            scores_by_cost_bucket.setdefault(cost_bucket_name, []).append(score)
            lost_score_by_cost_bucket[cost_bucket_name] += score_loss
        behavior_status_by_stage_first_failure.setdefault(first_stage, Counter())[behavior_status] += 1
        zero_reason = row_zero_credit_reason(row) if score == 0.0 else "nonzero"
        behavior_status_by_zero_credit_reason.setdefault(zero_reason, Counter())[behavior_status] += 1
        if behavior_status != "pass":
            failure_owner = behavior_failure_owner(behavior_status)
            behavior_failure_owner_counts[failure_owner] += 1
            detail_signature = behavior_detail_signature(behavior.get("detail"))
            if detail_signature != "none":
                behavior_failure_detail_counts[detail_signature] += 1
                behavior_failure_detail_rows.setdefault(detail_signature, []).append(triage_row_summary(row))
        source_lines = row.get("source_body_line_count")
        decomp_lines = row.get("decomp_line_count")
        source_bytes = row.get("source_body_byte_count")
        decomp_bytes = row.get("decomp_byte_count")
        if isinstance(source_lines, int | float):
            source_body_line_counts.append(float(source_lines))
        if isinstance(decomp_lines, int | float):
            decomp_line_counts.append(float(decomp_lines))
        if isinstance(source_bytes, int | float):
            source_body_byte_counts.append(float(source_bytes))
        if isinstance(decomp_bytes, int | float):
            decomp_byte_counts.append(float(decomp_bytes))
        if isinstance(source_lines, int | float) and isinstance(decomp_lines, int | float) and source_lines > 0:
            decomp_to_source_line_ratios.append(float(decomp_lines) / float(source_lines))
        if isinstance(source_bytes, int | float) and isinstance(decomp_bytes, int | float) and source_bytes > 0:
            decomp_to_source_byte_ratios.append(float(decomp_bytes) / float(source_bytes))
        if isinstance(source_lines, int | float) or isinstance(decomp_lines, int | float):
            source_decomp_size_hot_rows.append(
                {
                    "entry_id": row.get("entry_id"),
                    "function_name": row.get("function_name"),
                    "address": row.get("address"),
                    "semantic_score_percent": row.get("semantic_score_percent"),
                    "behavior_status": behavior_status,
                    "source_body_line_count": source_lines,
                    "decomp_line_count": decomp_lines,
                    "decomp_to_source_line_ratio": round(float(decomp_lines) / float(source_lines), 6)
                    if isinstance(source_lines, int | float)
                    and isinstance(decomp_lines, int | float)
                    and source_lines > 0
                    else None,
                }
            )
        case_pass_rate = behavior.get("case_pass_rate")
        if isinstance(case_pass_rate, int | float):
            behavior_case_pass_rates.append(float(case_pass_rate))
        case_pass_count = int(behavior.get("case_pass_count") or 0)
        compared_case_count = int(behavior.get("compared_case_count") or behavior.get("case_count") or 0)
        if behavior_status != "pass" and case_pass_count > 0:
            behavior_partial_progress_rows.append(
                {
                    **triage_row_summary(row),
                    "behavior_status": behavior_status,
                    "case_pass_count": case_pass_count,
                    "case_fail_count": int(behavior.get("case_fail_count") or 0),
                    "compared_case_count": compared_case_count,
                    "case_pass_rate": round(case_pass_count / compared_case_count, 6)
                    if compared_case_count
                    else 0.0,
                    "first_mismatch_index": behavior.get("first_mismatch_index"),
                    "candidate_missing_line_count": int(behavior.get("candidate_missing_line_count") or 0),
                    "candidate_extra_line_count": int(behavior.get("candidate_extra_line_count") or 0),
                }
            )
        cost_hot_rows.append(
            {
                "entry_id": row.get("entry_id"),
                "function_name": row.get("function_name"),
                "address": row.get("address"),
                "semantic_score_percent": row.get("semantic_score_percent"),
                "behavior_status": behavior_status,
                "decompile_sec": row.get("decomp_wall_sec"),
                "behavior_wall_sec": behavior.get("wall_sec"),
            }
        )
        if score == 1.0:
            score_distribution["perfect"] += 1
        elif score == 0.0:
            score_distribution["zero"] += 1
        elif score < 0.25:
            score_distribution["low"] += 1
        elif score < 0.75:
            score_distribution["medium"] += 1
        else:
            score_distribution["high"] += 1
        for component, value in (row.get("static_similarity_components") or {}).items():
            if isinstance(value, int | float):
                static_component_sums[component] += float(value)
        static_gaps = row.get("static_similarity_gaps")
        if isinstance(static_gaps, dict):
            row_source_total = float(static_gaps.get("source_feature_total", 0.0) or 0.0)
            row_decomp_total = float(static_gaps.get("decomp_feature_total", 0.0) or 0.0)
            row_intersection_total = float(static_gaps.get("intersection_feature_total", 0.0) or 0.0)
            row_union_total = float(static_gaps.get("union_feature_total", 0.0) or 0.0)
            row_missing_total = float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
            row_extra_total = float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
            missing_density = round(row_missing_total / row_source_total, 6) if row_source_total else 0.0
            extra_density = round(row_extra_total / row_decomp_total, 6) if row_decomp_total else 0.0
            static_missing_density_values.append(missing_density)
            static_extra_density_values.append(extra_density)
            missing_gap_bucket = feature_gap_bucket(row_missing_total)
            static_score_by_missing_gap_bucket.setdefault(missing_gap_bucket, []).append(static_score)
            if row_missing_total > 0.0 or row_extra_total > 0.0:
                static_gap_hot_rows.append(
                    {
                        **triage_row_summary(row),
                        "static_semantic_score_percent": row.get("static_semantic_score_percent"),
                        "source_feature_total": row_source_total,
                        "decomp_feature_total": row_decomp_total,
                        "intersection_feature_total": row_intersection_total,
                        "union_feature_total": row_union_total,
                        "missing_feature_total": row_missing_total,
                        "extra_feature_total": row_extra_total,
                        "missing_density": missing_density,
                        "extra_density": extra_density,
                        "top_missing_features": (static_gaps.get("top_missing_features") or [])[:5],
                        "top_extra_features": (static_gaps.get("top_extra_features") or [])[:5],
                    }
                )
                density_key = f"missing:{missing_gap_bucket}|extra:{feature_gap_bucket(row_extra_total)}"
                density_bucket = static_gap_density_rows.setdefault(
                    density_key,
                    {
                        "row_count": 0,
                        "score_sum": 0.0,
                        "missing_feature_total": 0.0,
                        "extra_feature_total": 0.0,
                        "top_rows": [],
                    },
                )
                density_bucket["row_count"] += 1
                density_bucket["score_sum"] += score
                density_bucket["missing_feature_total"] += row_missing_total
                density_bucket["extra_feature_total"] += row_extra_total
                density_bucket["top_rows"].append(triage_row_summary(row))
            source_feature_total_values.append(row_source_total)
            direct_feature_total = row.get("source_static_feature_count_direct")
            expanded_feature_total = row.get("source_static_feature_count_inline_expanded")
            if isinstance(direct_feature_total, int | float):
                source_feature_total_direct_values.append(float(direct_feature_total))
            if isinstance(expanded_feature_total, int | float):
                source_feature_total_inline_expanded_values.append(float(expanded_feature_total))
            decomp_feature_total_values.append(row_decomp_total)
            static_intersection_feature_total_values.append(row_intersection_total)
            static_union_feature_total_values.append(row_union_total)
            if (
                float(row.get("semantic_score", 0.0) or 0.0) < 1.0
                and (row_source_total >= 40.0 or float(row.get("source_body_line_count") or 0.0) >= 40.0)
            ):
                hard_function_rows.append(
                    {
                        **triage_row_summary(row),
                        "source_feature_total": row_source_total,
                        "source_body_line_count": row.get("source_body_line_count"),
                        "decomp_wall_sec": row.get("decomp_wall_sec"),
                    }
                )
            source_feature_rows += int(row_source_total > 0.0)
            decomp_feature_rows += int(row_decomp_total > 0.0)
            static_decomp_absent_feature_rows += int(row_source_total > 0.0 and row_decomp_total == 0.0)
            if row_source_total > 0.0 and row_decomp_total == 0.0:
                add_coverage_blind_spot("source_features_without_decomp_features", row)
            static_missing_feature_rows += int(row_missing_total > 0.0)
            static_extra_feature_rows += int(row_extra_total > 0.0)
            static_zero_similarity_rows += int(row_source_total > 0.0 and row_intersection_total == 0.0)
            missing_feature_count_values.append(row_missing_total)
            extra_feature_count_values.append(row_extra_total)
            for key in [
                "source_feature_total",
                "decomp_feature_total",
                "intersection_feature_total",
                "union_feature_total",
                "missing_feature_total",
                "extra_feature_total",
            ]:
                value = static_gaps.get(key)
                if isinstance(value, int | float):
                    static_gap_totals[key] += value
            for item in static_gaps.get("top_missing_features") or []:
                if isinstance(item, dict) and isinstance(item.get("feature"), str) and isinstance(item.get("count"), int | float):
                    static_missing_feature_counts[item["feature"]] += item["count"]
            for item in static_gaps.get("top_extra_features") or []:
                if isinstance(item, dict) and isinstance(item.get("feature"), str) and isinstance(item.get("count"), int | float):
                    static_extra_feature_counts[item["feature"]] += item["count"]
        gap_components = row.get("static_similarity_gap_components")
        if isinstance(gap_components, dict):
            for component, details in gap_components.items():
                if component not in static_gap_component_totals or not isinstance(details, dict):
                    continue
                component_missing_total = float(details.get("missing_feature_total", 0.0) or 0.0)
                component_extra_total = float(details.get("extra_feature_total", 0.0) or 0.0)
                component_source_total = float(details.get("source_feature_total", 0.0) or 0.0)
                component_decomp_total = float(details.get("decomp_feature_total", 0.0) or 0.0)
                component_intersection_total = float(details.get("intersection_feature_total", 0.0) or 0.0)
                component_source_present = component_source_total > 0.0
                component_decomp_present = component_decomp_total > 0.0
                component_intersection_present = component_intersection_total > 0.0
                static_component_source_feature_values[component].append(component_source_total)
                static_component_decomp_feature_values[component].append(component_decomp_total)
                static_component_missing_row_counts[component] += int(component_missing_total > 0.0)
                static_component_zero_similarity_row_counts[component] += int(
                    component_source_total > 0.0 and component_intersection_total == 0.0
                )
                absence_counts = static_component_absence_counts[component]
                absence_counts["observed_row_count"] += 1
                absence_counts["source_present_row_count"] += int(component_source_present)
                absence_counts["decomp_present_row_count"] += int(component_decomp_present)
                absence_counts["intersection_present_row_count"] += int(component_intersection_present)
                if component_source_present and component_decomp_present:
                    absence_counts["both_present_row_count"] += 1
                elif component_source_present:
                    absence_counts["source_only_row_count"] += 1
                elif component_decomp_present:
                    absence_counts["decomp_only_row_count"] += 1
                else:
                    absence_counts["both_absent_row_count"] += 1
                if component_source_present and not component_intersection_present:
                    absence_counts["zero_intersection_source_present_row_count"] += 1
                if component_source_present and not component_decomp_present:
                    static_component_absence_rows[component]["source_only_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                if component_decomp_present and not component_source_present:
                    static_component_absence_rows[component]["decomp_only_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                if component_source_present and not component_intersection_present:
                    static_component_absence_rows[component]["zero_intersection_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                for key in [
                    "source_feature_total",
                    "decomp_feature_total",
                    "intersection_feature_total",
                    "union_feature_total",
                    "missing_feature_total",
                    "extra_feature_total",
                ]:
                    value = details.get(key)
                    if isinstance(value, int | float):
                        static_gap_component_totals[component][key] += value
                for item in details.get("top_missing_features") or []:
                    if (
                        isinstance(item, dict)
                        and isinstance(item.get("feature"), str)
                        and isinstance(item.get("count"), int | float)
                    ):
                        static_gap_component_missing_features[component][item["feature"]] += item["count"]
                for item in details.get("top_extra_features") or []:
                    if (
                        isinstance(item, dict)
                        and isinstance(item.get("feature"), str)
                        and isinstance(item.get("count"), int | float)
                    ):
                        static_gap_component_extra_features[component][item["feature"]] += item["count"]
                if component == "signature":
                    signedness_gap = signedness_only_signature_gap(details)
                    total_signedness_gap = signedness_gap["param_pair_count"] + signedness_gap["return_pair_count"]
                    if total_signedness_gap > 0.0:
                        for key, value in signedness_gap.items():
                            signedness_only_signature_gap_totals[key] += value
                        signedness_only_signature_gap_rows.append(
                            {
                                **triage_row_summary(row),
                                **{
                                    key: round(value, 6)
                                    for key, value in signedness_gap.items()
                                    if value > 0.0
                                },
                            }
                        )
                if component_missing_total > 0.0 or component_extra_total > 0.0:
                    component_row = {
                        **triage_row_summary(row),
                        "component": component,
                        "component_missing_feature_total": component_missing_total,
                        "component_extra_feature_total": component_extra_total,
                        "component_source_feature_total": component_source_total,
                        "component_decomp_feature_total": component_decomp_total,
                    }
                    if component == "signature":
                        signature_gap_rows.append(component_row)
                    elif component == "memory":
                        memory_gap_rows.append(component_row)
                    elif component == "call":
                        call_gap_rows.append(component_row)
                    elif component == "control_flow":
                        control_flow_gap_rows.append(component_row)
        source_return_kind = str(row.get("source_return_kind") or "unknown")
        decomp_return_kind = str(row.get("decomp_return_kind") or "missing")
        source_param_kinds = row.get("source_param_kinds") if isinstance(row.get("source_param_kinds"), list) else []
        decomp_param_kinds = row.get("decomp_param_kinds") if isinstance(row.get("decomp_param_kinds"), list) else []
        signature_return_pair_counts[f"{source_return_kind}->{decomp_return_kind}"] += 1
        max_param_count = max(len(source_param_kinds), len(decomp_param_kinds))
        param_mismatch_count = 0
        missing_param_count = 0
        extra_param_count = 0
        for index in range(max_param_count):
            source_kind = str(source_param_kinds[index]) if index < len(source_param_kinds) else "missing"
            decomp_kind = str(decomp_param_kinds[index]) if index < len(decomp_param_kinds) else "missing"
            signature_param_pair_counts[f"{source_kind}->{decomp_kind}"] += 1
            param_mismatch_count += int(source_kind != decomp_kind)
            missing_param_count += int(source_kind != "missing" and decomp_kind == "missing")
            extra_param_count += int(source_kind == "missing" and decomp_kind != "missing")
        return_mismatch = source_return_kind != decomp_return_kind
        arity_mismatch = len(source_param_kinds) != len(decomp_param_kinds)
        if return_mismatch or param_mismatch_count > 0:
            signature_pair_gap_rows.append(
                {
                    **triage_row_summary(row),
                    "source_return_kind": source_return_kind,
                    "decomp_return_kind": decomp_return_kind,
                    "source_param_kinds": source_param_kinds,
                    "decomp_param_kinds": decomp_param_kinds,
                    "param_mismatch_count": param_mismatch_count,
                    "missing_param_count": missing_param_count,
                    "extra_param_count": extra_param_count,
                    "return_mismatch": return_mismatch,
                }
            )
        if arity_mismatch:
            signature_param_arity_mismatch_rows.append(
                {
                    **triage_row_summary(row),
                    "source_param_count": len(source_param_kinds),
                    "decomp_param_count": len(decomp_param_kinds),
                    "missing_param_count": missing_param_count,
                    "extra_param_count": extra_param_count,
                    "source_param_kinds": source_param_kinds,
                    "decomp_param_kinds": decomp_param_kinds,
                }
            )
        lang = row["language"]
        bucket = by_language.setdefault(
            lang,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(bucket, row)

        arch = str(row.get("binary_arch") or "unknown")
        arch_bucket = by_arch.setdefault(
            arch,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(arch_bucket, row)
        arch_metrics = architecture_stage_metrics.setdefault(
            arch,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "missing_feature_total": 0.0,
                "extra_feature_total": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "top_rows": [],
            },
        )
        arch_metrics["row_count"] += 1
        arch_metrics["score_sum"] += score
        arch_metrics["lost_score_sum"] += score_loss
        static_gaps_for_arch = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        arch_metrics["missing_feature_total"] += float(static_gaps_for_arch.get("missing_feature_total", 0.0) or 0.0)
        arch_metrics["extra_feature_total"] += float(static_gaps_for_arch.get("extra_feature_total", 0.0) or 0.0)
        arch_metrics["behavior_status_counts"][behavior_status] += 1
        arch_metrics["stage_first_failure_counts"][first_stage] += 1
        if score < 1.0:
            arch_metrics["top_rows"].append(triage_row_summary(row))

        return_kind = str(row.get("source_return_kind") or "unknown")
        return_bucket = by_source_return_kind.setdefault(
            return_kind,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(return_bucket, row)

        param_shape = str(row.get("source_param_shape") or "unknown")
        param_bucket = by_source_param_shape.setdefault(
            param_shape,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(param_bucket, row)

        entry_bucket = by_entry.setdefault(
            row["entry_id"],
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(entry_bucket, row)
        if row.get("mapping_status") != "matched" or (
            row.get("fission_name")
            and row.get("function_name")
            and normalize_name(str(row.get("fission_name"))) != normalize_name(str(row.get("function_name")))
        ):
            name_recovery_rows.append(triage_row_summary(row))
            if "mapping_name_recovery" not in areas:
                add_priority_bucket_row(
                    roadmap_priority_metrics,
                    "p4_fid_name_recovery",
                    row,
                    behavior_status,
                    first_stage,
                    score,
                )

        if isinstance(debug_decomp, dict):
            debug_decomp_row_count += 1
            debug_owner_bucket_counts.update(debug_decomp.get("owner_buckets") or [])
            stage_status = debug_decomp.get("stage_status")
            if isinstance(stage_status, dict):
                debug_stage_status_row_count += 1
                furthest_stage = furthest_ok_stage(stage_status)
                stage_furthest_ok_counts[furthest_stage] += 1
                if first_stage != "none":
                    stage_first_blocker_lost_score[first_stage] += score_loss
                pipeline_statuses = [
                    stage_status.get(stage)
                    for stage in STAGE_FAILURE_ORDER
                    if stage_status.get(stage) is not None
                ]
                pipeline_ok = bool(pipeline_statuses) and all(status == "ok" for status in pipeline_statuses)
                stage_funnel_counts["mapped_with_debug_stage_status"] += 1
                if pipeline_ok:
                    stage_funnel_counts["all_pipeline_stages_ok"] += 1
                for stage in STAGE_FAILURE_ORDER:
                    if stage_status.get(stage) == "ok":
                        stage_funnel_counts[f"{stage}_ok"] += 1
                debug_stage_status_counts.update(
                    f"{stage}:{status}"
                    for stage, status in stage_status.items()
                    if status is not None
                )
                for stage in STAGE_FAILURE_ORDER:
                    status = stage_status.get(stage)
                    stage_status_metrics[stage][str(status if status is not None else "missing")] += 1
                    if status is not None:
                        debug_stage_status_matrix[stage][str(status)] += 1
            quality = debug_decomp.get("quality_evidence")
            if isinstance(quality, dict):
                for key, value in numeric_items(quality):
                    debug_quality_evidence_totals[key] += value
                    if value != 0:
                        debug_quality_evidence_nonzero_rows[key] += 1
            pipeline = debug_decomp.get("rust_sleigh_pipeline")
            add_numeric_debug_pipeline_values(debug_pipeline_numeric_values, pipeline)
            template_sources = (
                pipeline.get("template_source_counts")
                if isinstance(pipeline, dict)
                and isinstance(pipeline.get("template_source_counts"), dict)
                else {}
            )
            for key, value in template_sources.items():
                if isinstance(value, int | float):
                    debug_template_source_totals[canonical_sleigh_template_source(str(key))] += value

        if isinstance(preview_stats, dict):
            nir_build_stats_row_count += 1
            row_debt_total = 0.0
            row_debt_metrics: dict[str, float] = {}
            for key, value in numeric_items(preview_stats):
                nir_build_stats_numeric_totals[key] += value
                nir_build_stats_values.setdefault(key, []).append(value)
                if value != 0:
                    nir_build_stats_nonzero_rows[key] += 1
                if is_debt_metric_name(key) and value != 0:
                    row_debt_metrics[key] = value
                    row_debt_total += value
            if row_debt_metrics:
                nir_build_stats_debt_hot_rows.append(
                    {
                        "entry_id": row.get("entry_id"),
                        "function_name": row.get("function_name"),
                        "address": row.get("address"),
                        "semantic_score_percent": row.get("semantic_score_percent"),
                        "behavior_status": behavior_status,
                        "stage_first_failure": first_stage,
                        "debt_metric_total": round(row_debt_total, 6),
                        "top_debt_metrics": [
                            {"metric": key, "value": value}
                            for key, value in sorted(
                                row_debt_metrics.items(),
                                key=lambda item: (item[1], item[0]),
                                reverse=True,
                            )[:10]
                        ],
                    }
                )
                nir_debt_row_count += 1
                nir_debt_score_values.append(score)
                nir_debt_behavior_status_counts[behavior_status] += 1
                nir_debt_stage_first_failure_counts[first_stage] += 1
            else:
                nir_no_debt_score_values.append(score)

        for tag in row.get("tags") or []:
            tag_bucket = by_tag.setdefault(
                tag,
                {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
            )
            add_bucket(tag_bucket, row)

        if isinstance(behavior, dict) and behavior.get("status") == "mismatch":
            first_mismatch = behavior.get("first_mismatch_index")
            behavior_first_mismatch_index_counts[str(first_mismatch)] += 1
            oracle = behavior.get("oracle")
            candidate = behavior.get("candidate")
            if isinstance(oracle, list) and isinstance(candidate, list):
                length_delta = len(candidate) - len(oracle)
                behavior_output_length_delta_counts[str(length_delta)] += 1
                if length_delta != 0:
                    behavior_mismatch_kind_counts["output_length"] += 1
                    if length_delta < 0:
                        behavior_missing_candidate_line_total += abs(length_delta)
                    else:
                        behavior_extra_candidate_line_total += length_delta
                else:
                    behavior_mismatch_kind_counts["wrong_value"] += 1
            else:
                behavior_mismatch_kind_counts["unknown"] += 1

    for bucket in (
        list(by_language.values())
        + list(by_arch.values())
        + list(by_source_return_kind.values())
        + list(by_source_param_shape.values())
        + list(by_tag.values())
        + list(by_entry.values())
    ):
        count = max(1, bucket["row_count"])
        avg_score = round(bucket.pop("score_sum") / count, 6)
        bucket["avg_semantic_score"] = avg_score
        bucket["avg_semantic_score_percent"] = percent(avg_score)
    host_statuses = Counter(
        row.get("behavior", {}).get("reason")
        for row in rows
        if row.get("behavior", {}).get("status") == "host_execution_unavailable"
    )
    weighted_semantic_similarity = round(sum(score_values) / total, 6) if total else 0.0
    decomp_times = [float(row.get("decomp_wall_sec") or 0.0) for row in rows if isinstance(row.get("decomp_wall_sec"), int | float)]
    behavior_compile_times = [
        float(row.get("behavior", {}).get("compile_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("compile_sec"), int | float)
    ]
    behavior_run_times = [
        float(row.get("behavior", {}).get("run_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("run_sec"), int | float)
    ]
    behavior_wall_times = [
        float(row.get("behavior", {}).get("wall_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("wall_sec"), int | float)
    ]
    behavior_case_total = sum(
        int(row.get("behavior", {}).get("case_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_compared_case_total = sum(
        int(row.get("behavior", {}).get("compared_case_count") or row.get("behavior", {}).get("case_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_case_pass_total = sum(
        int(row.get("behavior", {}).get("case_pass_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_case_fail_total = sum(
        int(row.get("behavior", {}).get("case_fail_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    partial_mismatch_rows = sum(
        1
        for row in rows
        if row.get("behavior", {}).get("status") == "mismatch"
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    )
    partial_progress_rows = sum(
        1
        for row in rows
        if row.get("behavior", {}).get("status")
        in {"mismatch", "candidate_run_timeout", "candidate_run_failed"}
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    )
    partial_timeout_rows = [
        row
        for row in rows
        if row.get("behavior", {}).get("status") == "candidate_run_timeout"
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    ]
    partial_timeout_case_pass_total = sum(
        int(row.get("behavior", {}).get("case_pass_count") or 0)
        for row in partial_timeout_rows
    )
    partial_timeout_compared_case_total = sum(
        int(
            row.get("behavior", {}).get("compared_case_count")
            or row.get("behavior", {}).get("case_count")
            or 0
        )
        for row in partial_timeout_rows
    )
    partial_timeout_missing_line_total = sum(
        int(row.get("behavior", {}).get("candidate_missing_line_count") or 0)
        for row in partial_timeout_rows
    )
    static_source_total = float(static_gap_totals.get("source_feature_total", 0.0) or 0.0)
    static_decomp_total = float(static_gap_totals.get("decomp_feature_total", 0.0) or 0.0)
    static_gap_summary = dict(sorted(static_gap_totals.items()))
    static_gap_summary["missing_feature_rate"] = round(
        float(static_gap_totals.get("missing_feature_total", 0.0) or 0.0) / static_source_total,
        6,
    ) if static_source_total else 0.0
    static_gap_summary["extra_feature_rate"] = round(
        float(static_gap_totals.get("extra_feature_total", 0.0) or 0.0) / static_decomp_total,
        6,
    ) if static_decomp_total else 0.0
    static_gap_summary["top_missing_features"] = [
        {"feature": feature, "count": count}
        for feature, count in static_missing_feature_counts.most_common(20)
    ]
    static_gap_summary["top_extra_features"] = [
        {"feature": feature, "count": count}
        for feature, count in static_extra_feature_counts.most_common(20)
    ]
    static_intersection_total = float(static_gap_totals.get("intersection_feature_total", 0.0) or 0.0)
    static_union_total = float(static_gap_totals.get("union_feature_total", 0.0) or 0.0)
    static_missing_total = float(static_gap_totals.get("missing_feature_total", 0.0) or 0.0)
    static_extra_total = float(static_gap_totals.get("extra_feature_total", 0.0) or 0.0)
    static_gap_component_summary: dict[str, dict[str, Any]] = {}
    static_gap_component_top_summary: dict[str, dict[str, Any]] = {}
    static_component_precision_recall_summary: dict[str, dict[str, Any]] = {}
    for component, totals in static_gap_component_totals.items():
        component_source_total = float(totals.get("source_feature_total", 0.0) or 0.0)
        component_decomp_total = float(totals.get("decomp_feature_total", 0.0) or 0.0)
        component_intersection_total = float(totals.get("intersection_feature_total", 0.0) or 0.0)
        component_precision = (
            round(component_intersection_total / component_decomp_total, 6)
            if component_decomp_total
            else 0.0
        )
        component_recall = (
            round(component_intersection_total / component_source_total, 6)
            if component_source_total
            else 0.0
        )
        component_f1 = (
            round((2.0 * component_precision * component_recall) / (component_precision + component_recall), 6)
            if component_precision + component_recall
            else 0.0
        )
        static_gap_component_summary[component] = dict(sorted(totals.items()))
        static_gap_component_summary[component]["missing_feature_rate"] = round(
            float(totals.get("missing_feature_total", 0.0) or 0.0) / component_source_total,
            6,
        ) if component_source_total else 0.0
        static_gap_component_summary[component]["extra_feature_rate"] = round(
            float(totals.get("extra_feature_total", 0.0) or 0.0) / component_decomp_total,
            6,
        ) if component_decomp_total else 0.0
        static_component_precision_recall_summary[component] = {
            "source_feature_total": component_source_total,
            "decomp_feature_total": component_decomp_total,
            "intersection_feature_total": component_intersection_total,
            "precision": component_precision,
            "precision_percent": percent(component_precision),
            "recall": component_recall,
            "recall_percent": percent(component_recall),
            "f1": component_f1,
            "f1_percent": percent(component_f1),
        }
        static_gap_component_top_summary[component] = {
            "top_missing_features": [
                {"feature": feature, "count": count}
                for feature, count in static_gap_component_missing_features[component].most_common(12)
            ],
            "top_extra_features": [
                {"feature": feature, "count": count}
                for feature, count in static_gap_component_extra_features[component].most_common(12)
            ],
        }
    static_component_absence_export: dict[str, dict[str, Any]] = {}

    def top_absence_rows(component: str, row_kind: str) -> list[dict[str, Any]]:
        return sorted(
            static_component_absence_rows.get(component, {}).get(row_kind) or [],
            key=lambda row: (
                -float(row.get("component_source_feature_total") or 0.0),
                -float(row.get("component_decomp_feature_total") or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:8]

    for component, counts in sorted(static_component_absence_counts.items()):
        observed = int(counts.get("observed_row_count", 0) or 0)
        static_component_absence_export[component] = {
            "observed_row_count": observed,
            "observed_row_rate_total_denominator": round(observed / total, 6) if total else 0.0,
            "source_present_row_count": int(counts.get("source_present_row_count", 0) or 0),
            "source_present_row_rate_observed_denominator": round(
                float(counts.get("source_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "decomp_present_row_count": int(counts.get("decomp_present_row_count", 0) or 0),
            "decomp_present_row_rate_observed_denominator": round(
                float(counts.get("decomp_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "intersection_present_row_count": int(counts.get("intersection_present_row_count", 0) or 0),
            "intersection_present_row_rate_observed_denominator": round(
                float(counts.get("intersection_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "both_present_row_count": int(counts.get("both_present_row_count", 0) or 0),
            "source_only_row_count": int(counts.get("source_only_row_count", 0) or 0),
            "decomp_only_row_count": int(counts.get("decomp_only_row_count", 0) or 0),
            "both_absent_row_count": int(counts.get("both_absent_row_count", 0) or 0),
            "zero_intersection_source_present_row_count": int(
                counts.get("zero_intersection_source_present_row_count", 0) or 0
            ),
            "source_only_rows": top_absence_rows(component, "source_only_rows"),
            "decomp_only_rows": top_absence_rows(component, "decomp_only_rows"),
            "zero_intersection_rows": top_absence_rows(component, "zero_intersection_rows"),
        }
    triage_priority_rows = [
        triage_row_summary(row)
        for row in sorted(rows, key=row_triage_priority)
        if float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ][:20]
    mapped_debug_denominator = max(1, mapped)
    debug_stage_status_matrix_export = {
        stage: dict(sorted(counts.items()))
        for stage, counts in debug_stage_status_matrix.items()
        if counts
    }
    score_by_behavior_status = {
        status: numeric_distribution(values)
        for status, values in sorted(score_values_by_behavior_status.items())
    }
    score_by_stage_first_failure = {
        stage: numeric_distribution(values)
        for stage, values in sorted(score_values_by_stage_first_failure.items())
    }
    pipeline_stage_metrics = {
        stage: {
            "row_count": sum(counts.values()),
            "ok_count": int(counts.get("ok", 0)),
            "missing_count": int(counts.get("missing", 0)),
            "non_ok_count": sum(count for status, count in counts.items() if status != "ok"),
            "ok_rate": round(float(counts.get("ok", 0)) / sum(counts.values()), 6)
            if sum(counts.values())
            else 0.0,
            "status_counts": dict(sorted(counts.items())),
        }
        for stage, counts in stage_status_metrics.items()
        if counts
    }
    nir_debt_totals = {
        key: value
        for key, value in sorted(nir_build_stats_numeric_totals.items())
        if is_debt_metric_name(key) and value != 0
    }
    nir_build_stats_distributions = {
        key: numeric_distribution(values)
        for key, values in sorted(nir_build_stats_values.items())
        if key in nir_debt_totals
    }
    nir_build_stats_debt_hot_rows = sorted(
        nir_build_stats_debt_hot_rows,
        key=lambda row: (float(row.get("debt_metric_total") or 0.0), row.get("function_name") or ""),
        reverse=True,
    )[:20]
    cost_hot_rows_by_decompile = sorted(
        (
            row
            for row in cost_hot_rows
            if isinstance(row.get("decompile_sec"), int | float)
        ),
        key=lambda row: float(row.get("decompile_sec") or 0.0),
        reverse=True,
    )[:12]
    cost_hot_rows_by_behavior_wall = sorted(
        (
            row
            for row in cost_hot_rows
            if isinstance(row.get("behavior_wall_sec"), int | float)
        ),
        key=lambda row: float(row.get("behavior_wall_sec") or 0.0),
        reverse=True,
    )[:12]
    semantic_loss_hot_rows = sorted(
        semantic_loss_hot_rows,
        key=lambda row: (float(row.get("lost_score") or 0.0), row.get("function_name") or ""),
        reverse=True,
    )[:20]
    source_decomp_size_hot_rows = sorted(
        source_decomp_size_hot_rows,
        key=lambda row: (
            float(row.get("decomp_to_source_line_ratio") or 0.0),
            float(row.get("decomp_line_count") or 0.0),
            row.get("function_name") or "",
        ),
        reverse=True,
    )[:20]
    inline_expanded_static_hot_rows = sorted(
        inline_expanded_static_hot_rows,
        key=lambda row: (
            float(row.get("static_score_delta_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
        reverse=True,
    )[:20]
    improvement_axis_export: dict[str, dict[str, Any]] = {}
    for axis, metrics in sorted(improvement_axis_metrics.items()):
        row_count = int(metrics.get("row_count", 0) or 0)
        top_rows = sorted(
            metrics.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12]
        improvement_axis_export[axis] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
            "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
            "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
            "top_rows": top_rows,
        }
    focus_area_export: dict[str, dict[str, Any]] = {}
    for area, metrics in sorted(focus_area_metrics.items()):
        row_count = int(metrics.get("row_count", 0) or 0)
        top_rows = sorted(
            metrics.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12]
        focus_area_export[area] = {
            "row_count": row_count,
            "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
            "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
            "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
            "top_rows": top_rows,
        }
    complexity_export: dict[str, dict[str, Any]] = {}
    for bucket_name, bucket in sorted(complexity_buckets.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        score_sum_for_bucket = float(bucket.get("score_sum", 0.0) or 0.0)
        complexity_export[bucket_name] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(score_sum_for_bucket / row_count, 6) if row_count else 0.0,
            "avg_semantic_score_percent": percent(round(score_sum_for_bucket / row_count, 6)) if row_count else 0.0,
            "behavior_pass_count": int(bucket.get("behavior_pass_count", 0) or 0),
            "behavior_pass_rate": round(float(bucket.get("behavior_pass_count", 0) or 0) / row_count, 6)
            if row_count
            else 0.0,
            "zero_score_count": int(bucket.get("zero_score_count", 0) or 0),
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "source_line_count_distribution": numeric_distribution(bucket.get("source_line_counts") or []),
            "source_feature_count_distribution": numeric_distribution(bucket.get("source_feature_counts") or []),
        }
    hard_function_rows = sorted(
        hard_function_rows,
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            -float(row.get("source_feature_total") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:20]
    score_sum = round(sum(score_values), 6)
    behavior_score_sum = round(sum(behavior_score_values), 6)
    static_score_sum = round(sum(static_score_values), 6)
    behavior_component_score_sum = round(0.65 * behavior_score_sum, 6)
    static_component_score_sum = round(0.35 * static_score_sum, 6)
    current_weighted_score = round(
        sum(
            (0.65 * behavior_score) + (0.35 * static_score)
            for behavior_score, static_score in zip(behavior_score_values, static_score_values, strict=False)
        ) / total,
        6,
    ) if total else 0.0
    weight_scenarios: dict[str, dict[str, Any]] = {}
    for behavior_weight in [0.0, 0.25, 0.5, 0.65, 0.75, 1.0]:
        static_weight = round(1.0 - behavior_weight, 6)
        scenario_score = round(
            sum(
                (behavior_weight * behavior_score) + (static_weight * static_score)
                for behavior_score, static_score in zip(behavior_score_values, static_score_values, strict=False)
            ) / total,
            6,
        ) if total else 0.0
        scenario_key = f"behavior_{int(round(behavior_weight * 100)):03d}_static_{int(round(static_weight * 100)):03d}"
        weight_scenarios[scenario_key] = {
            "behavior_weight": behavior_weight,
            "static_weight": static_weight,
            "weighted_score": scenario_score,
            "weighted_score_percent": percent(scenario_score),
            "delta_from_current_score": round(scenario_score - current_weighted_score, 6),
            "delta_from_current_score_percent": percent(scenario_score - current_weighted_score),
        }
    behavior_static_score_delta_values = [
        round(behavior_score - static_score, 6)
        for behavior_score, static_score in zip(behavior_score_values, static_score_values, strict=False)
    ]
    component_loss_hot_rows = sorted(
        component_loss_hot_rows,
        key=lambda row: (
            float(row.get("total_component_loss") or 0.0),
            float(row.get("behavior_component_loss") or 0.0),
            float(row.get("static_component_loss") or 0.0),
            str(row.get("function_name") or ""),
        ),
        reverse=True,
    )[:20]
    zero_score_count = int(score_distribution.get("zero", 0))
    nonzero_score_count = sum(1 for score in score_values if score > 0.0)
    perfect_score_count = sum(1 for score in score_values if score == 1.0)

    def row_stage_ok(row: dict[str, Any], stage: str) -> bool:
        debug_decomp = row.get("debug_decomp")
        if not isinstance(debug_decomp, dict):
            return False
        stage_status = debug_decomp.get("stage_status")
        return isinstance(stage_status, dict) and stage_status.get(stage) == "ok"

    admission_gate_counts = Counter(
        {
            "manifest_rows": total,
            "mapped_rows": mapped,
            "decompiled_rows": decomp_ok,
            "decode_ok_rows": sum(1 for row in rows if row_stage_ok(row, "decode")),
            "raw_pcode_ok_rows": sum(1 for row in rows if row_stage_ok(row, "raw_pcode")),
            "nir_build_ok_rows": sum(1 for row in rows if row_stage_ok(row, "nir_build")),
            "normalize_ok_rows": sum(1 for row in rows if row_stage_ok(row, "normalize")),
            "structuring_ok_rows": sum(1 for row in rows if row_stage_ok(row, "structuring")),
            "render_ok_rows": sum(1 for row in rows if row_stage_ok(row, "render")),
            "full_pipeline_ok_rows": sum(
                1
                for row in rows
                if all(row_stage_ok(row, stage) for stage in STAGE_FAILURE_ORDER if stage != "load")
            ),
            "candidate_compiled_rows": compile_ok,
            "behavior_pass_rows": behavior_pass,
            "static_perfect_rows": sum(
                1 for row in rows if float(row.get("static_semantic_score", 0.0) or 0.0) == 1.0
            ),
            "semantic_perfect_rows": perfect_score_count,
        }
    )
    admission_gate_metrics = {
        "gate_order": [
            "manifest_rows",
            "mapped_rows",
            "decompiled_rows",
            "decode_ok_rows",
            "raw_pcode_ok_rows",
            "nir_build_ok_rows",
            "normalize_ok_rows",
            "structuring_ok_rows",
            "render_ok_rows",
            "full_pipeline_ok_rows",
            "candidate_compiled_rows",
            "behavior_pass_rows",
            "static_perfect_rows",
            "semantic_perfect_rows",
        ],
        "counts": dict(admission_gate_counts),
        "rates_total_denominator": {
            key: round(float(value) / total, 6) if total else 0.0
            for key, value in sorted(admission_gate_counts.items())
        },
    }
    stage_transition_metrics = {
        "stage_ok_funnel_counts": dict(sorted(stage_funnel_counts.items())),
        "furthest_ok_stage_counts": dict(sorted(stage_furthest_ok_counts.items())),
        "lost_score_by_first_stage_blocker": {
            key: round(float(value), 6)
            for key, value in sorted(stage_first_blocker_lost_score.items())
        },
    }
    gate_order = [
        "manifest_rows",
        "mapped_rows",
        "decompiled_rows",
        "decode_ok_rows",
        "raw_pcode_ok_rows",
        "nir_build_ok_rows",
        "normalize_ok_rows",
        "structuring_ok_rows",
        "render_ok_rows",
        "full_pipeline_ok_rows",
        "candidate_compiled_rows",
        "behavior_pass_rows",
        "static_perfect_rows",
        "semantic_perfect_rows",
    ]
    gate_drop_rows: dict[str, int] = {}
    gate_retention_rates: dict[str, float] = {}
    previous_gate: str | None = None
    for gate in gate_order:
        count = int(admission_gate_counts.get(gate, 0))
        if previous_gate is not None:
            previous_count = int(admission_gate_counts.get(previous_gate, 0))
            gate_drop_rows[f"{previous_gate}->{gate}"] = max(0, previous_count - count)
            gate_retention_rates[f"{previous_gate}->{gate}"] = round(count / previous_count, 6) if previous_count else 0.0
        previous_gate = gate
    behavior_failure_detail_top_rows = {
        signature: rows_for_signature[:5]
        for signature, rows_for_signature in sorted(
            behavior_failure_detail_rows.items(),
            key=lambda item: (len(item[1]), item[0]),
            reverse=True,
        )[:12]
    }
    static_gap_density_export: dict[str, dict[str, Any]] = {}
    for bucket_name, bucket in sorted(static_gap_density_rows.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        top_rows = sorted(
            bucket.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:8]
        static_gap_density_export[bucket_name] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "extra_feature_total": round(float(bucket.get("extra_feature_total", 0.0) or 0.0), 6),
            "top_rows": top_rows,
        }
    static_gap_hot_row_metrics = {
        "top_missing_feature_rows": sorted(
            static_gap_hot_rows,
            key=lambda row: (
                -float(row.get("missing_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
        "top_extra_feature_rows": sorted(
            static_gap_hot_rows,
            key=lambda row: (
                -float(row.get("extra_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
        "top_zero_intersection_rows": sorted(
            [
                row
                for row in static_gap_hot_rows
                if float(row.get("source_feature_total") or 0.0) > 0.0
                and float(row.get("intersection_feature_total") or 0.0) == 0.0
            ],
            key=lambda row: (
                -float(row.get("source_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
    }
    semantic_readiness_metrics = {
        **{key: int(value) for key, value in sorted(semantic_readiness_counts.items())},
        "fully_perfect_rate": round(
            float(semantic_readiness_counts.get("fully_perfect_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "behavior_pass_static_perfect_rate": round(
            float(semantic_readiness_counts.get("behavior_pass_static_perfect_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "behavior_pass_static_gap_rate": round(
            float(semantic_readiness_counts.get("behavior_pass_static_gap_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "static_perfect_behavior_nonpass_rate": round(
            float(semantic_readiness_counts.get("static_perfect_behavior_nonpass_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "pipeline_ok_behavior_nonpass_rate": round(
            float(semantic_readiness_counts.get("pipeline_ok_behavior_nonpass_rows", 0)) / total,
            6,
        ) if total else 0.0,
    }
    benchmark_integrity_metrics = {
        "score_denominator_row_count": total,
        "row_count": total,
        "rows_excluded_from_semantic_score_denominator": 0,
        "rows_excluded_from_static_similarity_denominator": 0,
        "missing_source_features_penalized": True,
        "extra_decompiler_features_penalized": True,
        "behavior_missing_or_unsupported_rows_fail_closed": True,
        "unmapped_or_failed_rows_remain_in_denominator": True,
        "static_missing_feature_row_count": static_missing_feature_rows,
        "static_decomp_absent_feature_row_count": static_decomp_absent_feature_rows,
        "behavior_unsupported_or_ineligible_row_count": max(0, total - behavior_expected),
        "behavior_expected_but_not_executed_row_count": max(0, behavior_expected - behavior_executed),
    }
    semantic_quality_quadrant_export: dict[str, dict[str, Any]] = {}
    for quadrant, bucket in sorted(semantic_quality_quadrants.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        top_rows = sorted(
            bucket.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:10]
        semantic_quality_quadrant_export[quadrant] = {
            "row_count": row_count,
            "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(bucket.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "extra_feature_total": round(float(bucket.get("extra_feature_total", 0.0) or 0.0), 6),
            "top_rows": top_rows,
        }
    coverage_blind_spot_export = {
        kind: {
            "row_count": int(count),
            "row_rate_total_denominator": round(float(count) / total, 6) if total else 0.0,
            "top_rows": sorted(
                coverage_blind_spot_rows.get(kind) or [],
                key=lambda row: (
                    float(row.get("semantic_score_percent") or 0.0),
                    str(row.get("function_name") or ""),
                ),
            )[:8],
        }
        for kind, count in sorted(coverage_blind_spot_counts.items())
    }
    raw_pcode_compat_total = float(nir_build_stats_numeric_totals.get("raw_pcode_compat_import_count", 0.0) or 0.0)
    invalid_pcode_shape_total = float(debug_quality_evidence_totals.get("invalid_pcode_shape_count", 0.0) or 0.0)
    sleigh_blocker_row_count = len(sleigh_blocker_rows)
    sleigh_blocker_rows = sorted(
        sleigh_blocker_rows,
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:12]
    behavior_partial_progress_row_count = len(behavior_partial_progress_rows)
    behavior_partial_progress_case_pass_total = sum(
        int(row.get("case_pass_count") or 0) for row in behavior_partial_progress_rows
    )
    behavior_partial_progress_compared_case_total = sum(
        int(row.get("compared_case_count") or 0) for row in behavior_partial_progress_rows
    )
    behavior_partial_progress_case_pass_rates = [
        float(row.get("case_pass_rate") or 0.0) for row in behavior_partial_progress_rows
    ]
    behavior_partial_progress_rows = sorted(
        behavior_partial_progress_rows,
        key=lambda row: (
            -float(row.get("case_pass_count") or 0),
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:20]
    outcome_matrix_top = {
        key: {
            "row_count": int(outcome_matrix_counts.get(key, 0)),
            "lost_score_sum": round(float(outcome_matrix_lost_score.get(key, 0.0) or 0.0), 6),
            "top_rows": sorted(
                outcome_matrix_rows.get(key) or [],
                key=lambda row: (
                    float(row.get("semantic_score_percent") or 0.0),
                    str(row.get("function_name") or ""),
                ),
            )[:5],
        }
        for key, _count in sorted(
            outcome_matrix_counts.items(),
            key=lambda item: (
                float(outcome_matrix_lost_score.get(item[0], 0.0) or 0.0),
                item[1],
                item[0],
            ),
            reverse=True,
        )[:20]
    }

    roadmap_priority_export: dict[str, dict[str, Any]] = {}
    for priority in ROADMAP_PRIORITY_ORDER:
        if priority in roadmap_priority_metrics:
            roadmap_priority_export[priority] = metric_bucket_export(roadmap_priority_metrics[priority], total)
        else:
            roadmap_priority_export[priority] = metric_bucket_export({}, total)

    def top_component_gap_rows(component_rows: list[dict[str, Any]], limit: int = 12) -> list[dict[str, Any]]:
        return sorted(
            component_rows,
            key=lambda row: (
                -float(row.get("component_missing_feature_total") or 0.0),
                -float(row.get("component_extra_feature_total") or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:limit]

    type_data_gap_metrics = {
        "signature_gap_row_count": len(signature_gap_rows),
        "memory_gap_row_count": len(memory_gap_rows),
        "call_gap_row_count": len(call_gap_rows),
        "signature_gap_rows": top_component_gap_rows(signature_gap_rows),
        "memory_gap_rows": top_component_gap_rows(memory_gap_rows),
        "call_gap_rows": top_component_gap_rows(call_gap_rows),
    }
    signedness_only_signature_gap_metrics = {
        "row_count": len(signedness_only_signature_gap_rows),
        "total_pair_count": round(
            float(signedness_only_signature_gap_totals.get("param_pair_count", 0.0))
            + float(signedness_only_signature_gap_totals.get("return_pair_count", 0.0)),
            6,
        ),
        "param_pair_count": round(float(signedness_only_signature_gap_totals.get("param_pair_count", 0.0)), 6),
        "return_pair_count": round(float(signedness_only_signature_gap_totals.get("return_pair_count", 0.0)), 6),
        "source_int_param_decomp_uint_count": round(
            float(signedness_only_signature_gap_totals.get("source_int_param_decomp_uint_count", 0.0)),
            6,
        ),
        "source_uint_param_decomp_int_count": round(
            float(signedness_only_signature_gap_totals.get("source_uint_param_decomp_int_count", 0.0)),
            6,
        ),
        "source_int_return_decomp_uint_count": round(
            float(signedness_only_signature_gap_totals.get("source_int_return_decomp_uint_count", 0.0)),
            6,
        ),
        "source_uint_return_decomp_int_count": round(
            float(signedness_only_signature_gap_totals.get("source_uint_return_decomp_int_count", 0.0)),
            6,
        ),
        "top_rows": sorted(
            signedness_only_signature_gap_rows,
            key=lambda row: (
                -float(row.get("param_pair_count", 0.0) or 0.0),
                -float(row.get("return_pair_count", 0.0) or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    signature_return_pair_total = sum(signature_return_pair_counts.values())
    signature_param_pair_total = sum(signature_param_pair_counts.values())
    signature_return_mismatch_count = sum(
        count
        for pair, count in signature_return_pair_counts.items()
        if pair.split("->", 1)[0] != pair.split("->", 1)[1]
    )
    signature_param_mismatch_count = sum(
        count
        for pair, count in signature_param_pair_counts.items()
        if pair.split("->", 1)[0] != pair.split("->", 1)[1]
    )
    signature_kind_confusion_metrics = {
        "return_pair_count": signature_return_pair_total,
        "return_mismatch_count": signature_return_mismatch_count,
        "return_match_rate": round(
            (signature_return_pair_total - signature_return_mismatch_count) / signature_return_pair_total,
            6,
        )
        if signature_return_pair_total
        else 0.0,
        "param_pair_count": signature_param_pair_total,
        "param_mismatch_count": signature_param_mismatch_count,
        "param_match_rate": round(
            (signature_param_pair_total - signature_param_mismatch_count) / signature_param_pair_total,
            6,
        )
        if signature_param_pair_total
        else 0.0,
        "param_arity_mismatch_row_count": len(signature_param_arity_mismatch_rows),
        "return_pair_counts": dict(sorted(signature_return_pair_counts.items())),
        "param_pair_counts": dict(sorted(signature_param_pair_counts.items())),
        "top_signature_pair_gap_rows": sorted(
            signature_pair_gap_rows,
            key=lambda row: (
                -int(bool(row.get("return_mismatch"))),
                -int(row.get("param_mismatch_count") or 0),
                -int(row.get("missing_param_count") or 0),
                -int(row.get("extra_param_count") or 0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
        "top_param_arity_mismatch_rows": sorted(
            signature_param_arity_mismatch_rows,
            key=lambda row: (
                -int(row.get("missing_param_count") or 0),
                -int(row.get("extra_param_count") or 0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    structuring_gap_metrics = {
        "control_flow_gap_row_count": len(control_flow_gap_rows),
        "hard_nonperfect_row_count": len(hard_function_rows),
        "control_flow_gap_rows": top_component_gap_rows(control_flow_gap_rows),
        "hard_nonperfect_rows": hard_function_rows[:12],
    }
    fid_name_recovery_metrics = {
        "name_or_mapping_gap_row_count": len(name_recovery_rows),
        "top_name_or_mapping_gap_rows": sorted(
            name_recovery_rows,
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    architecture_support_metrics = {
        arch: metric_bucket_export(metrics, total)
        for arch, metrics in sorted(architecture_stage_metrics.items())
    }
    return {
        "manifest": manifest_name,
        "entry_count": len(entries),
        "row_count": total,
        "function_mapping_rate": round(mapped / total, 6) if total else 0.0,
        "decomp_success_rate": round(decomp_ok / total, 6) if total else 0.0,
        "candidate_compile_rate": round(compile_ok / total, 6) if total else 0.0,
        "behavior_pass_rate": round(behavior_pass / total, 6) if total else 0.0,
        "effective_coverage": {
            "mapped_rows": mapped,
            "mapped_rate": round(mapped / total, 6) if total else 0.0,
            "decompiled_rows": decomp_ok,
            "decompiled_rate": round(decomp_ok / total, 6) if total else 0.0,
            "behavior_expected_rows": behavior_expected,
            "behavior_expected_rate": round(behavior_expected / total, 6) if total else 0.0,
            "behavior_executed_rows": behavior_executed,
            "behavior_executed_rate": round(behavior_executed / total, 6) if total else 0.0,
        },
        "behavior_eligibility": {
            "eligible_rows": behavior_expected,
            "eligible_rate": round(behavior_expected / total, 6) if total else 0.0,
            "executed_rows": behavior_executed,
            "execution_rate": round(behavior_executed / behavior_expected, 6) if behavior_expected else 0.0,
            "pass_rate_eligible_denominator": round(behavior_pass / behavior_expected, 6) if behavior_expected else 0.0,
            "pass_rate_total_denominator": round(behavior_pass / total, 6) if total else 0.0,
        },
        "weighted_semantic_similarity": weighted_semantic_similarity,
        "weighted_semantic_similarity_percent": percent(weighted_semantic_similarity),
        "scoring_contract": {
            "semantic_score_denominator": "all manifest rows",
            "semantic_score_formula": "0.65 * behavior_score + 0.35 * static_multiset_jaccard",
            "behavior_score_policy": "pass=1.0; mismatch, unsupported, decomp failure, compile/run failure, and missing output=0.0",
            "static_similarity_policy": "multiset Jaccard over source and decompiler feature union; missing and extra features are included in the denominator",
            "unmapped_or_failed_row_policy": "row remains in denominator with zero semantic score unless another component earns credit",
        },
        "semantic_score_stats": {
            **numeric_distribution(score_values),
            "nonzero_count": nonzero_score_count,
            "nonzero_rate": round(
                nonzero_score_count / total,
                6,
            ) if total else 0.0,
        },
        "score_component_metrics": {
            "row_count": total,
            "behavior_weight": 0.65,
            "static_weight": 0.35,
            "behavior_score_sum": behavior_score_sum,
            "static_score_sum": static_score_sum,
            "behavior_component_score_sum": behavior_component_score_sum,
            "static_component_score_sum": static_component_score_sum,
            "weighted_score_sum": score_sum,
            "behavior_component_possible_score_sum": round(0.65 * total, 6),
            "static_component_possible_score_sum": round(0.35 * total, 6),
            "behavior_component_lost_score_sum": round((0.65 * total) - behavior_component_score_sum, 6),
            "static_component_lost_score_sum": round((0.35 * total) - static_component_score_sum, 6),
            "behavior_score_distribution": numeric_distribution(behavior_score_values),
            "static_score_distribution": numeric_distribution(static_score_values),
        },
        "score_weight_sensitivity_metrics": {
            "current_behavior_weight": 0.65,
            "current_static_weight": 0.35,
            "current_weighted_score": current_weighted_score,
            "current_weighted_score_percent": percent(current_weighted_score),
            "scenario_scores": weight_scenarios,
            "behavior_minus_static_score_distribution": numeric_distribution(behavior_static_score_delta_values),
            "behavior_greater_than_static_row_count": sum(1 for value in behavior_static_score_delta_values if value > 0.0),
            "static_greater_than_behavior_row_count": sum(1 for value in behavior_static_score_delta_values if value < 0.0),
            "behavior_static_equal_row_count": sum(1 for value in behavior_static_score_delta_values if value == 0.0),
        },
        "component_loss_hot_row_metrics": {
            "top_total_component_loss_rows": component_loss_hot_rows,
            "top_behavior_component_loss_rows": sorted(
                component_loss_hot_rows,
                key=lambda row: (
                    float(row.get("behavior_component_loss") or 0.0),
                    float(row.get("total_component_loss") or 0.0),
                    str(row.get("function_name") or ""),
                ),
                reverse=True,
            )[:12],
            "top_static_component_loss_rows": sorted(
                component_loss_hot_rows,
                key=lambda row: (
                    float(row.get("static_component_loss") or 0.0),
                    float(row.get("total_component_loss") or 0.0),
                    str(row.get("function_name") or ""),
                ),
                reverse=True,
            )[:12],
        },
        "score_denominator_metrics": {
            "score_denominator_row_count": total,
            "score_denominator_policy": "all_rows_including_unmapped_unsupported_and_failed",
            "score_sum": score_sum,
            "possible_score_sum": float(total),
            "lost_score_sum": round(float(total) - score_sum, 6),
            "zero_score_row_count": zero_score_count,
            "nonzero_score_row_count": nonzero_score_count,
            "perfect_score_row_count": perfect_score_count,
            "unmapped_row_count": max(0, total - mapped),
            "decomp_failed_or_unmapped_row_count": max(0, total - decomp_ok),
            "behavior_not_pass_row_count": max(0, total - behavior_pass),
        },
        "semantic_loss_metrics": {
            "total_lost_score": round(float(total) - score_sum, 6),
            "avg_lost_score_per_row": round((float(total) - score_sum) / total, 6) if total else 0.0,
            "lost_score_by_behavior_status": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_behavior_status.items())
            },
            "lost_score_by_stage_first_failure": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_stage_first_failure.items())
            },
            "lost_score_by_zero_credit_reason": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_zero_credit_reason.items())
            },
            "top_lost_score_rows": semantic_loss_hot_rows,
        },
        "semantic_readiness_metrics": semantic_readiness_metrics,
        "benchmark_integrity_metrics": benchmark_integrity_metrics,
        "improvement_axis_metrics": improvement_axis_export,
        "focus_area_metrics": focus_area_export,
        "roadmap_priority_metrics": {
            "priority_order": ROADMAP_PRIORITY_ORDER,
            "buckets": roadmap_priority_export,
        },
        "type_data_gap_metrics": type_data_gap_metrics,
        "signedness_only_signature_gap_metrics": signedness_only_signature_gap_metrics,
        "signature_kind_confusion_metrics": signature_kind_confusion_metrics,
        "structuring_gap_metrics": structuring_gap_metrics,
        "fid_name_recovery_metrics": fid_name_recovery_metrics,
        "architecture_support_metrics": architecture_support_metrics,
        "complexity_quality_metrics": {
            "source_feature_bucket_policy": "tiny<=5, small<=15, medium<=40, large>40 source static features",
            "by_source_feature_bucket": complexity_export,
            "hard_nonperfect_rows": hard_function_rows,
        },
        "stage_cost_correlation_metrics": {
            "decompile_wall_by_behavior_status": {
                status: numeric_distribution(values)
                for status, values in sorted(cost_values_by_behavior_status.items())
            },
            "decompile_wall_by_stage_first_failure": {
                stage: numeric_distribution(values)
                for stage, values in sorted(cost_values_by_stage_first_failure.items())
            },
            "decompile_wall_by_score_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(cost_values_by_score_bucket.items())
            },
            "score_by_decompile_cost_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(scores_by_cost_bucket.items())
            },
            "lost_score_by_decompile_cost_bucket": {
                bucket: round(float(value), 6)
                for bucket, value in sorted(lost_score_by_cost_bucket.items())
            },
        },
        "admission_gate_metrics": admission_gate_metrics,
        "quality_gate_funnel_metrics": {
            "gate_order": gate_order,
            "counts": {key: int(admission_gate_counts.get(key, 0)) for key in gate_order},
            "drop_rows_from_previous_gate": gate_drop_rows,
            "retention_rate_from_previous_gate": gate_retention_rates,
            "rates_total_denominator": {
                key: round(float(admission_gate_counts.get(key, 0)) / total, 6) if total else 0.0
                for key in gate_order
            },
        },
        "stage_transition_metrics": stage_transition_metrics,
        "sleigh_lift_health_metrics": {
            "mapped_rows": mapped,
            "debug_stage_status_rows": debug_stage_status_row_count,
            "decode_ok_rows": int(admission_gate_counts.get("decode_ok_rows", 0)),
            "raw_pcode_ok_rows": int(admission_gate_counts.get("raw_pcode_ok_rows", 0)),
            "decode_ok_rate_mapped_denominator": round(
                float(admission_gate_counts.get("decode_ok_rows", 0)) / mapped,
                6,
            ) if mapped else 0.0,
            "raw_pcode_ok_rate_mapped_denominator": round(
                float(admission_gate_counts.get("raw_pcode_ok_rows", 0)) / mapped,
                6,
            ) if mapped else 0.0,
            "template_source_totals": dict(sorted(debug_template_source_totals.items())),
            "raw_pcode_compat_import_total": raw_pcode_compat_total,
            "invalid_pcode_shape_total": invalid_pcode_shape_total,
            "sleigh_first_blocker_row_count": sleigh_blocker_row_count,
            "sleigh_first_blocker_lost_score": round(
                sum(
                    float(value)
                    for key, value in stage_first_blocker_lost_score.items()
                    if key.startswith("decode:") or key.startswith("raw_pcode:")
                ),
                6,
            ),
            "top_sleigh_blocker_rows": sleigh_blocker_rows,
        },
        "behavior_failure_diagnostics": {
            "owner_counts": dict(sorted(behavior_failure_owner_counts.items())),
            "detail_signature_counts": dict(behavior_failure_detail_counts.most_common(20)),
            "top_detail_rows": behavior_failure_detail_top_rows,
        },
        "semantic_quality_quadrant_metrics": semantic_quality_quadrant_export,
        "outcome_matrix_metrics": {
            "top_outcomes_by_lost_score": outcome_matrix_top,
            "outcome_count": len(outcome_matrix_counts),
        },
        "coverage_blind_spot_metrics": {
            "counts": dict(sorted(coverage_blind_spot_counts.items())),
            "details": coverage_blind_spot_export,
        },
        "static_gap_density_metrics": {
            "missing_density_distribution": numeric_distribution(static_missing_density_values),
            "extra_density_distribution": numeric_distribution(static_extra_density_values),
            "static_score_by_missing_gap_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(static_score_by_missing_gap_bucket.items())
            },
            "gap_bucket_rows": static_gap_density_export,
        },
        "static_gap_hot_row_metrics": static_gap_hot_row_metrics,
        "perfect_row_count": perfect_score_count,
        "supported_behavior_row_count": sum(
            1 for row in rows if row.get("behavior", {}).get("status") != "unsupported_signature"
        ),
        "mapping_status_counts": dict(sorted(mapping_status_counts.items())),
        "decomp_failure_counts": dict(sorted(decomp_failure_counts.items())),
        "behavior_status_counts": dict(sorted(behavior_status_counts.items())),
        "decomp_cache_status_counts": dict(sorted(decomp_cache_status_counts.items())),
        "behavior_cache_status_counts": dict(sorted(behavior_cache_status_counts.items())),
        "zero_credit_breakdown": dict(sorted(zero_credit_breakdown.items())),
        "score_distribution": dict(sorted(score_distribution.items())),
        "stage_first_failure_counts": dict(sorted(stage_first_failure_counts.items())),
        "static_similarity_component_averages": {
            component: round(static_component_sums[component] / total, 6) if total else 0.0
            for component in sorted(STATIC_SIMILARITY_COMPONENTS)
        },
        "static_similarity_component_average_percent": {
            component: percent(round(static_component_sums[component] / total, 6) if total else 0.0)
            for component in sorted(STATIC_SIMILARITY_COMPONENTS)
        },
        "static_similarity_gap_totals": static_gap_summary,
        "static_similarity_gap_component_totals": static_gap_component_summary,
        "static_similarity_gap_component_top_features": static_gap_component_top_summary,
        "static_component_precision_recall_metrics": static_component_precision_recall_summary,
        "behavior_case_metrics": {
            "case_count": behavior_case_total,
            "compared_case_count": behavior_compared_case_total,
            "case_pass_count": behavior_case_pass_total,
            "case_fail_count": behavior_case_fail_total,
            "case_pass_rate": round(behavior_case_pass_total / behavior_compared_case_total, 6)
            if behavior_compared_case_total
            else 0.0,
            "partial_mismatch_row_count": partial_mismatch_rows,
            "partial_progress_row_count": partial_progress_rows,
        },
        "behavior_timeout_progress_metrics": {
            "partial_timeout_row_count": len(partial_timeout_rows),
            "partial_timeout_case_pass_count": partial_timeout_case_pass_total,
            "partial_timeout_compared_case_count": partial_timeout_compared_case_total,
            "partial_timeout_case_pass_rate": round(
                partial_timeout_case_pass_total / partial_timeout_compared_case_total,
                6,
            )
            if partial_timeout_compared_case_total
            else 0.0,
            "partial_timeout_missing_candidate_line_total": partial_timeout_missing_line_total,
        },
        "behavior_partial_progress_metrics": {
            "row_count": behavior_partial_progress_row_count,
            "case_pass_count": behavior_partial_progress_case_pass_total,
            "compared_case_count": behavior_partial_progress_compared_case_total,
            "case_pass_rate_distribution": numeric_distribution(behavior_partial_progress_case_pass_rates),
            "top_rows": behavior_partial_progress_rows,
        },
        "behavior_support_metrics": {
            "case_source_counts": dict(sorted(behavior_case_source_counts.items())),
            "unsupported_reason_counts": dict(sorted(behavior_unsupported_reason_counts.items())),
            "unsupported_signature_row_count": int(behavior_status_counts.get("unsupported_signature", 0)),
            "eligible_row_count": behavior_expected,
            "executed_row_count": behavior_executed,
        },
        "behavior_denominator_metrics": {
            "row_denominator_count": total,
            "eligible_row_count": behavior_expected,
            "eligible_row_rate": round(behavior_expected / total, 6) if total else 0.0,
            "executed_row_count": behavior_executed,
            "executed_row_rate_total_denominator": round(behavior_executed / total, 6) if total else 0.0,
            "executed_row_rate_eligible_denominator": round(behavior_executed / behavior_expected, 6)
            if behavior_expected
            else 0.0,
            "pass_row_count": behavior_pass,
            "pass_row_rate_total_denominator": round(behavior_pass / total, 6) if total else 0.0,
            "pass_row_rate_eligible_denominator": round(behavior_pass / behavior_expected, 6)
            if behavior_expected
            else 0.0,
            "pass_row_rate_executed_denominator": round(behavior_pass / behavior_executed, 6)
            if behavior_executed
            else 0.0,
            "not_executed_row_count": max(0, behavior_expected - behavior_executed),
            "unsupported_or_ineligible_row_count": max(0, total - behavior_expected),
            "case_denominator_count": behavior_compared_case_total,
            "case_pass_count": behavior_case_pass_total,
            "case_fail_count": behavior_case_fail_total,
            "case_pass_rate": round(behavior_case_pass_total / behavior_compared_case_total, 6)
            if behavior_compared_case_total
            else 0.0,
        },
        "behavior_mismatch_metrics": {
            "mismatch_row_count": int(behavior_status_counts.get("mismatch", 0)),
            "first_mismatch_index_counts": dict(sorted(behavior_first_mismatch_index_counts.items())),
            "output_length_delta_counts": dict(sorted(behavior_output_length_delta_counts.items())),
            "mismatch_kind_counts": dict(sorted(behavior_mismatch_kind_counts.items())),
        },
        "behavior_distance_metrics": {
            "case_pass_rate_distribution": numeric_distribution(behavior_case_pass_rates),
            "missing_candidate_line_total": behavior_missing_candidate_line_total,
            "extra_candidate_line_total": behavior_extra_candidate_line_total,
            "output_length_delta_counts": dict(sorted(behavior_output_length_delta_counts.items())),
            "first_mismatch_index_counts": dict(sorted(behavior_first_mismatch_index_counts.items())),
        },
        "denominator_accounting_metrics": {
            "row_count": total,
            "mapped_row_count": mapped,
            "unmapped_row_count": max(0, total - mapped),
            "decompiled_row_count": decomp_ok,
            "mapped_but_not_decompiled_row_count": max(0, mapped - decomp_ok),
            "behavior_expected_row_count": behavior_expected,
            "behavior_not_expected_row_count": max(0, total - behavior_expected),
            "behavior_executed_row_count": behavior_executed,
            "behavior_expected_but_not_executed_row_count": max(0, behavior_expected - behavior_executed),
            "behavior_pass_row_count": behavior_pass,
            "behavior_nonpass_row_count": max(0, total - behavior_pass),
            "static_missing_feature_row_count": static_missing_feature_rows,
            "zero_score_row_count": int(score_distribution.get("zero", 0)),
            "nonzero_score_row_count": nonzero_score_count,
            "perfect_score_row_count": perfect_score_count,
            "semantic_score_denominator_row_count": total,
            "semantic_score_zero_fill_row_count": zero_score_count,
        },
        "score_by_behavior_status": score_by_behavior_status,
        "score_by_stage_first_failure": score_by_stage_first_failure,
        "behavior_status_by_stage_first_failure": {
            stage: dict(sorted(counts.items()))
            for stage, counts in sorted(behavior_status_by_stage_first_failure.items())
        },
        "behavior_status_by_zero_credit_reason": {
            reason: dict(sorted(counts.items()))
            for reason, counts in sorted(behavior_status_by_zero_credit_reason.items())
        },
        "static_gap_row_metrics": {
            "source_feature_row_count": source_feature_rows,
            "decomp_feature_row_count": decomp_feature_rows,
            "decomp_feature_row_rate": round(decomp_feature_rows / total, 6) if total else 0.0,
            "decomp_absent_feature_row_count": static_decomp_absent_feature_rows,
            "decomp_absent_feature_row_rate": round(static_decomp_absent_feature_rows / total, 6) if total else 0.0,
            "missing_feature_row_count": static_missing_feature_rows,
            "missing_feature_row_rate": round(static_missing_feature_rows / total, 6) if total else 0.0,
            "extra_feature_row_count": static_extra_feature_rows,
            "extra_feature_row_rate": round(static_extra_feature_rows / total, 6) if total else 0.0,
            "zero_static_intersection_row_count": static_zero_similarity_rows,
            "zero_static_intersection_row_rate": round(static_zero_similarity_rows / total, 6) if total else 0.0,
            "missing_feature_count_distribution": numeric_distribution(missing_feature_count_values),
            "extra_feature_count_distribution": numeric_distribution(extra_feature_count_values),
            "component_missing_row_counts": dict(sorted(static_component_missing_row_counts.items())),
            "component_zero_similarity_row_counts": dict(sorted(static_component_zero_similarity_row_counts.items())),
        },
        "source_feature_metrics": {
            "source_feature_total_distribution": numeric_distribution(source_feature_total_values),
            "source_feature_total_direct_distribution": numeric_distribution(source_feature_total_direct_values),
            "source_feature_total_inline_expanded_distribution": numeric_distribution(
                source_feature_total_inline_expanded_values
            ),
            "decomp_feature_total_distribution": numeric_distribution(decomp_feature_total_values),
            "intersection_feature_total_distribution": numeric_distribution(static_intersection_feature_total_values),
            "union_feature_total_distribution": numeric_distribution(static_union_feature_total_values),
            "component_source_feature_distributions": {
                component: numeric_distribution(values)
                for component, values in sorted(static_component_source_feature_values.items())
            },
            "component_decomp_feature_distributions": {
                component: numeric_distribution(values)
                for component, values in sorted(static_component_decomp_feature_values.items())
            },
        },
        "static_source_variant_metrics": {
            "variant_counts": dict(sorted(static_source_variant_counts.items())),
            "inline_expanded_static_score_delta_distribution": numeric_distribution(
                inline_expanded_static_score_deltas
            ),
            "top_inline_expanded_static_rows": inline_expanded_static_hot_rows,
        },
        "static_absence_penalty_metrics": {
            "source_feature_total": static_source_total,
            "decomp_feature_total": static_decomp_total,
            "intersection_feature_total": static_intersection_total,
            "union_feature_total": static_union_total,
            "missing_feature_total": static_missing_total,
            "extra_feature_total": static_extra_total,
            "source_recall": round(static_intersection_total / static_source_total, 6)
            if static_source_total
            else 0.0,
            "decomp_precision": round(static_intersection_total / static_decomp_total, 6)
            if static_decomp_total
            else 0.0,
            "union_jaccard": round(static_intersection_total / static_union_total, 6)
            if static_union_total
            else 1.0,
            "missing_feature_rate": round(static_missing_total / static_source_total, 6)
            if static_source_total
            else 0.0,
            "extra_feature_rate": round(static_extra_total / static_decomp_total, 6)
            if static_decomp_total
            else 0.0,
            "rows_with_source_features": source_feature_rows,
            "rows_with_decomp_features": decomp_feature_rows,
            "rows_with_no_decomp_features_despite_source": static_decomp_absent_feature_rows,
            "rows_with_missing_features": static_missing_feature_rows,
            "rows_with_zero_static_intersection": static_zero_similarity_rows,
        },
        "static_component_absence_matrix_metrics": static_component_absence_export,
        "source_decomp_size_metrics": {
            "source_body_line_count_distribution": numeric_distribution(source_body_line_counts),
            "decomp_line_count_distribution": numeric_distribution(decomp_line_counts),
            "source_body_byte_count_distribution": numeric_distribution(source_body_byte_counts),
            "decomp_byte_count_distribution": numeric_distribution(decomp_byte_counts),
            "decomp_to_source_line_ratio_distribution": numeric_distribution(decomp_to_source_line_ratios),
            "decomp_to_source_byte_ratio_distribution": numeric_distribution(decomp_to_source_byte_ratios),
            "top_decomp_to_source_line_ratio_rows": source_decomp_size_hot_rows,
        },
        "harness_cost_metrics": {
            "decompile_total_sec": round(sum(decomp_times), 6),
            "decompile_avg_sec": round(sum(decomp_times) / len(decomp_times), 6) if decomp_times else 0.0,
            "decompile_p50_sec": numeric_distribution(decomp_times)["p50"],
            "decompile_p90_sec": numeric_distribution(decomp_times)["p90"],
            "decompile_p95_sec": numeric_distribution(decomp_times)["p95"],
            "decompile_max_sec": numeric_distribution(decomp_times)["max"],
            "behavior_compile_total_sec": round(sum(behavior_compile_times), 6),
            "behavior_compile_avg_sec": round(sum(behavior_compile_times) / len(behavior_compile_times), 6)
            if behavior_compile_times
            else 0.0,
            "behavior_compile_p50_sec": numeric_distribution(behavior_compile_times)["p50"],
            "behavior_compile_p90_sec": numeric_distribution(behavior_compile_times)["p90"],
            "behavior_compile_p95_sec": numeric_distribution(behavior_compile_times)["p95"],
            "behavior_compile_max_sec": numeric_distribution(behavior_compile_times)["max"],
            "behavior_run_total_sec": round(sum(behavior_run_times), 6),
            "behavior_run_avg_sec": round(sum(behavior_run_times) / len(behavior_run_times), 6)
            if behavior_run_times
            else 0.0,
            "behavior_run_p50_sec": numeric_distribution(behavior_run_times)["p50"],
            "behavior_run_p90_sec": numeric_distribution(behavior_run_times)["p90"],
            "behavior_run_p95_sec": numeric_distribution(behavior_run_times)["p95"],
            "behavior_run_max_sec": numeric_distribution(behavior_run_times)["max"],
            "behavior_wall_total_sec": round(sum(behavior_wall_times), 6),
            "behavior_wall_avg_sec": round(sum(behavior_wall_times) / len(behavior_wall_times), 6)
            if behavior_wall_times
            else 0.0,
            "behavior_wall_p50_sec": numeric_distribution(behavior_wall_times)["p50"],
            "behavior_wall_p90_sec": numeric_distribution(behavior_wall_times)["p90"],
            "behavior_wall_p95_sec": numeric_distribution(behavior_wall_times)["p95"],
            "behavior_wall_max_sec": numeric_distribution(behavior_wall_times)["max"],
        },
        "cost_hot_rows": {
            "top_decompile_wall_rows": cost_hot_rows_by_decompile,
            "top_behavior_wall_rows": cost_hot_rows_by_behavior_wall,
        },
        "debug_coverage_metrics": {
            "debug_decomp_rows": debug_decomp_row_count,
            "debug_decomp_rate_mapped_denominator": round(debug_decomp_row_count / mapped_debug_denominator, 6)
            if mapped
            else 0.0,
            "debug_stage_status_rows": debug_stage_status_row_count,
            "debug_stage_status_rate_mapped_denominator": round(
                debug_stage_status_row_count / mapped_debug_denominator,
                6,
            ) if mapped else 0.0,
        },
        "pipeline_stage_metrics": pipeline_stage_metrics,
        "debug_pipeline_numeric_metrics": {
            key: numeric_distribution(values)
            for key, values in sorted(debug_pipeline_numeric_values.items())
        },
        "nir_build_stats_metrics": {
            "stats_row_count": nir_build_stats_row_count,
            "stats_row_rate_mapped_denominator": round(nir_build_stats_row_count / mapped_debug_denominator, 6)
            if mapped
            else 0.0,
            "numeric_totals": dict(sorted(nir_build_stats_numeric_totals.items())),
            "nonzero_row_counts": dict(sorted(nir_build_stats_nonzero_rows.items())),
            "debt_metric_totals": nir_debt_totals,
            "debt_metric_distributions": nir_build_stats_distributions,
            "top_debt_rows": nir_build_stats_debt_hot_rows,
        },
        "nir_debt_correlation_metrics": {
            "stats_row_count": nir_build_stats_row_count,
            "debt_row_count": nir_debt_row_count,
            "debt_row_rate_stats_denominator": round(nir_debt_row_count / nir_build_stats_row_count, 6)
            if nir_build_stats_row_count
            else 0.0,
            "score_distribution_debt_rows": numeric_distribution(nir_debt_score_values),
            "score_distribution_no_debt_rows": numeric_distribution(nir_no_debt_score_values),
            "behavior_status_counts_debt_rows": dict(sorted(nir_debt_behavior_status_counts.items())),
            "stage_first_failure_counts_debt_rows": dict(sorted(nir_debt_stage_first_failure_counts.items())),
        },
        "debug_owner_bucket_counts": dict(sorted(debug_owner_bucket_counts.items())),
        "debug_stage_status_counts": dict(sorted(debug_stage_status_counts.items())),
        "debug_stage_status_matrix": debug_stage_status_matrix_export,
        "debug_quality_evidence_totals": dict(sorted(debug_quality_evidence_totals.items())),
        "debug_quality_evidence_nonzero_rows": dict(sorted(debug_quality_evidence_nonzero_rows.items())),
        "debug_template_source_totals": dict(sorted(debug_template_source_totals.items())),
        "triage_priority_rows": triage_priority_rows,
        "host_execution_unavailable_count": sum(host_statuses.values()),
        "host_execution_unavailable_reasons": dict(host_statuses),
        "by_language": by_language,
        "by_arch": by_arch,
        "by_source_return_kind": by_source_return_kind,
        "by_source_param_shape": by_source_param_shape,
        "by_tag": by_tag,
        "by_entry": by_entry,
        "readability_score_stats": numeric_distribution(readability_score_values),
        "semantic_correctness_score_stats": numeric_distribution(semantic_correctness_score_values),
        "compiler_option_matrix": compiler_opt_matrix_data,
        "ai_remedy_summary": ai_remedy_summary,
    }




def snapshot_baseline_artifacts(
    output_dir: Path,
    baseline_summary_path: Path,
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    comparison: dict[str, Any],
) -> dict[str, Any]:
    snapshot_dir = output_dir / "baseline_snapshot"
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    summary_snapshot_path = snapshot_dir / "source_semantic_summary.json"
    rows_snapshot_path = snapshot_dir / "source_semantic_rows.json"
    comparison_snapshot_path = snapshot_dir / "source_semantic_comparison.json"
    manifest_path = snapshot_dir / "snapshot.json"
    summary_snapshot_path.write_text(dump_json_pretty(baseline_summary), encoding="utf-8")
    rows_snapshot_path.write_text(dump_json_pretty(baseline_rows), encoding="utf-8")
    comparison_snapshot_path.write_text(dump_json_pretty(comparison), encoding="utf-8")
    manifest = {
        "format": "source-semantic-baseline-snapshot-v1",
        "created_at_utc": utc_isoformat(utc_now()),
        "baseline_summary_path": rel(baseline_summary_path),
        "baseline_artifact_dir": rel(baseline_summary_path.parent),
        "summary_snapshot_path": rel(summary_snapshot_path),
        "rows_snapshot_path": rel(rows_snapshot_path),
        "comparison_snapshot_path": rel(comparison_snapshot_path),
    }
    manifest_path.write_text(dump_json_pretty(manifest), encoding="utf-8")
    manifest["snapshot_manifest_path"] = rel(manifest_path)
    return manifest




def append_history_record(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    comparison = summary.get("comparison") if isinstance(summary.get("comparison"), dict) else {}
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
        if isinstance(comparison, dict)
        else None
    )
    record = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "manifest": summary.get("manifest"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "comparison_outcome": summary.get("comparison_outcome"),
        "behavior_pass_rate": summary.get("behavior_pass_rate"),
        "candidate_compile_rate": summary.get("candidate_compile_rate"),
        "decomp_success_rate": summary.get("decomp_success_rate"),
        "baseline_summary_path": comparison.get("baseline_summary_path") if isinstance(comparison, dict) else None,
        "decomp_cache_hit_count": summary.get("decomp_cache_hit_count"),
        "decomp_cache_miss_count": summary.get("decomp_cache_miss_count"),
        "list_cache_hit_count": summary.get("list_cache_hit_count"),
        "list_cache_miss_count": summary.get("list_cache_miss_count"),
        "wall_sec": summary.get("wall_sec"),
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(dump_json_line(record))




def update_latest_index(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        index = load_json(path) if path.exists() else {}
    except Exception:
        index = {}
    if not isinstance(index, dict):
        index = {}
    manifest = str(summary.get("manifest") or "unknown")
    index[manifest] = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "summary_path": str(Path(str(summary.get("artifact_dir") or "")) / "source_semantic_summary.json"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "comparison_outcome": summary.get("comparison_outcome"),
        "decomp_cache_file": summary.get("decomp_cache_file"),
        "list_cache_file": summary.get("list_cache_file"),
        "history_file": summary.get("history_file"),
    }
    path.write_text(dump_json_pretty(index), encoding="utf-8")




def load_history_records(path: Path, manifest_name: str, limit: int = 12) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: list[dict[str, Any]] = []
    try:
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(record, dict) and record.get("manifest") == manifest_name:
                    records.append(record)
    except OSError:
        return []
    return records[-limit:]




def history_snapshot(path: Path, summary: dict[str, Any]) -> dict[str, Any] | None:
    records = load_history_records(path, str(summary.get("manifest") or ""))
    if not records:
        return None
    same_shape_records = [record for record in records if record.get("row_count") == summary.get("row_count")]
    comparison_record = same_shape_records[-1] if same_shape_records else records[-1]
    latest_record = records[-1]
    current_similarity = summary.get("weighted_semantic_similarity_percent")
    comparison_similarity = comparison_record.get("weighted_semantic_similarity_percent")
    latest_similarity = latest_record.get("weighted_semantic_similarity_percent")
    comparable_shape = comparison_record.get("row_count") == summary.get("row_count")
    comparison_delta = (
        round(float(current_similarity) - float(comparison_similarity), 6)
        if comparable_shape
        and isinstance(current_similarity, (int, float))
        and isinstance(comparison_similarity, (int, float))
        else None
    )
    latest_delta = (
        round(float(current_similarity) - float(latest_similarity), 6)
        if latest_record.get("row_count") == summary.get("row_count")
        and isinstance(current_similarity, (int, float))
        and isinstance(latest_similarity, (int, float))
        else None
    )
    return {
        "history_file": rel(path),
        "previous_run_count": len(records),
        "latest_previous_run": latest_record,
        "comparison_previous_run": comparison_record,
        "comparison_shape_matches": comparable_shape,
        "latest_shape_matches": latest_record.get("row_count") == summary.get("row_count"),
        "weighted_semantic_similarity_percent_delta_vs_comparison": comparison_delta,
        "weighted_semantic_similarity_percent_delta_vs_latest": latest_delta,
        "recent_runs": records,
    }




def render_markdown(summary: dict[str, Any], rows: list[dict[str, Any]]) -> str:
    lines = [
        f"# Source Semantic Benchmark: {summary['manifest']}",
        "",
        f"- Run ID: `{summary.get('run_id', 'unknown')}`",
        f"- Artifact dir: `{summary.get('artifact_dir', 'unknown')}`",
        f"- Entries: {summary['entry_count']}",
        f"- Rows: {summary['row_count']}",
        f"- Function mapping rate: {summary['function_mapping_rate']:.3f}",
        f"- Decompile success rate: {summary['decomp_success_rate']:.3f}",
        f"- Candidate compile rate: {summary['candidate_compile_rate']:.3f}",
        f"- Behavior pass rate: {summary['behavior_pass_rate']:.3f}",
        f"- Weighted semantic similarity: {summary['weighted_semantic_similarity_percent']:.3f}%",
        f"- Perfect rows: {summary['perfect_row_count']}",
        f"- Supported behavior rows: {summary['supported_behavior_row_count']}",
        f"- Host execution unavailable rows: {summary['host_execution_unavailable_count']}",
    ]
    effective = summary.get("effective_coverage") if isinstance(summary.get("effective_coverage"), dict) else {}
    behavior_eligibility = (
        summary.get("behavior_eligibility") if isinstance(summary.get("behavior_eligibility"), dict) else {}
    )
    if effective:
        lines.append(
            "- Effective coverage: "
            f"mapped {effective.get('mapped_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('mapped_rate', 0.0) or 0.0):.3f}), "
            f"decompiled {effective.get('decompiled_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('decompiled_rate', 0.0) or 0.0):.3f}), "
            f"behavior executed {effective.get('behavior_executed_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('behavior_executed_rate', 0.0) or 0.0):.3f})"
        )
    if behavior_eligibility:
        lines.append(
            "- Behavior eligibility: "
            f"eligible {behavior_eligibility.get('eligible_rows', 0)}, "
            f"execution rate {float(behavior_eligibility.get('execution_rate', 0.0) or 0.0):.3f}, "
            f"pass/eligible {float(behavior_eligibility.get('pass_rate_eligible_denominator', 0.0) or 0.0):.3f}, "
            f"pass/total {float(behavior_eligibility.get('pass_rate_total_denominator', 0.0) or 0.0):.3f}"
        )
    if "wall_sec" in summary:
        lines.append(f"- Wall time: {summary['wall_sec']:.3f}s")
    if summary.get("decomp_cache_file"):
        lines.append(f"- Decomp cache: `{summary['decomp_cache_file']}`")
        lines.append(
            f"- Decomp cache hits/misses: {summary.get('decomp_cache_hit_count', 0)}/"
            f"{summary.get('decomp_cache_miss_count', 0)}"
        )
    if summary.get("list_cache_file"):
        lines.append(f"- List cache: `{summary['list_cache_file']}`")
        lines.append(
            f"- List cache hits/misses: {summary.get('list_cache_hit_count', 0)}/"
            f"{summary.get('list_cache_miss_count', 0)}"
        )
    cache_efficiency = summary.get("cache_efficiency_metrics")
    if isinstance(cache_efficiency, dict):
        lines.append(
            "- Cache hit rates: "
            f"decomp {float(cache_efficiency.get('decomp_cache_hit_rate', 0.0) or 0.0):.3f}, "
            f"list {float(cache_efficiency.get('list_cache_hit_rate', 0.0) or 0.0):.3f}, "
            f"behavior {float(cache_efficiency.get('behavior_cache_hit_rate', 0.0) or 0.0):.3f}"
        )
    if summary.get("history_file"):
        lines.append(f"- History: `{summary['history_file']}`")
    if summary.get("latest_index_file"):
        lines.append(f"- Latest index: `{summary['latest_index_file']}`")
    history = summary.get("history")
    if isinstance(history, dict):
        comparison_record = (
            history.get("comparison_previous_run")
            if isinstance(history.get("comparison_previous_run"), dict)
            else {}
        )
        latest_record = (
            history.get("latest_previous_run")
            if isinstance(history.get("latest_previous_run"), dict)
            else {}
        )
        comparison_delta = history.get("weighted_semantic_similarity_percent_delta_vs_comparison")
        latest_delta = history.get("weighted_semantic_similarity_percent_delta_vs_latest")
        comparison_delta_text = "n/a" if comparison_delta is None else f"{comparison_delta:+.3f}%"
        latest_delta_text = "n/a" if latest_delta is None else f"{latest_delta:+.3f}%"
        lines.append(
            f"- Latest comparable history delta: {comparison_delta_text} "
            f"(previous run `{comparison_record.get('run_id', 'unknown')}`)"
        )
        lines.append(
            f"- Latest history delta: {latest_delta_text} "
            f"(previous run `{latest_record.get('run_id', 'unknown')}`)"
        )
    if summary.get("baseline_snapshot"):
        snapshot = summary["baseline_snapshot"]
        lines.append(f"- Baseline snapshot: `{snapshot.get('snapshot_manifest_path')}`")
    improvement = summary.get("improvement_summary")
    if isinstance(improvement, dict):
        lines.extend(["", "## Improvement Summary", "", f"- {improvement.get('headline', 'n/a')}"])
        improved_metrics = improvement.get("improved_metrics") or []
        regressed_metrics = improvement.get("regressed_metrics") or []
        if improved_metrics:
            lines.extend(["", "### Improved Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in improved_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        if regressed_metrics:
            lines.extend(["", "### Regressed Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in regressed_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        top_improved = improvement.get("top_improved_functions") or []
        if top_improved:
            lines.extend(["", "### Improved Functions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improved[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {float(row.get('delta_percent', 0.0) or 0.0):+.3f}% | "
                    f"{float(row.get('baseline_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('current_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    lines.extend([
        "",
        "## By Language",
        "",
        "| Language | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
        "|---|---:|---:|---:|---:|---:|",
    ])
    for lang, bucket in sorted(summary["by_language"].items()):
        lines.append(
            f"| {lang} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
            f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
        )
    if summary.get("by_arch"):
        lines.extend([
            "",
            "## By Architecture",
            "",
            "| Architecture | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for arch, bucket in sorted(summary["by_arch"].items()):
            lines.append(
                f"| {arch} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
            )
    if summary.get("by_source_return_kind"):
        lines.extend([
            "",
            "## By Source Signature",
            "",
            "| Return Kind | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for return_kind, bucket in sorted(summary["by_source_return_kind"].items()):
            lines.append(
                f"| {return_kind} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
            )
        param_shapes = summary.get("by_source_param_shape")
        if isinstance(param_shapes, dict) and param_shapes:
            lines.extend(["", "| Param Shape | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |", "|---|---:|---:|---:|---:|---:|"])
            for param_shape, bucket in sorted(param_shapes.items()):
                lines.append(
                    f"| {param_shape} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                    f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
                )
    if summary.get("behavior_status_counts"):
        lines.extend(["", "## Behavior Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["behavior_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    comp_matrix = summary.get("compiler_option_matrix")
    if isinstance(comp_matrix, dict) and comp_matrix:
        lines.extend([
            "",
            "## Compiler/Optimization Performance Matrix",
            "",
            "| Compiler | Opt | Rows | Mapped % | Decomp OK % | Compile OK % | Behavior Pass % | Avg Semantic | Avg Readability | Avg Correctness |",
            "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|",
        ])
        for compiler, opts in sorted(comp_matrix.items()):
            for opt, metrics in sorted(opts.items()):
                lines.append(
                    f"| {compiler} | {opt} | {metrics.get('row_count', 0)} | "
                    f"{metrics.get('mapped_rate', 0.0)*100.0:.1f}% | "
                    f"{metrics.get('decomp_success_rate', 0.0)*100.0:.1f}% | "
                    f"{metrics.get('candidate_compile_rate', 0.0)*100.0:.1f}% | "
                    f"{metrics.get('behavior_pass_rate', 0.0)*100.0:.1f}% | "
                    f"{float(metrics.get('avg_semantic_score', 0.0) or 0.0):.3f} | "
                    f"{float(metrics.get('avg_readability_score', 0.0) or 0.0):.3f} | "
                    f"{float(metrics.get('avg_semantic_correctness_score', 0.0) or 0.0):.3f} |"
                )
    ai_summary = summary.get("ai_remedy_summary")
    if isinstance(ai_summary, dict) and ai_summary:
        lines.extend([
            "",
            "## AI Remedy Feedback Summary",
            "",
            f"- **Sanitizer Errors**: {ai_summary.get('total_sanitizer_errors', 0)}",
            f"- **Time Limit Exceeded (TLE) Errors**: {ai_summary.get('total_tle_errors', 0)}",
            f"- **Memory Limit Exceeded (MLE) Errors**: {ai_summary.get('total_mle_errors', 0)}",
            f"- **Excessive Nesting Warnings**: {ai_summary.get('excessive_nesting_count', 0)}",
            f"- **Avg CFG Topological Similarity**: {float(ai_summary.get('avg_cfg_similarity', 0.0) or 0.0):.3f}",
        ])
        discrepancies = ai_summary.get("readability_vs_correctness_discrepancy_counts") or {}
        if discrepancies:
            lines.extend([
                "",
                "### Readability vs Semantic Correctness Discrepancies",
                "",
                "| Discrepancy Type | Description | Count |",
                "|---|---|---:|",
            ])
            desc_map = {
                "perfect_alignment": "Both clean/readable and semantically correct",
                "semantic_match_readability_gap": "Semantically correct but low readability",
                "readability_ok_semantic_bug": "Highly readable but semantically incorrect",
                "both_failed": "Both semantically incorrect and unreadable"
            }
            for disc_type in ["perfect_alignment", "semantic_match_readability_gap", "readability_ok_semantic_bug", "both_failed"]:
                count = discrepancies.get(disc_type, 0)
                desc = desc_map.get(disc_type, "")
                lines.append(f"| `{disc_type}` | {desc} | {count} |")
        cfg_mismatches = ai_summary.get("cfg_mismatch_counters") or {}
        if cfg_mismatches:
            lines.extend([
                "",
                "### Top CFG Topological Mismatches",
                "",
                "| Mismatch | Occurrences |",
                "|---|---:|",
            ])
            for mismatch, count in sorted(cfg_mismatches.items(), key=lambda x: (-x[1], x[0]))[:10]:
                lines.append(f"| `{mismatch}` | {count} |")
        suggested_actions = ai_summary.get("suggested_action_counts") or {}
        if suggested_actions:
            lines.extend([
                "",
                "### Top Suggested AI Actions",
                "",
                "| Action | Occurrences |",
                "|---|---:|",
            ])
            for action, count in sorted(suggested_actions.items(), key=lambda x: (-x[1], x[0]))[:10]:
                lines.append(f"| `{action}` | {count} |")
    if summary.get("zero_credit_breakdown"):
        lines.extend(["", "## Zero-Credit Breakdown", "", "| Reason | Rows |", "|---|---:|"])
        for reason, count in sorted(summary["zero_credit_breakdown"].items()):
            lines.append(f"| {reason} | {count} |")
    if summary.get("score_distribution"):
        lines.extend(["", "## Score Distribution", "", "| Bucket | Rows |", "|---|---:|"])
        for bucket, count in sorted(summary["score_distribution"].items()):
            lines.append(f"| {bucket} | {count} |")
    if summary.get("semantic_score_stats"):
        stats = summary["semantic_score_stats"]
        lines.extend(["", "## Semantic Score Stats", ""])
        lines.append(
            f"- Avg {float(stats.get('avg', 0.0) or 0.0):.6f}, "
            f"p50 {float(stats.get('p50', 0.0) or 0.0):.6f}, "
            f"p90 {float(stats.get('p90', 0.0) or 0.0):.6f}, "
            f"nonzero {stats.get('nonzero_count', 0)}/{stats.get('count', 0)} "
            f"({float(stats.get('nonzero_rate', 0.0) or 0.0):.3f})"
        )
    if summary.get("readability_score_stats"):
        r_stats = summary["readability_score_stats"]
        lines.append(
            f"- Readability Score: Avg {float(r_stats.get('avg', 0.0) or 0.0):.6f}, "
            f"p50 {float(r_stats.get('p50', 0.0) or 0.0):.6f}, "
            f"p90 {float(r_stats.get('p90', 0.0) or 0.0):.6f}"
        )
    if summary.get("semantic_correctness_score_stats"):
        c_stats = summary["semantic_correctness_score_stats"]
        lines.append(
            f"- Semantic Correctness Score: Avg {float(c_stats.get('avg', 0.0) or 0.0):.6f}, "
            f"p50 {float(c_stats.get('p50', 0.0) or 0.0):.6f}, "
            f"p90 {float(c_stats.get('p90', 0.0) or 0.0):.6f}"
        )
    score_components = summary.get("score_component_metrics")
    if isinstance(score_components, dict):
        lines.extend(["", "## Score Component Metrics", ""])
        lines.append(
            f"- Behavior contribution {float(score_components.get('behavior_component_score_sum', 0.0) or 0.0):.6f}/"
            f"{float(score_components.get('behavior_component_possible_score_sum', 0.0) or 0.0):.6f}, "
            f"static contribution {float(score_components.get('static_component_score_sum', 0.0) or 0.0):.6f}/"
            f"{float(score_components.get('static_component_possible_score_sum', 0.0) or 0.0):.6f}"
        )
        lines.append(
            f"- Behavior lost {float(score_components.get('behavior_component_lost_score_sum', 0.0) or 0.0):.6f}, "
            f"static lost {float(score_components.get('static_component_lost_score_sum', 0.0) or 0.0):.6f}"
        )
    weight_sensitivity = summary.get("score_weight_sensitivity_metrics")
    if isinstance(weight_sensitivity, dict):
        lines.extend(["", "## Score Weight Sensitivity Metrics", ""])
        lines.append(
            f"- Current weighted score "
            f"{float(weight_sensitivity.get('current_weighted_score_percent', 0.0) or 0.0):.3f}% "
            f"at behavior/static weights "
            f"{float(weight_sensitivity.get('current_behavior_weight', 0.0) or 0.0):.2f}/"
            f"{float(weight_sensitivity.get('current_static_weight', 0.0) or 0.0):.2f}"
        )
        scenarios = weight_sensitivity.get("scenario_scores")
        if isinstance(scenarios, dict) and scenarios:
            lines.extend(["", "| Scenario | Score | Delta vs Current |", "|---|---:|---:|"])
            for name, scenario in sorted(scenarios.items()):
                if not isinstance(scenario, dict):
                    continue
                lines.append(
                    f"| `{name}` | "
                    f"{float(scenario.get('weighted_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(scenario.get('delta_from_current_score_percent', 0.0) or 0.0):+.3f}% |"
                )
        delta_dist = weight_sensitivity.get("behavior_minus_static_score_distribution")
        if isinstance(delta_dist, dict):
            lines.append(
                f"- Behavior-static row delta avg {float(delta_dist.get('avg', 0.0) or 0.0):+.6f}, "
                f"p50 {float(delta_dist.get('p50', 0.0) or 0.0):+.6f}; "
                f"behavior higher rows {weight_sensitivity.get('behavior_greater_than_static_row_count', 0)}, "
                f"static higher rows {weight_sensitivity.get('static_greater_than_behavior_row_count', 0)}"
            )
    component_loss_rows = summary.get("component_loss_hot_row_metrics")
    if isinstance(component_loss_rows, dict):
        top_rows = component_loss_rows.get("top_total_component_loss_rows")
        if isinstance(top_rows, list) and top_rows:
            lines.extend([
                "",
                "## Component Loss Hot Rows",
                "",
                "| Function | Score | Behavior Loss | Static Loss | Behavior |",
                "|---|---:|---:|---:|---|",
            ])
            for row in top_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('behavior_component_loss', 0.0) or 0.0):.6f} | "
                    f"{float(row.get('static_component_loss', 0.0) or 0.0):.6f} | "
                    f"{row.get('behavior_status')} |"
                )
    scoring_contract = summary.get("scoring_contract")
    if isinstance(scoring_contract, dict):
        lines.extend(["", "## Scoring Contract", ""])
        for key, value in sorted(scoring_contract.items()):
            lines.append(f"- `{key}`: {value}")
    score_denominators = summary.get("score_denominator_metrics")
    if isinstance(score_denominators, dict):
        lines.extend(["", "## Score Denominator Metrics", "", "| Metric | Value |", "|---|---:|"])
        for key, value in sorted(score_denominators.items()):
            if isinstance(value, str):
                lines.append(f"| {key} | `{value}` |")
            else:
                lines.append(f"| {key} | {value} |")
    semantic_loss = summary.get("semantic_loss_metrics")
    if isinstance(semantic_loss, dict):
        lines.extend(["", "## Semantic Loss Metrics", ""])
        lines.append(
            f"- Lost score: {float(semantic_loss.get('total_lost_score', 0.0) or 0.0):.6f}, "
            f"avg per row {float(semantic_loss.get('avg_lost_score_per_row', 0.0) or 0.0):.6f}"
        )
        loss_by_stage = semantic_loss.get("lost_score_by_stage_first_failure")
        if isinstance(loss_by_stage, dict) and loss_by_stage:
            lines.extend(["", "| First Stage Failure | Lost Score |", "|---|---:|"])
            for stage, value in sorted(loss_by_stage.items()):
                lines.append(f"| {stage} | {float(value or 0.0):.6f} |")
    readiness = summary.get("semantic_readiness_metrics")
    if isinstance(readiness, dict):
        lines.extend(["", "## Semantic Readiness Metrics", ""])
        lines.append(
            f"- Fully perfect {readiness.get('fully_perfect_rows', 0)}/{readiness.get('manifest_rows', 0)} "
            f"({float(readiness.get('fully_perfect_rate', 0.0) or 0.0):.3f}), "
            f"behavior pass + static perfect {readiness.get('behavior_pass_static_perfect_rows', 0)} "
            f"({float(readiness.get('behavior_pass_static_perfect_rate', 0.0) or 0.0):.3f})"
        )
        lines.append(
            f"- Behavior pass with static gap {readiness.get('behavior_pass_static_gap_rows', 0)}, "
            f"static perfect but behavior non-pass {readiness.get('static_perfect_behavior_nonpass_rows', 0)}, "
            f"pipeline OK but behavior non-pass {readiness.get('pipeline_ok_behavior_nonpass_rows', 0)}"
        )
    integrity = summary.get("benchmark_integrity_metrics")
    if isinstance(integrity, dict):
        lines.extend(["", "## Benchmark Integrity Metrics", ""])
        lines.append(
            f"- Score denominator rows {integrity.get('score_denominator_row_count', 0)}, "
            f"excluded rows {integrity.get('rows_excluded_from_semantic_score_denominator', 0)}, "
            f"static excluded rows {integrity.get('rows_excluded_from_static_similarity_denominator', 0)}"
        )
        lines.append(
            f"- Missing source features penalized: {integrity.get('missing_source_features_penalized')}; "
            f"extra decompiler features penalized: {integrity.get('extra_decompiler_features_penalized')}; "
            f"behavior unsupported/missing fail closed: {integrity.get('behavior_missing_or_unsupported_rows_fail_closed')}"
        )
    improvement_axes = summary.get("improvement_axis_metrics")
    if isinstance(improvement_axes, dict) and improvement_axes:
        lines.extend([
            "",
            "## Improvement Axis Metrics",
            "",
            "| Axis | Rows | Avg Similarity | Lost Score | Missing Features |",
            "|---|---:|---:|---:|---:|",
        ])
        for axis, metrics in sorted(
            improvement_axes.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {axis} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} |"
            )
    focus_areas = summary.get("focus_area_metrics")
    if isinstance(focus_areas, dict) and focus_areas:
        lines.extend([
            "",
            "## Focus Area Metrics",
            "",
            "| Focus Area | Rows | Avg Similarity | Lost Score | Missing Features |",
            "|---|---:|---:|---:|---:|",
        ])
        for area, metrics in sorted(
            focus_areas.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {area} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} |"
            )
    roadmap = summary.get("roadmap_priority_metrics")
    if isinstance(roadmap, dict):
        buckets = roadmap.get("buckets") if isinstance(roadmap.get("buckets"), dict) else {}
        order = roadmap.get("priority_order") if isinstance(roadmap.get("priority_order"), list) else sorted(buckets)
        if buckets:
            lines.extend([
                "",
                "## Roadmap Priority Metrics",
                "",
                "| Priority | Rows | Avg Similarity | Lost Score | Missing | Extra |",
                "|---|---:|---:|---:|---:|---:|",
            ])
            for priority in order:
                metrics = buckets.get(priority)
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| {priority} | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                    f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
                )
    type_data_gaps = summary.get("type_data_gap_metrics")
    if isinstance(type_data_gaps, dict):
        lines.extend(["", "## Type/Data Gap Metrics", ""])
        lines.append(
            f"- Signature gap rows {type_data_gaps.get('signature_gap_row_count', 0)}, "
            f"memory gap rows {type_data_gaps.get('memory_gap_row_count', 0)}, "
            f"call gap rows {type_data_gaps.get('call_gap_row_count', 0)}"
        )
    signedness_gaps = summary.get("signedness_only_signature_gap_metrics")
    if isinstance(signedness_gaps, dict):
        lines.extend(["", "## Signedness-Only Signature Gap Metrics", ""])
        lines.append(
            f"- Rows {signedness_gaps.get('row_count', 0)}, "
            f"pairs {float(signedness_gaps.get('total_pair_count', 0.0) or 0.0):.0f}, "
            f"param pairs {float(signedness_gaps.get('param_pair_count', 0.0) or 0.0):.0f}, "
            f"return pairs {float(signedness_gaps.get('return_pair_count', 0.0) or 0.0):.0f}"
        )
        top_rows = signedness_gaps.get("top_rows")
        if isinstance(top_rows, list) and top_rows:
            lines.extend(["", "| Function | Score | Param Pairs | Return Pairs |", "|---|---:|---:|---:|"])
            for row in top_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('param_pair_count', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('return_pair_count', 0.0) or 0.0):.0f} |"
                )
    signature_confusion = summary.get("signature_kind_confusion_metrics")
    if isinstance(signature_confusion, dict):
        lines.extend(["", "## Signature Kind Confusion Metrics", ""])
        lines.append(
            f"- Return match rate {float(signature_confusion.get('return_match_rate', 0.0) or 0.0):.3f}, "
            f"param match rate {float(signature_confusion.get('param_match_rate', 0.0) or 0.0):.3f}, "
            f"arity mismatch rows {signature_confusion.get('param_arity_mismatch_row_count', 0)}"
        )
        param_pairs = signature_confusion.get("param_pair_counts")
        if isinstance(param_pairs, dict) and param_pairs:
            lines.extend(["", "| Param Pair | Count |", "|---|---:|"])
            for pair, count in sorted(param_pairs.items(), key=lambda item: (-int(item[1]), str(item[0])))[:10]:
                lines.append(f"| `{pair}` | {count} |")
        gap_rows = signature_confusion.get("top_signature_pair_gap_rows")
        if isinstance(gap_rows, list) and gap_rows:
            lines.extend(["", "| Function | Score | Return | Params |", "|---|---:|---|---|"])
            for row in gap_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"`{row.get('source_return_kind')}->{row.get('decomp_return_kind')}` | "
                    f"{int(row.get('param_mismatch_count') or 0)} mismatches |"
                )
    structuring_gaps = summary.get("structuring_gap_metrics")
    if isinstance(structuring_gaps, dict):
        lines.extend(["", "## Structuring Gap Metrics", ""])
        lines.append(
            f"- Control-flow gap rows {structuring_gaps.get('control_flow_gap_row_count', 0)}, "
            f"hard non-perfect rows {structuring_gaps.get('hard_nonperfect_row_count', 0)}"
        )
    fid_name = summary.get("fid_name_recovery_metrics")
    if isinstance(fid_name, dict):
        lines.extend(["", "## FID/Name Recovery Metrics", ""])
        lines.append(f"- Name or mapping gap rows {fid_name.get('name_or_mapping_gap_row_count', 0)}")
    arch_support = summary.get("architecture_support_metrics")
    if isinstance(arch_support, dict) and arch_support:
        lines.extend([
            "",
            "## Architecture Support Metrics",
            "",
            "| Architecture | Rows | Avg Similarity | Lost Score | Top First Failure |",
            "|---|---:|---:|---:|---|",
        ])
        for arch, metrics in sorted(arch_support.items()):
            if not isinstance(metrics, dict):
                continue
            stage_counts = metrics.get("stage_first_failure_counts")
            top_stage = "none"
            if isinstance(stage_counts, dict) and stage_counts:
                top_stage = sorted(stage_counts.items(), key=lambda item: (item[1], item[0]), reverse=True)[0][0]
            lines.append(
                f"| {arch} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | {top_stage} |"
            )
    complexity_quality = summary.get("complexity_quality_metrics")
    if isinstance(complexity_quality, dict):
        buckets = complexity_quality.get("by_source_feature_bucket")
        if isinstance(buckets, dict) and buckets:
            lines.extend([
                "",
                "## Complexity Quality Metrics",
                "",
                "| Source Feature Bucket | Rows | Avg Similarity | Behavior Pass Rate | Zero Rows | Missing Features |",
                "|---|---:|---:|---:|---:|---:|",
            ])
            for bucket_name, bucket in sorted(buckets.items()):
                if not isinstance(bucket, dict):
                    continue
                lines.append(
                    f"| {bucket_name} | {bucket.get('row_count', 0)} | "
                    f"{float(bucket.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(bucket.get('behavior_pass_rate', 0.0) or 0.0):.3f} | "
                    f"{bucket.get('zero_score_count', 0)} | "
                    f"{float(bucket.get('missing_feature_total', 0.0) or 0.0):.0f} |"
                )
        hard_rows = complexity_quality.get("hard_nonperfect_rows") or []
        if hard_rows:
            lines.extend(["", "### Hard Non-Perfect Rows", "", "| Function | Score | Behavior | Stage | Source Features | Missing |", "|---|---:|---|---|---:|---:|"])
            for row in hard_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('behavior_status')} | {row.get('stage_first_failure')} | "
                    f"{float(row.get('source_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('missing_feature_total', 0.0) or 0.0):.0f} |"
                )
    stage_costs = summary.get("stage_cost_correlation_metrics")
    if isinstance(stage_costs, dict):
        by_stage = stage_costs.get("decompile_wall_by_stage_first_failure")
        if isinstance(by_stage, dict) and by_stage:
            lines.extend(["", "## Stage Cost Correlation Metrics", "", "| First Stage Failure | Rows | Avg Decompile Sec | P95 | Max |", "|---|---:|---:|---:|---:|"])
            for stage, stats in sorted(by_stage.items()):
                if not isinstance(stats, dict):
                    continue
                lines.append(
                    f"| {stage} | {stats.get('count', 0)} | "
                    f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('p95', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('max', 0.0) or 0.0):.6f} |"
                )
        score_by_cost = stage_costs.get("score_by_decompile_cost_bucket")
        if isinstance(score_by_cost, dict) and score_by_cost:
            lines.extend(["", "| Cost Bucket | Rows | Avg Score | P50 | Lost Score |", "|---|---:|---:|---:|---:|"])
            lost_by_cost = stage_costs.get("lost_score_by_decompile_cost_bucket")
            if not isinstance(lost_by_cost, dict):
                lost_by_cost = {}
            for bucket, stats in sorted(score_by_cost.items()):
                if not isinstance(stats, dict):
                    continue
                lines.append(
                    f"| {bucket} | {stats.get('count', 0)} | "
                    f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('p50', 0.0) or 0.0):.6f} | "
                    f"{float(lost_by_cost.get(bucket, 0.0) or 0.0):.6f} |"
                )
    admission = summary.get("admission_gate_metrics")
    if isinstance(admission, dict):
        counts = admission.get("counts") if isinstance(admission.get("counts"), dict) else {}
        rates = (
            admission.get("rates_total_denominator")
            if isinstance(admission.get("rates_total_denominator"), dict)
            else {}
        )
        order = admission.get("gate_order") if isinstance(admission.get("gate_order"), list) else sorted(counts)
        lines.extend(["", "## Admission Gate Metrics", "", "| Gate | Rows | Total Rate |", "|---|---:|---:|"])
        for gate_name in order:
            if gate_name not in counts:
                continue
            lines.append(
                f"| {gate_name} | {counts.get(gate_name, 0)} | "
                f"{float(rates.get(gate_name, 0.0) or 0.0):.3f} |"
            )
    quality_funnel = summary.get("quality_gate_funnel_metrics")
    if isinstance(quality_funnel, dict):
        counts = quality_funnel.get("counts") if isinstance(quality_funnel.get("counts"), dict) else {}
        drops = (
            quality_funnel.get("drop_rows_from_previous_gate")
            if isinstance(quality_funnel.get("drop_rows_from_previous_gate"), dict)
            else {}
        )
        retention = (
            quality_funnel.get("retention_rate_from_previous_gate")
            if isinstance(quality_funnel.get("retention_rate_from_previous_gate"), dict)
            else {}
        )
        order = quality_funnel.get("gate_order") if isinstance(quality_funnel.get("gate_order"), list) else sorted(counts)
        if counts:
            lines.extend(["", "## Quality Gate Funnel Metrics", "", "| Gate | Rows | Drop From Previous | Retention |", "|---|---:|---:|---:|"])
            previous_gate = None
            for gate_name in order:
                if gate_name not in counts:
                    continue
                edge = f"{previous_gate}->{gate_name}" if previous_gate is not None else ""
                lines.append(
                    f"| {gate_name} | {counts.get(gate_name, 0)} | "
                    f"{drops.get(edge, 0) if edge else 0} | "
                    f"{float(retention.get(edge, 1.0 if previous_gate is None else 0.0) or 0.0):.3f} |"
                )
                previous_gate = gate_name
    stage_transitions = summary.get("stage_transition_metrics")
    if isinstance(stage_transitions, dict):
        furthest = stage_transitions.get("furthest_ok_stage_counts")
        if isinstance(furthest, dict) and furthest:
            lines.extend(["", "## Stage Transition Metrics", "", "| Furthest OK Stage | Rows |", "|---|---:|"])
            for stage, count in sorted(furthest.items()):
                lines.append(f"| {stage} | {count} |")
    sleigh_health = summary.get("sleigh_lift_health_metrics")
    if isinstance(sleigh_health, dict):
        lines.extend(["", "## SLEIGH Lift Health Metrics", ""])
        lines.append(
            f"- Decode OK {sleigh_health.get('decode_ok_rows', 0)}/{sleigh_health.get('mapped_rows', 0)} "
            f"({float(sleigh_health.get('decode_ok_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator), "
            f"raw p-code OK {sleigh_health.get('raw_pcode_ok_rows', 0)}/{sleigh_health.get('mapped_rows', 0)} "
            f"({float(sleigh_health.get('raw_pcode_ok_rate_mapped_denominator', 0.0) or 0.0):.3f})"
        )
        lines.append(
            f"- Compat imports {float(sleigh_health.get('raw_pcode_compat_import_total', 0.0) or 0.0):.0f}, "
            f"invalid p-code shapes {float(sleigh_health.get('invalid_pcode_shape_total', 0.0) or 0.0):.0f}, "
            f"SLEIGH first-blocker rows {sleigh_health.get('sleigh_first_blocker_row_count', 0)}"
        )
        blocker_rows = sleigh_health.get("top_sleigh_blocker_rows") or []
        if blocker_rows:
            lines.extend(["", "| Function | Address | Score | First Failure |", "|---|---|---:|---|"])
            for row in blocker_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('stage_first_failure')} |"
                )
    failure_diagnostics = summary.get("behavior_failure_diagnostics")
    if isinstance(failure_diagnostics, dict):
        owner_counts = failure_diagnostics.get("owner_counts")
        if isinstance(owner_counts, dict) and owner_counts:
            lines.extend(["", "## Behavior Failure Diagnostics", "", "| Owner | Rows |", "|---|---:|"])
            for owner, count in sorted(owner_counts.items()):
                lines.append(f"| {owner} | {count} |")
        detail_counts = failure_diagnostics.get("detail_signature_counts")
        if isinstance(detail_counts, dict) and detail_counts:
            lines.extend(["", "| Detail Signature | Rows |", "|---|---:|"])
            for signature, count in list(detail_counts.items())[:8]:
                lines.append(f"| `{signature}` | {count} |")
    quadrants = summary.get("semantic_quality_quadrant_metrics")
    if isinstance(quadrants, dict) and quadrants:
        lines.extend([
            "",
            "## Semantic Quality Quadrants",
            "",
            "| Quadrant | Rows | Avg Similarity | Lost Score | Missing | Extra |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for quadrant, metrics in sorted(
            quadrants.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| `{quadrant}` | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
            )
    outcome_matrix = summary.get("outcome_matrix_metrics")
    if isinstance(outcome_matrix, dict):
        outcomes = outcome_matrix.get("top_outcomes_by_lost_score")
        if isinstance(outcomes, dict) and outcomes:
            lines.extend(["", "## Outcome Matrix Metrics", "", "| Outcome | Rows | Lost Score |", "|---|---:|---:|"])
            for outcome, metrics in outcomes.items():
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| `{outcome}` | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} |"
                )
    blind_spots = summary.get("coverage_blind_spot_metrics")
    if isinstance(blind_spots, dict):
        counts = blind_spots.get("counts")
        if isinstance(counts, dict) and counts:
            lines.extend(["", "## Coverage Blind-Spot Metrics", "", "| Blind Spot | Rows |", "|---|---:|"])
            for kind, count in sorted(counts.items()):
                lines.append(f"| {kind} | {count} |")
    gap_density = summary.get("static_gap_density_metrics")
    if isinstance(gap_density, dict):
        missing_density = gap_density.get("missing_density_distribution")
        extra_density = gap_density.get("extra_density_distribution")
        if isinstance(missing_density, dict) and isinstance(extra_density, dict):
            lines.extend(["", "## Static Gap Density Metrics", ""])
            lines.append(
                f"- Missing density avg {float(missing_density.get('avg', 0.0) or 0.0):.6f}, "
                f"p95 {float(missing_density.get('p95', 0.0) or 0.0):.6f}; "
                f"extra density avg {float(extra_density.get('avg', 0.0) or 0.0):.6f}, "
                f"p95 {float(extra_density.get('p95', 0.0) or 0.0):.6f}"
            )
        gap_buckets = gap_density.get("gap_bucket_rows")
        if isinstance(gap_buckets, dict) and gap_buckets:
            lines.extend(["", "| Gap Bucket | Rows | Avg Similarity | Missing | Extra |", "|---|---:|---:|---:|---:|"])
            for bucket, metrics in sorted(gap_buckets.items()):
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| `{bucket}` | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
                )
    static_hot_rows = summary.get("static_gap_hot_row_metrics")
    if isinstance(static_hot_rows, dict):
        missing_rows = static_hot_rows.get("top_missing_feature_rows")
        if isinstance(missing_rows, list) and missing_rows:
            lines.extend(["", "## Static Gap Hot Rows", "", "| Function | Score | Missing | Extra | Top Missing Features |", "|---|---:|---:|---:|---|"])
            for row in missing_rows[:10]:
                if not isinstance(row, dict):
                    continue
                top_missing = ", ".join(
                    f"{item.get('feature')}:{item.get('count')}"
                    for item in (row.get("top_missing_features") or [])[:3]
                    if isinstance(item, dict)
                )
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('static_semantic_score_percent', row.get('semantic_score_percent', 0.0)) or 0.0):.3f}% | "
                    f"{float(row.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('extra_feature_total', 0.0) or 0.0):.0f} | "
                    f"`{top_missing}` |"
                )
    denominator_accounting = summary.get("denominator_accounting_metrics")
    if isinstance(denominator_accounting, dict):
        lines.extend(["", "## Denominator Accounting", "", "| Metric | Rows |", "|---|---:|"])
        for key, value in sorted(denominator_accounting.items()):
            lines.append(f"| {key} | {value} |")
    source_selection = summary.get("source_row_selection_metrics")
    if isinstance(source_selection, dict):
        lines.extend(["", "## Source Row Selection Metrics", ""])
        lines.append(
            f"- Extracted {source_selection.get('extracted_source_function_count', 0)} source functions, "
            f"filtered {source_selection.get('filtered_source_function_count', 0)}, "
            f"selected {source_selection.get('selected_source_function_count', 0)}, "
            f"suppressed static inline helpers "
            f"{source_selection.get('suppressed_static_inline_helper_count', 0)}"
        )
        suppressed = source_selection.get("suppressed_static_inline_helpers") or []
        if suppressed:
            lines.extend(["", "| Suppressed Helper | Entry | Callers | Reason |", "|---|---|---|---|"])
            for row in suppressed[:12]:
                callers = ", ".join(str(name) for name in (row.get("matched_callers") or []))
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('entry_id')}` | `{callers}` | "
                    f"{row.get('reason')} |"
                )
    score_by_behavior = summary.get("score_by_behavior_status")
    if isinstance(score_by_behavior, dict) and score_by_behavior:
        lines.extend(["", "## Score By Behavior Status", "", "| Status | Rows | Avg | P50 | P90 |", "|---|---:|---:|---:|---:|"])
        for status, stats in sorted(score_by_behavior.items()):
            if not isinstance(stats, dict):
                continue
            lines.append(
                f"| {status} | {stats.get('count', 0)} | "
                f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p50', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p90', 0.0) or 0.0):.6f} |"
            )
    behavior_by_stage = summary.get("behavior_status_by_stage_first_failure")
    if isinstance(behavior_by_stage, dict) and behavior_by_stage:
        lines.extend(["", "## Behavior By First Stage Failure", "", "| Stage | Behavior | Rows |", "|---|---|---:|"])
        for stage, counts in sorted(behavior_by_stage.items()):
            if not isinstance(counts, dict):
                continue
            for status, count in sorted(counts.items()):
                lines.append(f"| {stage} | {status} | {count} |")
    if summary.get("stage_first_failure_counts"):
        lines.extend(["", "## First Stage Failure", "", "| Stage Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["stage_first_failure_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("debug_stage_status_matrix"):
        lines.extend(["", "## Debug Stage Status Matrix", "", "| Stage | Status | Rows |", "|---|---|---:|"])
        for stage, counts in sorted(summary["debug_stage_status_matrix"].items()):
            if not isinstance(counts, dict):
                continue
            for status, count in sorted(counts.items()):
                lines.append(f"| {stage} | {status} | {count} |")
    if summary.get("static_similarity_component_average_percent"):
        lines.extend(["", "## Static Similarity Components", "", "| Component | Avg Similarity |", "|---|---:|"])
        for component, avg in sorted(summary["static_similarity_component_average_percent"].items()):
            lines.append(f"| {component} | {float(avg or 0.0):.3f}% |")
    static_gaps = summary.get("static_similarity_gap_totals")
    if isinstance(static_gaps, dict):
        lines.extend(["", "## Static Similarity Gaps", ""])
        lines.append(
            f"- Source features: {int(static_gaps.get('source_feature_total', 0) or 0)}, "
            f"decomp features: {int(static_gaps.get('decomp_feature_total', 0) or 0)}, "
            f"missing: {int(static_gaps.get('missing_feature_total', 0) or 0)} "
            f"({float(static_gaps.get('missing_feature_rate', 0.0) or 0.0):.3f}), "
            f"extra: {int(static_gaps.get('extra_feature_total', 0) or 0)} "
            f"({float(static_gaps.get('extra_feature_rate', 0.0) or 0.0):.3f})"
        )
        missing = static_gaps.get("top_missing_features") or []
        if missing:
            lines.extend(["", "| Top Missing Feature | Count |", "|---|---:|"])
            for item in missing[:10]:
                lines.append(f"| `{item.get('feature')}` | {item.get('count')} |")
        extra = static_gaps.get("top_extra_features") or []
        if extra:
            lines.extend(["", "| Top Extra Feature | Count |", "|---|---:|"])
            for item in extra[:10]:
                lines.append(f"| `{item.get('feature')}` | {item.get('count')} |")
    static_gap_rows = summary.get("static_gap_row_metrics")
    if isinstance(static_gap_rows, dict):
        lines.extend(["", "## Static Gap Row Metrics", ""])
        lines.append(
            f"- Rows with missing features: {static_gap_rows.get('missing_feature_row_count', 0)} "
            f"({float(static_gap_rows.get('missing_feature_row_rate', 0.0) or 0.0):.3f}), "
            f"rows with extra features: {static_gap_rows.get('extra_feature_row_count', 0)} "
            f"({float(static_gap_rows.get('extra_feature_row_rate', 0.0) or 0.0):.3f}), "
            f"zero static intersection rows: {static_gap_rows.get('zero_static_intersection_row_count', 0)} "
            f"({float(static_gap_rows.get('zero_static_intersection_row_rate', 0.0) or 0.0):.3f})"
        )
        component_missing = static_gap_rows.get("component_missing_row_counts")
        if isinstance(component_missing, dict) and component_missing:
            lines.extend(["", "| Component | Missing Rows |", "|---|---:|"])
            for component, count in sorted(component_missing.items()):
                lines.append(f"| {component} | {count} |")
    source_features = summary.get("source_feature_metrics")
    if isinstance(source_features, dict):
        source_dist = source_features.get("source_feature_total_distribution")
        decomp_dist = source_features.get("decomp_feature_total_distribution")
        union_dist = source_features.get("union_feature_total_distribution")
        if isinstance(source_dist, dict) and isinstance(decomp_dist, dict) and isinstance(union_dist, dict):
            lines.extend(["", "## Source Feature Metrics", ""])
            lines.append(
                f"- Source feature avg {float(source_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp feature avg {float(decomp_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"union feature p95 {float(union_dist.get('p95', 0.0) or 0.0):.6f}"
            )
        component_source = source_features.get("component_source_feature_distributions")
        if isinstance(component_source, dict) and component_source:
            lines.extend(["", "| Component | Source Avg Features | Decomp Avg Features |", "|---|---:|---:|"])
            component_decomp = (
                source_features.get("component_decomp_feature_distributions")
                if isinstance(source_features.get("component_decomp_feature_distributions"), dict)
                else {}
            )
            for component, stats in sorted(component_source.items()):
                decomp_stats = component_decomp.get(component) if isinstance(component_decomp, dict) else {}
                lines.append(
                    f"| {component} | {float((stats or {}).get('avg', 0.0) or 0.0):.6f} | "
                    f"{float((decomp_stats or {}).get('avg', 0.0) or 0.0):.6f} |"
                )
    source_variants = summary.get("static_source_variant_metrics")
    if isinstance(source_variants, dict):
        counts = source_variants.get("variant_counts")
        delta_dist = source_variants.get("inline_expanded_static_score_delta_distribution")
        if isinstance(counts, dict) and counts:
            lines.extend(["", "## Static Source Variant Metrics", "", "| Variant | Rows |", "|---|---:|"])
            for variant, count in sorted(counts.items()):
                lines.append(f"| {variant} | {count} |")
        if isinstance(delta_dist, dict) and delta_dist.get("count"):
            lines.append(
                f"- Inline-expanded static score delta avg "
                f"{float(delta_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"max {float(delta_dist.get('max', 0.0) or 0.0):.6f}"
            )
        hot_rows = source_variants.get("top_inline_expanded_static_rows") or []
        if hot_rows:
            lines.extend(["", "| Function | Direct Static | Inline Expanded | Delta |", "|---|---:|---:|---:|"])
            for row in hot_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('direct_static_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('inline_expanded_static_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('static_score_delta_percent', 0.0) or 0.0):.3f}% |"
                )
    static_absence = summary.get("static_absence_penalty_metrics")
    if isinstance(static_absence, dict):
        lines.extend(["", "## Static Absence Penalty Metrics", ""])
        lines.append(
            f"- Source recall {float(static_absence.get('source_recall', 0.0) or 0.0):.6f}, "
            f"decomp precision {float(static_absence.get('decomp_precision', 0.0) or 0.0):.6f}, "
            f"union Jaccard {float(static_absence.get('union_jaccard', 0.0) or 0.0):.6f}"
        )
        lines.append(
            f"- Missing features {int(static_absence.get('missing_feature_total', 0) or 0)}, "
            f"extra features {int(static_absence.get('extra_feature_total', 0) or 0)}, "
            f"rows with no decomp features despite source "
            f"{int(static_absence.get('rows_with_no_decomp_features_despite_source', 0) or 0)}"
        )
    component_absence = summary.get("static_component_absence_matrix_metrics")
    if isinstance(component_absence, dict) and component_absence:
        lines.extend(["", "## Static Component Absence Matrix", ""])
        lines.extend(
            [
                "| Component | Observed Rows | Source Present | Decomp Present | Both Present | Source Only | Decomp Only | Zero Intersection |",
                "|---|---:|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for component, metrics in sorted(component_absence.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {component} | {metrics.get('observed_row_count', 0)} | "
                f"{metrics.get('source_present_row_count', 0)} | "
                f"{metrics.get('decomp_present_row_count', 0)} | "
                f"{metrics.get('both_present_row_count', 0)} | "
                f"{metrics.get('source_only_row_count', 0)} | "
                f"{metrics.get('decomp_only_row_count', 0)} | "
                f"{metrics.get('zero_intersection_source_present_row_count', 0)} |"
            )
    component_pr = summary.get("static_component_precision_recall_metrics")
    if isinstance(component_pr, dict) and component_pr:
        lines.extend(["", "## Static Component Precision/Recall", "", "| Component | Precision | Recall | F1 | Source | Decomp | Intersection |", "|---|---:|---:|---:|---:|---:|---:|"])
        for component, metrics in sorted(component_pr.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {component} | {float(metrics.get('precision', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('recall', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('f1', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('source_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('decomp_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('intersection_feature_total', 0.0) or 0.0):.0f} |"
            )
    size_metrics = summary.get("source_decomp_size_metrics")
    if isinstance(size_metrics, dict):
        line_ratio = size_metrics.get("decomp_to_source_line_ratio_distribution")
        source_lines = size_metrics.get("source_body_line_count_distribution")
        decomp_lines = size_metrics.get("decomp_line_count_distribution")
        if isinstance(line_ratio, dict) and isinstance(source_lines, dict) and isinstance(decomp_lines, dict):
            lines.extend(["", "## Source/Decompiler Size Metrics", ""])
            lines.append(
                f"- Source lines avg {float(source_lines.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp lines avg {float(decomp_lines.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp/source line ratio p95 {float(line_ratio.get('p95', 0.0) or 0.0):.6f}"
            )
        hot_size_rows = size_metrics.get("top_decomp_to_source_line_ratio_rows") or []
        if hot_size_rows:
            lines.extend(["", "| Function | Address | Source Lines | Decomp Lines | Ratio | Behavior |", "|---|---|---:|---:|---:|---|"])
            for row in hot_size_rows[:8]:
                ratio = row.get("decomp_to_source_line_ratio")
                ratio_text = "n/a" if ratio is None else f"{float(ratio or 0.0):.6f}"
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{row.get('source_body_line_count') or 0} | {row.get('decomp_line_count') or 0} | "
                    f"{ratio_text} | {row.get('behavior_status')} |"
                )
    behavior_cases = summary.get("behavior_case_metrics")
    if isinstance(behavior_cases, dict):
        lines.extend(["", "## Behavior Case Metrics", ""])
        lines.append(
            f"- Cases: pass {behavior_cases.get('case_pass_count', 0)}/"
            f"{behavior_cases.get('compared_case_count', behavior_cases.get('case_count', 0))} "
            f"({float(behavior_cases.get('case_pass_rate', 0.0) or 0.0):.3f}), "
            f"failed {behavior_cases.get('case_fail_count', 0)}, "
            f"partial mismatch rows {behavior_cases.get('partial_mismatch_row_count', 0)}, "
            f"partial progress rows {behavior_cases.get('partial_progress_row_count', 0)}"
        )
    behavior_timeouts = summary.get("behavior_timeout_progress_metrics")
    if isinstance(behavior_timeouts, dict) and behavior_timeouts.get("partial_timeout_row_count"):
        lines.extend(["", "## Behavior Timeout Progress Metrics", ""])
        lines.append(
            f"- Partial timeout rows {behavior_timeouts.get('partial_timeout_row_count', 0)}, "
            f"cases passed before timeout {behavior_timeouts.get('partial_timeout_case_pass_count', 0)}/"
            f"{behavior_timeouts.get('partial_timeout_compared_case_count', 0)} "
            f"({float(behavior_timeouts.get('partial_timeout_case_pass_rate', 0.0) or 0.0):.3f}), "
            f"missing candidate lines {behavior_timeouts.get('partial_timeout_missing_candidate_line_total', 0)}"
        )
    partial_progress = summary.get("behavior_partial_progress_metrics")
    if isinstance(partial_progress, dict) and partial_progress.get("row_count"):
        lines.extend(["", "## Behavior Partial Progress Metrics", ""])
        lines.append(
            f"- Partial progress rows {partial_progress.get('row_count', 0)}, "
            f"cases passed {partial_progress.get('case_pass_count', 0)}/"
            f"{partial_progress.get('compared_case_count', 0)}"
        )
        top_rows = partial_progress.get("top_rows")
        if isinstance(top_rows, list) and top_rows:
            lines.extend(["", "| Function | Address | Behavior | Passed Cases | First Mismatch | Score |", "|---|---|---|---:|---:|---:|"])
            for row in top_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | {row.get('behavior_status')} | "
                    f"{row.get('case_pass_count', 0)}/{row.get('compared_case_count', 0)} | "
                    f"{row.get('first_mismatch_index')} | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% |"
                )
    behavior_support = summary.get("behavior_support_metrics")
    if isinstance(behavior_support, dict):
        lines.extend(["", "## Behavior Support Metrics", ""])
        lines.append(
            f"- Eligible rows {behavior_support.get('eligible_row_count', 0)}, "
            f"executed rows {behavior_support.get('executed_row_count', 0)}, "
            f"unsupported signature rows {behavior_support.get('unsupported_signature_row_count', 0)}"
        )
        case_sources = behavior_support.get("case_source_counts")
        if isinstance(case_sources, dict) and case_sources:
            lines.extend(["", "| Case Source | Rows |", "|---|---:|"])
            for source, count in sorted(case_sources.items()):
                lines.append(f"| {source} | {count} |")
        unsupported_reasons = behavior_support.get("unsupported_reason_counts")
        if isinstance(unsupported_reasons, dict) and unsupported_reasons:
            lines.extend(["", "| Unsupported Reason | Rows |", "|---|---:|"])
            for reason, count in sorted(unsupported_reasons.items()):
                lines.append(f"| `{reason}` | {count} |")
    behavior_denominators = summary.get("behavior_denominator_metrics")
    if isinstance(behavior_denominators, dict):
        lines.extend(["", "## Behavior Denominator Metrics", "", "| Metric | Value |", "|---|---:|"])
        for key, value in sorted(behavior_denominators.items()):
            lines.append(f"| {key} | {value} |")
    behavior_mismatches = summary.get("behavior_mismatch_metrics")
    if isinstance(behavior_mismatches, dict) and behavior_mismatches.get("mismatch_row_count"):
        lines.extend(["", "## Behavior Mismatch Metrics", ""])
        lines.append(f"- Mismatch rows: {behavior_mismatches.get('mismatch_row_count', 0)}")
        kinds = behavior_mismatches.get("mismatch_kind_counts")
        if isinstance(kinds, dict) and kinds:
            lines.extend(["", "| Kind | Rows |", "|---|---:|"])
            for kind, count in sorted(kinds.items()):
                lines.append(f"| {kind} | {count} |")
    behavior_distance = summary.get("behavior_distance_metrics")
    if isinstance(behavior_distance, dict):
        case_pass_rate = behavior_distance.get("case_pass_rate_distribution")
        if isinstance(case_pass_rate, dict) and case_pass_rate.get("count"):
            lines.extend(["", "## Behavior Distance Metrics", ""])
            lines.append(
                f"- Case pass rate avg {float(case_pass_rate.get('avg', 0.0) or 0.0):.6f}, "
                f"p50 {float(case_pass_rate.get('p50', 0.0) or 0.0):.6f}, "
                f"p90 {float(case_pass_rate.get('p90', 0.0) or 0.0):.6f}"
            )
            lines.append(
                f"- Missing candidate lines: {behavior_distance.get('missing_candidate_line_total', 0)}, "
                f"extra candidate lines: {behavior_distance.get('extra_candidate_line_total', 0)}"
            )
    if summary.get("harness_cost_metrics"):
        costs = summary["harness_cost_metrics"]
        lines.extend(["", "## Harness Cost Metrics", "", "| Metric | Seconds |", "|---|---:|"])
        for key, value in sorted(costs.items()):
            lines.append(f"| {key} | {float(value or 0.0):.6f} |")
    cost_hot_rows = summary.get("cost_hot_rows")
    if isinstance(cost_hot_rows, dict):
        top_decompile = cost_hot_rows.get("top_decompile_wall_rows") or []
        top_behavior = cost_hot_rows.get("top_behavior_wall_rows") or []
        if top_decompile:
            lines.extend(["", "## Cost Hot Rows", "", "| Function | Address | Decompile Sec | Behavior Wall Sec | Behavior |", "|---|---|---:|---:|---|"])
            for row in top_decompile[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('decompile_sec') or 0.0):.6f} | "
                    f"{float(row.get('behavior_wall_sec') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} |"
                )
        if top_behavior:
            lines.extend(["", "### Behavior Wall Hot Rows", "", "| Function | Address | Behavior Wall Sec | Decompile Sec | Behavior |", "|---|---|---:|---:|---|"])
            for row in top_behavior[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('behavior_wall_sec') or 0.0):.6f} | "
                    f"{float(row.get('decompile_sec') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} |"
                )
    debug_coverage = summary.get("debug_coverage_metrics")
    if isinstance(debug_coverage, dict):
        lines.extend(["", "## Debug Coverage Metrics", ""])
        lines.append(
            f"- Debug decomp rows: {debug_coverage.get('debug_decomp_rows', 0)} "
            f"({float(debug_coverage.get('debug_decomp_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator), "
            f"stage status rows: {debug_coverage.get('debug_stage_status_rows', 0)} "
            f"({float(debug_coverage.get('debug_stage_status_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator)"
        )
    pipeline_stage_metrics = summary.get("pipeline_stage_metrics")
    if isinstance(pipeline_stage_metrics, dict) and pipeline_stage_metrics:
        lines.extend(["", "## Pipeline Stage Metrics", "", "| Stage | Rows | OK | Non-OK | Missing | OK Rate |", "|---|---:|---:|---:|---:|---:|"])
        for stage, metrics in sorted(pipeline_stage_metrics.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {stage} | {metrics.get('row_count', 0)} | {metrics.get('ok_count', 0)} | "
                f"{metrics.get('non_ok_count', 0)} | {metrics.get('missing_count', 0)} | "
                f"{float(metrics.get('ok_rate', 0.0) or 0.0):.3f} |"
            )
    debug_pipeline_numeric = summary.get("debug_pipeline_numeric_metrics")
    if isinstance(debug_pipeline_numeric, dict) and debug_pipeline_numeric:
        lines.extend(["", "## Debug Pipeline Numeric Metrics", "", "| Metric | Rows | Avg | P95 | Max |", "|---|---:|---:|---:|---:|"])
        for metric, stats in sorted(debug_pipeline_numeric.items()):
            if not isinstance(stats, dict):
                continue
            lines.append(
                f"| {metric} | {stats.get('count', 0)} | "
                f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p95', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('max', 0.0) or 0.0):.6f} |"
            )
    nir_stats = summary.get("nir_build_stats_metrics")
    if isinstance(nir_stats, dict) and nir_stats.get("stats_row_count"):
        lines.extend(["", "## NIR Build Stats Metrics", ""])
        lines.append(
            f"- Stats rows: {nir_stats.get('stats_row_count', 0)} "
            f"({float(nir_stats.get('stats_row_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator)"
        )
        debt_totals = nir_stats.get("debt_metric_totals")
        if isinstance(debt_totals, dict) and debt_totals:
            lines.extend(["", "| Debt Metric | Total | Nonzero Rows |", "|---|---:|---:|"])
            nonzero_rows = nir_stats.get("nonzero_row_counts") if isinstance(nir_stats.get("nonzero_row_counts"), dict) else {}
            for metric, total_value in sorted(debt_totals.items()):
                lines.append(f"| {metric} | {total_value} | {nonzero_rows.get(metric, 0)} |")
        top_debt_rows = nir_stats.get("top_debt_rows") or []
        if top_debt_rows:
            lines.extend(["", "### NIR Debt Hot Rows", "", "| Function | Address | Debt Total | Behavior | First Failure |", "|---|---|---:|---|---|"])
            for row in top_debt_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('debt_metric_total') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} | {row.get('stage_first_failure')} |"
                )
    debt_correlation = summary.get("nir_debt_correlation_metrics")
    if isinstance(debt_correlation, dict) and debt_correlation.get("stats_row_count"):
        debt_scores = debt_correlation.get("score_distribution_debt_rows")
        no_debt_scores = debt_correlation.get("score_distribution_no_debt_rows")
        lines.extend(["", "## NIR Debt Correlation Metrics", ""])
        lines.append(
            f"- Debt rows: {debt_correlation.get('debt_row_count', 0)}/"
            f"{debt_correlation.get('stats_row_count', 0)} "
            f"({float(debt_correlation.get('debt_row_rate_stats_denominator', 0.0) or 0.0):.3f})"
        )
        if isinstance(debt_scores, dict) and isinstance(no_debt_scores, dict):
            lines.append(
                f"- Avg score with debt {float(debt_scores.get('avg', 0.0) or 0.0):.6f}, "
                f"without debt {float(no_debt_scores.get('avg', 0.0) or 0.0):.6f}"
            )
    if summary.get("decomp_cache_status_counts"):
        lines.extend(["", "## Decompile Cache Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["decomp_cache_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("behavior_cache_status_counts"):
        lines.extend(["", "## Behavior Cache Status", "", "| Status | Hits |", "|---|---:|"])
        for status, count in sorted(summary["behavior_cache_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("decomp_failure_counts"):
        lines.extend(["", "## Decompile Failures", "", "| Failure | Rows |", "|---|---:|"])
        for failure, count in sorted(summary["decomp_failure_counts"].items()):
            lines.append(f"| {failure} | {count} |")
    if summary.get("debug_owner_bucket_counts"):
        lines.extend(["", "## Debug Owner Buckets", "", "| Bucket | Rows |", "|---|---:|"])
        for bucket, count in sorted(summary["debug_owner_bucket_counts"].items()):
            lines.append(f"| {bucket} | {count} |")
    if summary.get("debug_quality_evidence_totals"):
        lines.extend(["", "## Debug Quality Evidence", "", "| Metric | Total |", "|---|---:|"])
        for metric, total_value in sorted(summary["debug_quality_evidence_totals"].items()):
            lines.append(f"| {metric} | {total_value} |")
    if summary.get("debug_quality_evidence_nonzero_rows"):
        lines.extend(["", "## Debug Quality Evidence Nonzero Rows", "", "| Metric | Rows |", "|---|---:|"])
        for metric, count in sorted(summary["debug_quality_evidence_nonzero_rows"].items()):
            lines.append(f"| {metric} | {count} |")
    if summary.get("debug_template_source_totals"):
        lines.extend(["", "## Debug SLEIGH Template Sources", "", "| Source | Total |", "|---|---:|"])
        for source, total_value in sorted(summary["debug_template_source_totals"].items()):
            lines.append(f"| {source} | {total_value} |")
    gate = summary.get("sleigh_template_source_gate")
    if isinstance(gate, dict):
        lines.extend(["", "## SLEIGH Template Source Gate", ""])
        lines.append(f"- Status: `{gate.get('status')}`")
        lines.append(f"- Required source: `{gate.get('required_source')}`")
        if gate.get("mapped_row_count") is not None:
            lines.append(
                f"- Rows: mapped `{gate.get('mapped_row_count')}` / total `{gate.get('row_count')}`"
            )
        if gate.get("unmapped_row_count"):
            lines.append(
                f"- Unmapped rows ignored by SLEIGH gate: `{gate.get('unmapped_row_count')}`"
            )
        for failure in gate.get("failures") or []:
            lines.append(f"- Failure: {failure}")
    triage_rows = summary.get("triage_priority_rows") or []
    if triage_rows:
        lines.extend(["", "## Triage Priority Rows", "", "| Function | Score | Behavior | Stage | Missing | Artifact |", "|---|---:|---|---|---:|---|"])
        for row in triage_rows[:12]:
            artifact = row.get("behavior_artifact_dir") or row.get("debug_decomp_bundle_path") or ""
            lines.append(
                f"| `{row.get('function_name')}` | "
                f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{row.get('behavior_status')} | {row.get('stage_first_failure')} | "
                f"{row.get('missing_feature_total') or 0} | `{artifact}` |"
            )
    comparison = summary.get("comparison")
    if isinstance(comparison, dict):
        outcome = summary.get("comparison_outcome") if isinstance(summary.get("comparison_outcome"), dict) else {}
        weighted = comparison.get("metric_deltas", {}).get("weighted_semantic_similarity_percent", {})
        delta = weighted.get("delta")
        delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
        lines.extend(
            [
                "",
                "## Baseline Comparison",
                "",
                f"- Baseline: `{comparison.get('baseline_summary_path')}`",
                f"- Outcome: {outcome.get('headline', 'n/a')}",
                f"- Weighted semantic similarity delta: {delta_text}",
                f"- Improved rows: {comparison.get('improved_row_count', 0)}",
                f"- Regressed rows: {comparison.get('regressed_row_count', 0)}",
                f"- Behavior improved rows: {comparison.get('behavior_improved_row_count', 0)}",
                f"- Behavior regressed rows: {comparison.get('behavior_regressed_row_count', 0)}",
                f"- New rows: {comparison.get('new_row_count', 0)}",
                f"- Missing rows: {comparison.get('missing_row_count', 0)}",
            ]
        )
        severity = comparison.get("regression_severity") if isinstance(comparison.get("regression_severity"), dict) else {}
        if severity:
            lines.extend(
                [
                    f"- Negative score delta sum: {float(severity.get('score_delta_sum_negative', 0.0) or 0.0):+.6f}",
                    f"- New zero-score rows: {severity.get('new_zero_score_rows', 0)}",
                    f"- New unmapped rows: {severity.get('new_unmapped_rows', 0)}",
                    f"- New behavior-fail rows: {severity.get('new_behavior_fail_rows', 0)}",
                ]
            )
        top_improvements = comparison.get("top_improvements") or []
        if top_improvements:
            lines.extend(["", "### Top Improvements", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improvements[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_regressions = comparison.get("top_regressions") or []
        if top_regressions:
            lines.extend(["", "### Top Regressions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_regressions[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_deltas = comparison.get("top_row_deltas") or []
        if top_deltas:
            lines.extend(["", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_deltas[:10]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    ghidra_reference = summary.get("ghidra_reference")
    if isinstance(ghidra_reference, dict):
        ghidra_comparison = ghidra_reference.get("comparison") if isinstance(ghidra_reference.get("comparison"), dict) else {}
        delta = ghidra_comparison.get("weighted_semantic_similarity_delta_percent")
        delta_text = "n/a" if delta is None else f"{float(delta):+.3f}%"
        lines.extend(
            [
                "",
                "## Ghidra Reference Lane",
                "",
                "- Contract: Ghidra is scored against source as a reference lane; it is not the oracle.",
                f"- Ghidra summary: `{ghidra_reference.get('summary_path')}`",
                f"- Ghidra rows: `{ghidra_reference.get('rows_path')}`",
                f"- Comparison: `{ghidra_reference.get('comparison_path')}`",
                f"- Ghidra weighted semantic similarity: {float(ghidra_reference.get('weighted_semantic_similarity_percent', 0.0) or 0.0):.3f}%",
                f"- Fission minus Ghidra weighted delta: {delta_text}",
                f"- Export success/failure: {ghidra_reference.get('reference_export_success_count', 0)}/"
                f"{ghidra_reference.get('reference_export_failure_count', 0)}",
            ]
        )
        bucket_counts = ghidra_comparison.get("bucket_counts") if isinstance(ghidra_comparison.get("bucket_counts"), dict) else {}
        if bucket_counts:
            lines.extend(["", "| Bucket | Rows |", "|---|---:|"])
            for bucket, count in sorted(bucket_counts.items()):
                lines.append(f"| `{bucket}` | {count} |")
        export_failures = [
            export
            for export in (ghidra_reference.get("reference_exports") or [])
            if isinstance(export, dict) and not export.get("success")
        ]
        if export_failures:
            lines.extend(["", "### Ghidra Export Failures", "", "| Entry | Failure | Detail |", "|---|---|---|"])
            for export in export_failures[:8]:
                detail = str(export.get("failure_detail") or "")
                if len(detail) > 160:
                    detail = detail[:157] + "..."
                lines.append(
                    f"| `{export.get('entry_id')}` | `{export.get('failure_kind')}` | {detail} |"
                )
        top_ghidra = ghidra_comparison.get("top_ghidra_ahead") or []
        if top_ghidra:
            lines.extend(["", "### Ghidra Ahead Rows", "", "| Function | Delta | Fission | Ghidra | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_ghidra[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {float(row.get('delta_percent', 0.0) or 0.0):+.3f}% | "
                    f"{float(row.get('fission_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('ghidra_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('fission_behavior')} -> {row.get('ghidra_behavior')} |"
                )
        top_fission = ghidra_comparison.get("top_fission_ahead") or []
        if top_fission:
            lines.extend(["", "### Fission Ahead Rows", "", "| Function | Delta | Fission | Ghidra | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_fission[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {float(row.get('delta_percent', 0.0) or 0.0):+.3f}% | "
                    f"{float(row.get('fission_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('ghidra_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('fission_behavior')} -> {row.get('ghidra_behavior')} |"
                )
    debug_triage = summary.get("debug_triage") or []
    if debug_triage:
        lines.extend(["", "## Materialized Debug Triage", "", "| Function | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---|---|---|---|"])
        for row in debug_triage[:12]:
            lines.append(
                f"| `{row.get('function_name')}` | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    regression_debug_triage = summary.get("regression_debug_triage") or []
    if regression_debug_triage:
        lines.extend(["", "## Regression Debug Triage", "", "| Function | Delta | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---:|---|---|---|---|"])
        for row in regression_debug_triage[:12]:
            regression = row.get("baseline_regression") if isinstance(row.get("baseline_regression"), dict) else {}
            delta = regression.get("delta_percent")
            delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
            lines.append(
                f"| `{row.get('function_name')}` | {delta_text} | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    debug_commands = summary.get("debug_repro_commands") or []
    if debug_commands:
        lines.extend(["", "## Debug Repro Commands", ""])
        for row in debug_commands[:8]:
            lines.append(
                f"- `{row.get('entry_id')}` `{row.get('function_name')}` "
                f"({row.get('semantic_score_percent', 0.0):.3f}%, {row.get('behavior_status')}):"
            )
            lines.append("")
            lines.append("  ```bash")
            lines.append(f"  {row.get('debug_decomp_command')}")
            lines.append("  ```")
            if row.get("disasm_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('disasm_function_command')}")
                lines.append("  ```")
            if row.get("xrefs_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('xrefs_function_command')}")
                lines.append("  ```")
            if row.get("preview_candidate_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('preview_candidate_command')}")
                lines.append("  ```")
            if row.get("function_facts_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('function_facts_command')}")
                lines.append("  ```")
            if row.get("behavior_artifact_dir"):
                lines.append(f"  Behavior artifacts: `{row.get('behavior_artifact_dir')}`")
    failing = [row for row in rows if row.get("semantic_score", 0.0) < 1.0][:20]
    if failing:
        lines.extend(["", "## First Non-Perfect Rows", ""])
        for row in failing:
            behavior = row.get("behavior", {})
            lines.append(
                f"- `{row['entry_id']}` `{row['function_name']}`: score={row['semantic_score']:.3f}, "
                f"similarity={row['semantic_score_percent']:.3f}%, "
                f"map={row['mapping_status']}, behavior={behavior.get('status')}"
            )
            if behavior.get("artifact_dir"):
                lines.append(f"  behavior artifacts: `{behavior.get('artifact_dir')}`")
    lines.append("")
    return "\n".join(lines)


STAGE_FAILURE_ORDER = ["load", "decode", "raw_pcode", "nir_build", "normalize", "structuring", "render"]


def shell_command(parts: list[Any]) -> str:
    return " ".join(shlex.quote(str(part)) for part in parts)





