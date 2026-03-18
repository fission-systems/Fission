#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import json
import statistics
import time
from pathlib import Path
from typing import Any

from grand_finale_support.metrics import (
    collect_code_metrics,
    compute_residue_score,
    compute_timing_stats,
    load_struct_pointer_aliases,
    normalize_address,
)
from grand_finale_support.runners import (
    list_functions_with_fission,
    run_fission_function,
    run_ghidra_binary_with_meta,
)


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_OUTPUT_DIR = ROOT_DIR / "artifacts" / "compare_legacy_preview"
DEFAULT_GHIDRA_DIR = ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
BASE_TYPES_JSON = ROOT_DIR / "crates" / "fission-signatures" / "data" / "win_types" / "base_types.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare legacy and mlil_preview decompilation output for specific functions."
    )
    parser.add_argument("binary", help="Target binary")
    parser.add_argument(
        "--addresses",
        nargs="*",
        default=[],
        help="Function addresses to compare (for example: 0x140006260 0x140011060)",
    )
    parser.add_argument(
        "--from-summary",
        type=Path,
        help="Use a grand_finale_summary.json to auto-select offender addresses for this binary",
    )
    parser.add_argument(
        "--top-offenders",
        type=int,
        default=0,
        help="Number of offender addresses to extract from the summary for this binary",
    )
    parser.add_argument(
        "--with-ghidra",
        action="store_true",
        help="Also include the pyghidra baseline as a first-class third engine",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help="Directory to write comparison artifacts into",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=DEFAULT_FISSION_BIN,
        help="Path to a prebuilt fission_cli binary with native_decomp enabled",
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=DEFAULT_GHIDRA_DIR,
        help="Path to Ghidra installation directory",
    )
    parser.add_argument(
        "--per-func-timeout",
        type=int,
        default=90,
        help="Per-function timeout in seconds",
    )
    parser.add_argument(
        "--repeat",
        type=int,
        default=1,
        help="Repeat each engine N times to collect timing statistics",
    )
    parser.add_argument(
        "--list-timeout",
        type=int,
        default=15,
        help="Best-effort timeout in seconds for optional --list name resolution; timeout falls back to address-only reporting.",
    )
    return parser.parse_args()


def load_summary_addresses(summary_path: Path, binary_name: str, top_offenders: int) -> list[str]:
    if top_offenders <= 0:
        return []
    data = json.loads(summary_path.read_text())
    binary_reports = data.get("binaries", [])
    selected: list[str] = []
    for report in binary_reports:
        if report.get("binary") != binary_name:
            continue
        for offender in report.get("top_residue_offenders", [])[:top_offenders]:
            addr = offender.get("address")
            if addr:
                selected.append(addr)
        break
    if selected:
        return selected
    for offender in data.get("global", {}).get("top_residue_offenders", []):
        if offender.get("binary") == binary_name and offender.get("address"):
            selected.append(offender["address"])
            if len(selected) >= top_offenders:
                break
    return selected


def resolve_addresses(args: argparse.Namespace, binary_name: str) -> list[str]:
    addresses = [normalize_address(addr) for addr in args.addresses]
    if args.from_summary:
        addresses.extend(normalize_address(addr) for addr in load_summary_addresses(args.from_summary, binary_name, args.top_offenders))
    deduped: list[str] = []
    seen: set[str] = set()
    for addr in addresses:
        if addr not in seen:
            deduped.append(addr)
            seen.add(addr)
    return deduped


def resolve_names(binary_path: Path, fission_bin: Path, addresses: list[str], list_timeout: int) -> dict[str, str]:
    names = {normalize_address(addr): "" for addr in addresses}
    try:
        functions = list_functions_with_fission(ROOT_DIR, binary_path, fission_bin, timeout_sec=list_timeout)
    except Exception:  # noqa: BLE001
        return names
    for address, name in functions:
        normalized = normalize_address(address)
        if normalized in names:
            names[normalized] = name
    return names


def unified_diff_text(legacy_code: str, preview_code: str, address: str) -> str:
    diff = difflib.unified_diff(
        legacy_code.splitlines(),
        preview_code.splitlines(),
        fromfile=f"legacy_{address}",
        tofile=f"preview_{address}",
        lineterm="",
    )
    text = "\n".join(diff)
    return text or "(no diff)"


def compare_pair(left: dict[str, Any] | None, right: dict[str, Any] | None) -> dict[str, Any]:
    left = left or {}
    right = right or {}
    left_metrics = left.get("metrics", {})
    right_metrics = right.get("metrics", {})
    left_residue = compute_residue_score(left) if left.get("success") else 0
    right_residue = compute_residue_score(right) if right.get("success") else 0
    left_code = left.get("code", "")
    right_code = right.get("code", "")
    left_timing = float((left.get("timing_stats") or {}).get("avg_ms", 0.0) or 0.0)
    right_timing = float((right.get("timing_stats") or {}).get("avg_ms", 0.0) or 0.0)
    left_success = bool(left.get("success"))
    right_success = bool(right.get("success"))
    return {
        "success_transition": f"{'ok' if left_success else 'fail'}->{'ok' if right_success else 'fail'}",
        "goto_count": int(right_metrics.get("goto_count", 0)) - int(left_metrics.get("goto_count", 0)),
        "top_level_label_count": int(right_metrics.get("top_level_label_count", 0))
        - int(left_metrics.get("top_level_label_count", 0)),
        "must_emit_label_count": int(right_metrics.get("must_emit_label_count", 0))
        - int(left_metrics.get("must_emit_label_count", 0)),
        "empty_if_count": int(right_metrics.get("empty_if_count", 0))
        - int(left_metrics.get("empty_if_count", 0)),
        "constant_if_count": int(right_metrics.get("constant_if_count", 0))
        - int(left_metrics.get("constant_if_count", 0)),
        "temp_surface_count": int(right_metrics.get("temp_surface_count", 0))
        - int(left_metrics.get("temp_surface_count", 0)),
        "cast_chain_count": int(right_metrics.get("cast_chain_count", 0))
        - int(left_metrics.get("cast_chain_count", 0)),
        "helper_call_total": int(right_metrics.get("helper_call_total", 0))
        - int(left_metrics.get("helper_call_total", 0)),
        "residue_score": right_residue - left_residue,
        "code_length": len(right_code) - len(left_code),
        "avg_timing_ms": round(right_timing - left_timing, 3),
        "speedup_ratio": round((left_timing / right_timing), 3) if right_timing > 0 else None,
    }


def summarize_engine_result(label: str, result: dict[str, Any] | None) -> dict[str, Any]:
    result = result or {}
    metrics = result.get("metrics", {})
    return {
        "label": label,
        "success": bool(result.get("success")),
        "failure_kind": result.get("failure_kind"),
        "failure_detail": result.get("failure_detail"),
        "engine_used": result.get("engine_used", label),
        "fell_back": bool(result.get("fell_back", False)),
        "fallback_kind": result.get("fallback_kind"),
        "avg_ms": float((result.get("timing_stats") or {}).get("avg_ms", 0.0) or 0.0),
        "goto_count": int(metrics.get("goto_count", 0)),
        "top_level_label_count": int(metrics.get("top_level_label_count", 0)),
        "empty_if_count": int(metrics.get("empty_if_count", 0)),
        "constant_if_count": int(metrics.get("constant_if_count", 0)),
        "residue_score": compute_residue_score(result) if result.get("success") else 0,
        "preview_surface_kind": result.get("preview_surface_kind"),
    }


def winner_summary(legacy: dict[str, Any], preview: dict[str, Any], pyghidra: dict[str, Any] | None) -> dict[str, Any]:
    engine_rows = [
        summarize_engine_result("legacy", legacy),
        summarize_engine_result("preview", preview),
    ]
    if pyghidra is not None:
        engine_rows.append(summarize_engine_result("pyghidra", pyghidra))

    successful_rows = [row for row in engine_rows if row["success"]]
    timing_winner = min(successful_rows, key=lambda row: row["avg_ms"])["label"] if successful_rows else None
    goto_winner = min(successful_rows, key=lambda row: row["goto_count"])["label"] if successful_rows else None
    label_winner = min(successful_rows, key=lambda row: row["top_level_label_count"])["label"] if successful_rows else None
    residue_winner = min(successful_rows, key=lambda row: row["residue_score"])["label"] if successful_rows else None
    structured = [row["label"] for row in successful_rows if row.get("preview_surface_kind") == "structured"]
    return {
        "timing_winner": timing_winner,
        "goto_winner": goto_winner,
        "label_winner": label_winner,
        "residue_winner": residue_winner,
        "structured_engines": structured,
        "successful_engines": [row["label"] for row in successful_rows],
    }


def run_engine_repeated(
    binary_path: Path,
    address: str,
    name: str,
    fission_bin: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
    engine: str,
    repeat: int,
) -> dict[str, Any]:
    attempts: list[dict[str, Any]] = []
    timings: list[float] = []
    resource_samples: list[dict[str, Any]] = []
    for _ in range(repeat):
        result = run_fission_function(
            ROOT_DIR,
            binary_path,
            address=address,
            fission_bin=fission_bin,
            timeout_sec=timeout_sec,
            struct_ptr_aliases=struct_ptr_aliases,
            engine=engine,
        )
        attempts.append(result)
        timings.append(float(result.get("wall_sec", 0.0)))
        if result.get("resources"):
            resource_samples.append(result["resources"])
    preferred = next((entry for entry in attempts if entry.get("success")), attempts[0])
    preferred = dict(preferred)
    preferred.setdefault("address", address)
    preferred.setdefault("name", name)
    preferred["timing_ms"] = round(float(preferred.get("wall_sec", 0.0)) * 1000.0, 3)
    preferred["timing_stats"] = compute_timing_stats(timings)
    if resource_samples:
        preferred["resources"] = {
            "max_rss_mb": round(max(sample.get("max_rss_mb", 0.0) for sample in resource_samples), 2),
            "avg_rss_mb": round(sum(sample.get("avg_rss_mb", 0.0) for sample in resource_samples) / len(resource_samples), 2),
            "avg_cpu_pct": round(sum(sample.get("avg_cpu_pct", 0.0) for sample in resource_samples) / len(resource_samples), 2),
            "max_cpu_pct": round(max(sample.get("max_cpu_pct", 0.0) for sample in resource_samples), 2),
        }
    return preferred


def run_pyghidra_repeated(
    binary_path: Path,
    address: str,
    name: str,
    ghidra_dir: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
    repeat: int,
) -> dict[str, Any]:
    attempts: list[dict[str, Any]] = []
    wall_samples: list[float] = []
    init_samples: list[float] = []
    resource_samples: list[dict[str, Any]] = []
    for _ in range(repeat):
        meta, entries = run_ghidra_binary_with_meta(
            binary_path,
            [(f"0x{address}", name)],
            ghidra_dir,
            timeout_sec,
            struct_ptr_aliases,
        )
        result = dict(entries.get(normalize_address(address), {}))
        if not result:
            result = {
                "address": f"0x{address}",
                "name": name,
                "success": False,
                "failure_kind": "missing_function",
            }
        result["engine_used"] = "pyghidra"
        result["fell_back"] = False
        result["fallback_kind"] = None
        attempts.append(result)
        wall_samples.append(float(meta.get("wall_sec", 0.0) or 0.0))
        init_samples.append(float(meta.get("init_sec", 0.0) or 0.0))
        if meta.get("resources"):
            resource_samples.append(meta["resources"])

    preferred = next((entry for entry in attempts if entry.get("success")), attempts[0])
    preferred = dict(preferred)
    preferred["timing_stats"] = compute_timing_stats(wall_samples)
    preferred["init_timing_stats"] = compute_timing_stats(init_samples)
    preferred["timing_ms"] = float(preferred["timing_stats"]["avg_ms"])
    if resource_samples:
        preferred["resources"] = {
            "max_rss_mb": round(max(sample.get("max_rss_mb", 0.0) for sample in resource_samples), 2),
            "avg_rss_mb": round(sum(sample.get("avg_rss_mb", 0.0) for sample in resource_samples) / len(resource_samples), 2),
            "avg_cpu_pct": round(sum(sample.get("avg_cpu_pct", 0.0) for sample in resource_samples) / len(resource_samples), 2),
            "max_cpu_pct": round(max(sample.get("max_cpu_pct", 0.0) for sample in resource_samples), 2),
        }
    return preferred


def compare_function(
    binary_path: Path,
    address: str,
    name: str,
    fission_bin: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
    repeat: int,
    with_ghidra: bool,
    ghidra_dir: Path,
) -> dict[str, Any]:
    legacy = run_engine_repeated(
        binary_path,
        address,
        name,
        fission_bin,
        timeout_sec,
        struct_ptr_aliases,
        "legacy",
        repeat,
    )
    preview = run_engine_repeated(
        binary_path,
        address,
        name,
        fission_bin,
        timeout_sec,
        struct_ptr_aliases,
        "mlil_preview",
        repeat,
    )
    ghidra_entry: dict[str, Any] | None = None
    code_bundle: dict[str, Any] = {
        "legacy": legacy.get("code", ""),
        "preview": preview.get("code", ""),
    }
    if with_ghidra:
        ghidra_entry = run_pyghidra_repeated(
            binary_path,
            address,
            name,
            ghidra_dir,
            timeout_sec,
            struct_ptr_aliases,
            repeat,
        )
        code_bundle["pyghidra"] = ghidra_entry.get("code", "")
    return {
        "address": f"0x{address}",
        "name": name,
        "pyghidra": ghidra_entry,
        "legacy": legacy,
        "preview": preview,
        "preview_surface_kind": classify_preview_surface(preview),
        "promotion_candidate_count": (
            (preview.get("preview_build_stats") or {}).get("promotion_candidate_count", 0)
        ),
        "promoted_region_count": (
            (preview.get("preview_build_stats") or {}).get("promoted_region_count", 0)
        ),
        "promotion_rejected_by_shape_count": (
            (preview.get("preview_build_stats") or {}).get("promotion_rejected_by_shape_count", 0)
        ),
        "promotion_rejected_by_gate_count": (
            (preview.get("preview_build_stats") or {}).get("promotion_rejected_by_gate_count", 0)
        ),
        "discovery_seen_guarded_tail_like_shape_count": (
            (preview.get("preview_build_stats") or {}).get(
                "discovery_seen_guarded_tail_like_shape_count", 0
            )
        ),
        "discovery_rejected_noncanonical_layout_count": (
            (preview.get("preview_build_stats") or {}).get(
                "discovery_rejected_noncanonical_layout_count", 0
            )
        ),
        "canonicalized_guarded_tail_shape_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalized_guarded_tail_shape_count", 0
            )
        ),
        "canonicalization_failed_multiple_payload_entries": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_multiple_payload_entries", 0
            )
        ),
        "canonicalization_failed_interleaved_join_uses": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_interleaved_join_uses", 0
            )
        ),
        "canonicalization_failed_nonterminal_join_label": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_nonterminal_join_label", 0
            )
        ),
        "canonicalization_failed_nested_tail_escape": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_nested_tail_escape", 0
            )
        ),
        "canonicalized_interleaved_join_use_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalized_interleaved_join_use_count", 0
            )
        ),
        "canonicalized_local_nonfallthrough_alias_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalized_local_nonfallthrough_alias_count", 0
            )
        ),
        "canonicalization_failed_alias_not_fallthrough_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_alias_not_fallthrough_count", 0
            )
        ),
        "canonicalization_failed_alias_has_multiple_internal_predecessors_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_alias_has_multiple_internal_predecessors_count", 0
            )
        ),
        "canonicalization_failed_alias_has_nonlocal_ref_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_alias_has_nonlocal_ref_count", 0
            )
        ),
        "canonicalization_failed_alias_body_not_trivial_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_alias_body_not_trivial_count", 0
            )
        ),
        "canonicalization_failed_join_has_external_ref_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_join_has_external_ref_count", 0
            )
        ),
        "canonicalization_failed_payload_crosses_join_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_payload_crosses_join_count", 0
            )
        ),
        "rejected_must_emit_label": (
            (preview.get("preview_build_stats") or {}).get("rejected_must_emit_label", 0)
        ),
        "rejected_not_single_pred_succ": (
            (preview.get("preview_build_stats") or {}).get("rejected_not_single_pred_succ", 0)
        ),
        "rejected_external_entry": (
            (preview.get("preview_build_stats") or {}).get("rejected_external_entry", 0)
        ),
        "rejected_loop_or_switch_target": (
            (preview.get("preview_build_stats") or {}).get("rejected_loop_or_switch_target", 0)
        ),
        "legacy_dependent": is_legacy_dependent(preview),
        "delta": compare_pair(legacy, preview),
        "three_way_delta": {
            "pyghidra_vs_legacy": compare_pair(ghidra_entry, legacy) if ghidra_entry else None,
            "legacy_vs_preview": compare_pair(legacy, preview),
            "pyghidra_vs_preview": compare_pair(ghidra_entry, preview) if ghidra_entry else None,
        },
        "winner_summary": winner_summary(legacy, preview, ghidra_entry),
        "code": code_bundle,
        "diff": unified_diff_text(code_bundle["legacy"], code_bundle["preview"], address),
    }


def classify_preview_surface(preview: dict[str, Any]) -> str | None:
    if not preview.get("success"):
        return None
    if preview.get("engine_used") != "mlil_preview":
        return None
    code = preview.get("code", "")
    if "goto " in code:
        return "unstructured"
    for line in code.splitlines():
        trimmed = line.strip()
        if trimmed.endswith(":") and not trimmed.startswith("case ") and trimmed != "default:":
            return "unstructured"
    return "structured"


def is_legacy_dependent(preview: dict[str, Any]) -> bool:
    if not preview.get("success"):
        return False
    return not (preview.get("engine_used") == "mlil_preview" and not preview.get("fell_back"))


def summarize_engine_collection(
    functions: list[dict[str, Any]],
    label: str,
) -> dict[str, Any]:
    rows: list[dict[str, Any]] = []
    for item in functions:
        row = item.get(label)
        if row is not None:
            rows.append(row)
    success_rows = [row for row in rows if row.get("success")]
    avg_ms_samples = [float((row.get("timing_stats") or {}).get("avg_ms", 0.0) or 0.0) for row in rows]
    return {
        "function_count": len(rows),
        "success_count": sum(1 for row in rows if row.get("success")),
        "failure_count": sum(1 for row in rows if not row.get("success")),
        "timeout_count": sum(1 for row in rows if row.get("failure_kind") == "timeout"),
        "avg_ms": round(sum(avg_ms_samples) / len(avg_ms_samples), 3) if avg_ms_samples else 0.0,
        "median_ms": round(statistics.median(avg_ms_samples), 3) if avg_ms_samples else 0.0,
        "goto_total": sum(int((row.get("metrics") or {}).get("goto_count", 0)) for row in success_rows),
        "top_level_label_total": sum(int((row.get("metrics") or {}).get("top_level_label_count", 0)) for row in success_rows),
        "empty_if_total": sum(int((row.get("metrics") or {}).get("empty_if_count", 0)) for row in success_rows),
        "constant_if_total": sum(int((row.get("metrics") or {}).get("constant_if_count", 0)) for row in success_rows),
        "residue_total": sum(compute_residue_score(row) for row in success_rows),
        "preview_surface_structured_count": sum(1 for item in functions if label == "preview" and item.get("preview_surface_kind") == "structured"),
        "direct_preview_success_count": sum(1 for row in rows if label == "preview" and row.get("success") and row.get("engine_used") == "mlil_preview" and not row.get("fell_back")),
        "fallback_count": sum(1 for row in rows if bool(row.get("fell_back"))),
    }


def summarize_pairwise(functions: list[dict[str, Any]], pair_key: str) -> dict[str, Any]:
    deltas = [item["three_way_delta"][pair_key] for item in functions if item["three_way_delta"].get(pair_key)]
    if not deltas:
        return {}
    speedup_samples = [delta["speedup_ratio"] for delta in deltas if delta.get("speedup_ratio") is not None]
    return {
        "count": len(deltas),
        "avg_timing_ms_delta": round(sum(delta["avg_timing_ms"] for delta in deltas) / len(deltas), 3),
        "avg_speedup_ratio": round(sum(speedup_samples) / len(speedup_samples), 3) if speedup_samples else 0.0,
        "better_on_goto_count": sum(1 for delta in deltas if delta["goto_count"] < 0),
        "better_on_label_count": sum(1 for delta in deltas if delta["top_level_label_count"] < 0),
        "better_on_empty_if_count": sum(1 for delta in deltas if delta["empty_if_count"] < 0),
        "better_on_constant_if_count": sum(1 for delta in deltas if delta["constant_if_count"] < 0),
        "better_on_residue_count": sum(1 for delta in deltas if delta["residue_score"] < 0),
        "success_transitions": {
            transition: sum(1 for delta in deltas if delta["success_transition"] == transition)
            for transition in sorted({delta["success_transition"] for delta in deltas})
        },
    }


def summarize_results(functions: list[dict[str, Any]], with_ghidra: bool) -> dict[str, Any]:
    preview = [item["preview"] for item in functions]
    fallback_kind_counts: dict[str, int] = {}
    legacy_dependent_functions: list[dict[str, Any]] = []
    for item in functions:
        preview_row = item["preview"]
        fallback_kind = preview_row.get("fallback_kind")
        if fallback_kind:
            fallback_kind_counts[fallback_kind] = fallback_kind_counts.get(fallback_kind, 0) + 1
        if item.get("legacy_dependent"):
            legacy_dependent_functions.append(
                {
                    "address": item["address"],
                    "name": item["name"],
                    "preview_engine_used": preview_row.get("engine_used"),
                    "preview_fallback_kind": preview_row.get("fallback_kind"),
                    "preview_failure_kind": preview_row.get("failure_kind"),
                    "preview_surface_kind": item.get("preview_surface_kind"),
                }
            )

    summary = {
        "function_count": len(functions),
        "preview_used_count": sum(1 for row in preview if row.get("engine_used") == "mlil_preview" and row.get("success")),
        "preview_fallback_count": sum(1 for row in preview if bool(row.get("fell_back"))),
        "preview_unstructured_count": sum(1 for item in functions if item.get("preview_surface_kind") == "unstructured"),
        "fallback_kind_counts": fallback_kind_counts,
        "promotion_candidate_count": sum(int(item.get("promotion_candidate_count", 0)) for item in functions),
        "promoted_region_count": sum(int(item.get("promoted_region_count", 0)) for item in functions),
        "promotion_rejected_by_shape_count": sum(int(item.get("promotion_rejected_by_shape_count", 0)) for item in functions),
        "promotion_rejected_by_gate_count": sum(int(item.get("promotion_rejected_by_gate_count", 0)) for item in functions),
        "discovery_seen_guarded_tail_like_shape_count": sum(int(item.get("discovery_seen_guarded_tail_like_shape_count", 0)) for item in functions),
        "discovery_rejected_noncanonical_layout_count": sum(int(item.get("discovery_rejected_noncanonical_layout_count", 0)) for item in functions),
        "canonicalized_guarded_tail_shape_count": sum(int(item.get("canonicalized_guarded_tail_shape_count", 0)) for item in functions),
        "canonicalization_failed_multiple_payload_entries": sum(int(item.get("canonicalization_failed_multiple_payload_entries", 0)) for item in functions),
        "canonicalization_failed_interleaved_join_uses": sum(int(item.get("canonicalization_failed_interleaved_join_uses", 0)) for item in functions),
        "canonicalization_failed_nonterminal_join_label": sum(int(item.get("canonicalization_failed_nonterminal_join_label", 0)) for item in functions),
        "canonicalization_failed_nested_tail_escape": sum(int(item.get("canonicalization_failed_nested_tail_escape", 0)) for item in functions),
        "canonicalized_interleaved_join_use_count": sum(int(item.get("canonicalized_interleaved_join_use_count", 0)) for item in functions),
        "canonicalized_local_nonfallthrough_alias_count": sum(int(item.get("canonicalized_local_nonfallthrough_alias_count", 0)) for item in functions),
        "canonicalization_failed_alias_not_fallthrough_count": sum(int(item.get("canonicalization_failed_alias_not_fallthrough_count", 0)) for item in functions),
        "canonicalization_failed_alias_has_multiple_internal_predecessors_count": sum(int(item.get("canonicalization_failed_alias_has_multiple_internal_predecessors_count", 0)) for item in functions),
        "canonicalization_failed_alias_has_nonlocal_ref_count": sum(int(item.get("canonicalization_failed_alias_has_nonlocal_ref_count", 0)) for item in functions),
        "canonicalization_failed_alias_body_not_trivial_count": sum(int(item.get("canonicalization_failed_alias_body_not_trivial_count", 0)) for item in functions),
        "canonicalization_failed_join_has_external_ref_count": sum(int(item.get("canonicalization_failed_join_has_external_ref_count", 0)) for item in functions),
        "canonicalization_failed_payload_crosses_join_count": sum(int(item.get("canonicalization_failed_payload_crosses_join_count", 0)) for item in functions),
        "rejected_must_emit_label": sum(int(item.get("rejected_must_emit_label", 0)) for item in functions),
        "rejected_not_single_pred_succ": sum(int(item.get("rejected_not_single_pred_succ", 0)) for item in functions),
        "rejected_external_entry": sum(int(item.get("rejected_external_entry", 0)) for item in functions),
        "rejected_loop_or_switch_target": sum(int(item.get("rejected_loop_or_switch_target", 0)) for item in functions),
        "legacy_dependent_count": len(legacy_dependent_functions),
        "legacy_dependent_functions": legacy_dependent_functions,
        "engines": {
            "legacy": summarize_engine_collection(functions, "legacy"),
            "preview": summarize_engine_collection(functions, "preview"),
        },
        "engine_pairs": {
            "legacy_vs_preview": summarize_pairwise(functions, "legacy_vs_preview"),
        },
    }
    if with_ghidra:
        summary["engines"]["pyghidra"] = summarize_engine_collection(functions, "pyghidra")
        summary["engine_pairs"]["pyghidra_vs_legacy"] = summarize_pairwise(functions, "pyghidra_vs_legacy")
        summary["engine_pairs"]["pyghidra_vs_preview"] = summarize_pairwise(functions, "pyghidra_vs_preview")
        py_engine = summary["engines"]["pyghidra"]
        legacy_engine = summary["engines"]["legacy"]
        preview_engine = summary["engines"]["preview"]
        summary["public_summary_line"] = (
            f"legacy vs pyghidra avg speedup {summary['engine_pairs']['pyghidra_vs_legacy'].get('avg_speedup_ratio', 0)}x; "
            f"preview vs legacy residue wins {summary['engine_pairs']['legacy_vs_preview'].get('better_on_residue_count', 0)}/{len(functions)}; "
            f"timeouts pyghidra={py_engine['timeout_count']}, legacy={legacy_engine['timeout_count']}, preview={preview_engine['timeout_count']}"
        )
    else:
        summary["public_summary_line"] = (
            f"preview vs legacy residue wins {summary['engine_pairs']['legacy_vs_preview'].get('better_on_residue_count', 0)}/{len(functions)}"
        )
    return summary


def write_markdown_report(report: dict[str, Any], output_path: Path) -> None:
    with_ghidra = any(item.get("pyghidra") is not None for item in report["functions"])
    title = "# 3-Way Fixed-Seed Benchmark" if with_ghidra else "# Legacy vs MLIL Preview Comparison"
    lines = [
        title,
        "",
        f"- Generated: {report['generated_at']}",
        f"- Binary: `{report['binary']}`",
        f"- Repeat count: {report['repeat']}",
        f"- Cache mode: `{report.get('cache_mode', 'warm')}`",
        "",
        "## Summary",
        "",
        f"- Compared functions: {report['summary']['function_count']}",
        f"- Preview used count: {report['summary']['preview_used_count']}",
        f"- Preview fallback count: {report['summary']['preview_fallback_count']}",
        f"- Preview unstructured count: {report['summary'].get('preview_unstructured_count', 0)}",
        f"- Fallback kind counts: {report['summary'].get('fallback_kind_counts', {})}",
        f"- Public summary: {report['summary'].get('public_summary_line', '')}",
        f"- Promotion candidate count: {report['summary'].get('promotion_candidate_count', 0)}",
        f"- Promoted region count: {report['summary'].get('promoted_region_count', 0)}",
        f"- Promotion rejected by shape: {report['summary'].get('promotion_rejected_by_shape_count', 0)}",
        f"- Promotion rejected by gate: {report['summary'].get('promotion_rejected_by_gate_count', 0)}",
        f"- Discovery saw guarded-tail-like shape: {report['summary'].get('discovery_seen_guarded_tail_like_shape_count', 0)}",
        f"- Discovery rejected noncanonical layout: {report['summary'].get('discovery_rejected_noncanonical_layout_count', 0)}",
        f"- Canonicalized guarded-tail shape: {report['summary'].get('canonicalized_guarded_tail_shape_count', 0)}",
        f"- Canonicalization failed multiple payload entries: {report['summary'].get('canonicalization_failed_multiple_payload_entries', 0)}",
        f"- Canonicalization failed interleaved join uses: {report['summary'].get('canonicalization_failed_interleaved_join_uses', 0)}",
        f"- Canonicalization failed nonterminal join label: {report['summary'].get('canonicalization_failed_nonterminal_join_label', 0)}",
        f"- Canonicalization failed nested tail escape: {report['summary'].get('canonicalization_failed_nested_tail_escape', 0)}",
        f"- Canonicalized interleaved join use: {report['summary'].get('canonicalized_interleaved_join_use_count', 0)}",
        f"- Canonicalization failed alias not fallthrough: {report['summary'].get('canonicalization_failed_alias_not_fallthrough_count', 0)}",
        f"- Canonicalization failed join has external ref: {report['summary'].get('canonicalization_failed_join_has_external_ref_count', 0)}",
        f"- Canonicalization failed payload crosses join: {report['summary'].get('canonicalization_failed_payload_crosses_join_count', 0)}",
        f"- Rejected must-emit label: {report['summary'].get('rejected_must_emit_label', 0)}",
        f"- Rejected not single-pred/succ: {report['summary'].get('rejected_not_single_pred_succ', 0)}",
        f"- Rejected external entry: {report['summary'].get('rejected_external_entry', 0)}",
        f"- Rejected loop/switch target: {report['summary'].get('rejected_loop_or_switch_target', 0)}",
        f"- Legacy-dependent count: {report['summary'].get('legacy_dependent_count', 0)}",
        "",
        "## Why 3-Way",
        "",
        "- `pyghidra`: Python-host baseline",
        "- `legacy`: native FFI baseline",
        "- `preview`: Rust preview pipeline",
        "",
        "## Engine Summary",
        "",
    ]
    engine_table_header = "| Engine | Success | Failure | Timeout | Avg ms | Median ms | goto total | label total | empty if | constant if | residue total |"
    lines.extend([engine_table_header, "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"])
    for label, entry in report["summary"].get("engines", {}).items():
        lines.append(
            f"| `{label}` | {entry.get('success_count', 0)} | {entry.get('failure_count', 0)} | "
            f"{entry.get('timeout_count', 0)} | {entry.get('avg_ms', 0.0)} | {entry.get('median_ms', 0.0)} | "
            f"{entry.get('goto_total', 0)} | {entry.get('top_level_label_total', 0)} | "
            f"{entry.get('empty_if_total', 0)} | {entry.get('constant_if_total', 0)} | {entry.get('residue_total', 0)} |"
        )
    lines.extend([
        "",
        "## Pairwise Deltas",
        "",
        "| Pair | Avg speedup | Avg timing delta ms | Better goto | Better labels | Better empty if | Better const if | Better residue |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for label, entry in report["summary"].get("engine_pairs", {}).items():
        lines.append(
            f"| `{label}` | {entry.get('avg_speedup_ratio', 0.0)} | {entry.get('avg_timing_ms_delta', 0.0)} | "
            f"{entry.get('better_on_goto_count', 0)} | {entry.get('better_on_label_count', 0)} | "
            f"{entry.get('better_on_empty_if_count', 0)} | {entry.get('better_on_constant_if_count', 0)} | "
            f"{entry.get('better_on_residue_count', 0)} |"
        )
    lines.extend([
        "",
        "## Function Table",
        "",
    ])
    if with_ghidra:
        lines.extend([
            "| Address | pyghidra | Legacy | Preview | pyghidra ms | Legacy ms | Preview ms | py->leg speedup | leg->prev speedup | Winner |",
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |",
        ])
    else:
        lines.extend([
            "| Address | Legacy | Preview | Legacy ms | Preview ms | leg->prev speedup | Winner |",
            "| --- | ---: | ---: | ---: | ---: | ---: | --- |",
        ])
    for item in report["functions"]:
        legacy = item["legacy"]
        preview = item["preview"]
        delta = item["three_way_delta"]["legacy_vs_preview"]
        winner = item.get("winner_summary", {}).get("timing_winner")
        if with_ghidra:
            pyghidra = item.get("pyghidra") or {}
            py_to_leg = (item["three_way_delta"].get("pyghidra_vs_legacy") or {}).get("speedup_ratio")
            lines.append(
                f"| `{item['address']}` | {'ok' if pyghidra.get('success') else 'fail'} | {'ok' if legacy.get('success') else 'fail'} | "
                f"{'ok' if preview.get('success') else 'fail'} | {pyghidra.get('timing_stats', {}).get('avg_ms', 0.0)} | "
                f"{legacy['timing_stats']['avg_ms']} | {preview['timing_stats']['avg_ms']} | {py_to_leg} | {delta['speedup_ratio']} | `{winner}` |"
            )
        else:
            lines.append(
                f"| `{item['address']}` | {'ok' if legacy.get('success') else 'fail'} | {'ok' if preview.get('success') else 'fail'} | "
                f"{legacy['timing_stats']['avg_ms']} | {preview['timing_stats']['avg_ms']} | {delta['speedup_ratio']} | `{winner}` |"
            )
    lines.extend(["", "## Legacy-Dependent", ""])
    if not report["summary"].get("legacy_dependent_functions"):
        lines.append("- none")
    else:
        for item in report["summary"]["legacy_dependent_functions"]:
            lines.append(
                f"- `{item['address']}` `{item['name']}`: "
                f"preview_engine={item.get('preview_engine_used')}, "
                f"fallback={item.get('preview_fallback_kind')}, "
                f"failure={item.get('preview_failure_kind')}, "
                f"surface={item.get('preview_surface_kind')}"
            )
    lines.extend(["", "## Details", ""])
    for item in report["functions"]:
        lines.extend(
            [
                f"### {item['address']} {item['name']}",
                "",
                f"- Legacy success: {item['legacy'].get('success')}",
                f"- Preview success: {item['preview'].get('success')}",
                f"- pyghidra success: {(item.get('pyghidra') or {}).get('success')}",
                f"- Winner summary: {item.get('winner_summary')}",
                f"- Legacy timing stats: {item['legacy']['timing_stats']}",
                f"- Preview timing stats: {item['preview']['timing_stats']}",
                f"- pyghidra timing stats: {(item.get('pyghidra') or {}).get('timing_stats')}",
                f"- Pair deltas: {item['three_way_delta']}",
                f"- Promotion stats: candidates={item.get('promotion_candidate_count', 0)}, promoted={item.get('promoted_region_count', 0)}, rejected_shape={item.get('promotion_rejected_by_shape_count', 0)}, rejected_gate={item.get('promotion_rejected_by_gate_count', 0)}, seen_guarded_tail_like={item.get('discovery_seen_guarded_tail_like_shape_count', 0)}, rejected_noncanonical={item.get('discovery_rejected_noncanonical_layout_count', 0)}, canonicalized={item.get('canonicalized_guarded_tail_shape_count', 0)}, canonicalized_interleaved={item.get('canonicalized_interleaved_join_use_count', 0)}, canonicalized_local_nonfallthrough={item.get('canonicalized_local_nonfallthrough_alias_count', 0)}, failed_multi_payload={item.get('canonicalization_failed_multiple_payload_entries', 0)}, failed_interleaved_join={item.get('canonicalization_failed_interleaved_join_uses', 0)}, failed_alias_not_fallthrough={item.get('canonicalization_failed_alias_not_fallthrough_count', 0)}, failed_alias_multi_pred={item.get('canonicalization_failed_alias_has_multiple_internal_predecessors_count', 0)}, failed_alias_nonlocal_ref={item.get('canonicalization_failed_alias_has_nonlocal_ref_count', 0)}, failed_alias_body_not_trivial={item.get('canonicalization_failed_alias_body_not_trivial_count', 0)}, failed_join_has_external_ref={item.get('canonicalization_failed_join_has_external_ref_count', 0)}, failed_payload_crosses_join={item.get('canonicalization_failed_payload_crosses_join_count', 0)}, failed_nonterminal_join={item.get('canonicalization_failed_nonterminal_join_label', 0)}, failed_nested_escape={item.get('canonicalization_failed_nested_tail_escape', 0)}, must_emit={item.get('rejected_must_emit_label', 0)}, not_single_pred_succ={item.get('rejected_not_single_pred_succ', 0)}, external_entry={item.get('rejected_external_entry', 0)}, loop_or_switch={item.get('rejected_loop_or_switch_target', 0)}",
                "",
                "#### Legacy",
                "```c",
                item["code"]["legacy"],
                "```",
                "",
                "#### MLIL Preview",
                "```c",
                item["code"]["preview"],
                "```",
            ]
        )
        if ghidra_code := item["code"].get("pyghidra"):
            lines.extend(["", "#### pyghidra", "```c", ghidra_code, "```"])
        lines.extend(["", "#### Unified Diff", "```diff", item["diff"], "```", ""])
    output_path.write_text("\n".join(lines))


def main() -> int:
    args = parse_args()
    binary_path = Path(args.binary).resolve()
    binary_name = binary_path.stem
    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")
    if args.with_ghidra and not args.ghidra_dir.exists():
        raise SystemExit(f"Ghidra dir not found: {args.ghidra_dir}")
    if args.repeat <= 0:
        raise SystemExit("--repeat must be >= 1")

    addresses = resolve_addresses(args, binary_name)
    if not addresses:
        raise SystemExit("No function addresses selected; use --addresses or --from-summary/--top-offenders")
    struct_ptr_aliases = load_struct_pointer_aliases(BASE_TYPES_JSON)
    names = resolve_names(binary_path, args.fission_bin, addresses, args.list_timeout)

    results: list[dict[str, Any]] = []
    for normalized in addresses:
        display_address = f"0x{normalized}"
        name = names.get(normalized, "")
        print(f"[*] Comparing {binary_name} {display_address} {name}", flush=True)
        result = compare_function(
            binary_path,
            normalized,
            name,
            args.fission_bin,
            args.per_func_timeout,
            struct_ptr_aliases,
            args.repeat,
            args.with_ghidra,
            args.ghidra_dir,
        )
        results.append(result)

    report = {
        "binary": str(binary_path),
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "repeat": args.repeat,
        "cache_mode": "warm",
        "functions": results,
        "summary": summarize_results(results, with_ghidra=args.with_ghidra),
    }
    json_path = output_dir / f"{binary_name}_legacy_vs_preview.json"
    md_path = output_dir / f"{binary_name}_legacy_vs_preview.md"
    json_path.write_text(json.dumps(report, indent=2))
    write_markdown_report(report, md_path)
    print(f"[+] Wrote comparison JSON to {json_path}", flush=True)
    print(f"[+] Wrote comparison Markdown to {md_path}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
