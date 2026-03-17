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
    load_struct_pointer_aliases,
    normalize_address,
)
from grand_finale_support.runners import (
    list_functions_with_fission,
    run_fission_function,
    run_ghidra_binary,
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
        help="Also include Ghidra output for side-by-side comparison",
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


def timing_stats(samples: list[float]) -> dict[str, Any]:
    ms = [round(sample * 1000.0, 3) for sample in samples]
    if not ms:
        return {
            "runs": 0,
            "min_ms": 0.0,
            "max_ms": 0.0,
            "avg_ms": 0.0,
            "median_ms": 0.0,
        }
    return {
        "runs": len(ms),
        "min_ms": round(min(ms), 3),
        "max_ms": round(max(ms), 3),
        "avg_ms": round(sum(ms) / len(ms), 3),
        "median_ms": round(statistics.median(ms), 3),
    }


def compare_delta(legacy: dict[str, Any], preview: dict[str, Any]) -> dict[str, Any]:
    legacy_metrics = legacy.get("metrics", {})
    preview_metrics = preview.get("metrics", {})
    legacy_residue = compute_residue_score(legacy) if legacy.get("success") else 0
    preview_residue = compute_residue_score(preview) if preview.get("success") else 0
    legacy_code = legacy.get("code", "")
    preview_code = preview.get("code", "")
    legacy_timing = legacy.get("timing_stats", {}).get("avg_ms", 0.0)
    preview_timing = preview.get("timing_stats", {}).get("avg_ms", 0.0)
    return {
        "goto_count": int(preview_metrics.get("goto_count", 0)) - int(legacy_metrics.get("goto_count", 0)),
        "temp_surface_count": int(preview_metrics.get("temp_surface_count", 0))
        - int(legacy_metrics.get("temp_surface_count", 0)),
        "cast_chain_count": int(preview_metrics.get("cast_chain_count", 0))
        - int(legacy_metrics.get("cast_chain_count", 0)),
        "helper_call_total": int(preview_metrics.get("helper_call_total", 0))
        - int(legacy_metrics.get("helper_call_total", 0)),
        "residue_score": preview_residue - legacy_residue,
        "code_length": len(preview_code) - len(legacy_code),
        "avg_timing_ms": round(preview_timing - legacy_timing, 3),
        "speedup_ratio": round((legacy_timing / preview_timing), 3) if preview_timing > 0 else None,
    }


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
    preferred = next((entry for entry in attempts if entry.get("success")), attempts[0])
    preferred = dict(preferred)
    preferred.setdefault("address", address)
    preferred.setdefault("name", name)
    preferred["timing_ms"] = round(float(preferred.get("wall_sec", 0.0)) * 1000.0, 3)
    preferred["timing_stats"] = timing_stats(timings)
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
    code_bundle: dict[str, Any] = {
        "legacy": legacy.get("code", ""),
        "preview": preview.get("code", ""),
    }
    if with_ghidra:
        _, ghidra_entries = run_ghidra_binary(
            binary_path,
            [(f"0x{address}", name)],
            ghidra_dir,
            timeout_sec,
            struct_ptr_aliases,
        )
        ghidra_entry = ghidra_entries.get(normalize_address(address), {})
        code_bundle["ghidra"] = ghidra_entry.get("code", "")
    return {
        "address": f"0x{address}",
        "name": name,
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
        "canonicalization_failed_alias_not_fallthrough_count": (
            (preview.get("preview_build_stats") or {}).get(
                "canonicalization_failed_alias_not_fallthrough_count", 0
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
        "delta": compare_delta(legacy, preview),
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


def summarize_results(functions: list[dict[str, Any]]) -> dict[str, Any]:
    preview_better_on_goto = 0
    preview_better_on_temp = 0
    preview_better_on_cast = 0
    preview_better_on_residue = 0
    preview_better_on_helper = 0
    preview_faster = 0
    legacy_faster = 0
    timing_tie = 0
    preview_used_count = 0
    preview_fallback_count = 0
    preview_unstructured_count = 0
    fallback_kind_counts: dict[str, int] = {}
    legacy_dependent_functions: list[dict[str, Any]] = []
    promotion_candidate_total = 0
    promoted_region_total = 0
    promotion_rejected_by_shape_total = 0
    promotion_rejected_by_gate_total = 0
    discovery_seen_guarded_tail_like_shape_total = 0
    discovery_rejected_noncanonical_layout_total = 0
    canonicalized_guarded_tail_shape_total = 0
    canonicalization_failed_multiple_payload_entries_total = 0
    canonicalization_failed_interleaved_join_uses_total = 0
    canonicalization_failed_nonterminal_join_label_total = 0
    canonicalization_failed_nested_tail_escape_total = 0
    canonicalized_interleaved_join_use_total = 0
    canonicalization_failed_alias_not_fallthrough_total = 0
    canonicalization_failed_join_has_external_ref_total = 0
    canonicalization_failed_payload_crosses_join_total = 0
    rejected_must_emit_label_total = 0
    rejected_not_single_pred_succ_total = 0
    rejected_external_entry_total = 0
    rejected_loop_or_switch_target_total = 0
    legacy_avg_samples: list[float] = []
    preview_avg_samples: list[float] = []
    speedup_samples: list[float] = []
    for item in functions:
        delta = item["delta"]
        if delta["goto_count"] < 0:
            preview_better_on_goto += 1
        if delta["temp_surface_count"] < 0:
            preview_better_on_temp += 1
        if delta["cast_chain_count"] < 0:
            preview_better_on_cast += 1
        if delta["residue_score"] < 0:
            preview_better_on_residue += 1
        if delta["helper_call_total"] > 0:
            preview_better_on_helper += 1
        preview = item["preview"]
        preview_used_count += int(preview.get("engine_used") == "mlil_preview" and preview.get("success"))
        preview_fallback_count += int(bool(preview.get("fell_back")))
        preview_unstructured_count += int(item.get("preview_surface_kind") == "unstructured")
        promotion_candidate_total += int(item.get("promotion_candidate_count", 0))
        promoted_region_total += int(item.get("promoted_region_count", 0))
        promotion_rejected_by_shape_total += int(item.get("promotion_rejected_by_shape_count", 0))
        promotion_rejected_by_gate_total += int(item.get("promotion_rejected_by_gate_count", 0))
        discovery_seen_guarded_tail_like_shape_total += int(
            item.get("discovery_seen_guarded_tail_like_shape_count", 0)
        )
        discovery_rejected_noncanonical_layout_total += int(
            item.get("discovery_rejected_noncanonical_layout_count", 0)
        )
        canonicalized_guarded_tail_shape_total += int(
            item.get("canonicalized_guarded_tail_shape_count", 0)
        )
        canonicalization_failed_multiple_payload_entries_total += int(
            item.get("canonicalization_failed_multiple_payload_entries", 0)
        )
        canonicalization_failed_interleaved_join_uses_total += int(
            item.get("canonicalization_failed_interleaved_join_uses", 0)
        )
        canonicalization_failed_nonterminal_join_label_total += int(
            item.get("canonicalization_failed_nonterminal_join_label", 0)
        )
        canonicalization_failed_nested_tail_escape_total += int(
            item.get("canonicalization_failed_nested_tail_escape", 0)
        )
        canonicalized_interleaved_join_use_total += int(
            item.get("canonicalized_interleaved_join_use_count", 0)
        )
        canonicalization_failed_alias_not_fallthrough_total += int(
            item.get("canonicalization_failed_alias_not_fallthrough_count", 0)
        )
        canonicalization_failed_join_has_external_ref_total += int(
            item.get("canonicalization_failed_join_has_external_ref_count", 0)
        )
        canonicalization_failed_payload_crosses_join_total += int(
            item.get("canonicalization_failed_payload_crosses_join_count", 0)
        )
        rejected_must_emit_label_total += int(item.get("rejected_must_emit_label", 0))
        rejected_not_single_pred_succ_total += int(
            item.get("rejected_not_single_pred_succ", 0)
        )
        rejected_external_entry_total += int(item.get("rejected_external_entry", 0))
        rejected_loop_or_switch_target_total += int(
            item.get("rejected_loop_or_switch_target", 0)
        )
        fallback_kind = preview.get("fallback_kind")
        if fallback_kind:
            fallback_kind_counts[fallback_kind] = fallback_kind_counts.get(fallback_kind, 0) + 1
        if item.get("legacy_dependent"):
            legacy_dependent_functions.append(
                {
                    "address": item["address"],
                    "name": item["name"],
                    "preview_engine_used": preview.get("engine_used"),
                    "preview_fallback_kind": preview.get("fallback_kind"),
                    "preview_failure_kind": preview.get("failure_kind"),
                    "preview_surface_kind": item.get("preview_surface_kind"),
                }
            )
        legacy_avg = float(item["legacy"]["timing_stats"]["avg_ms"])
        preview_avg = float(preview["timing_stats"]["avg_ms"])
        legacy_avg_samples.append(legacy_avg)
        preview_avg_samples.append(preview_avg)
        if preview_avg > 0:
            ratio = legacy_avg / preview_avg
            speedup_samples.append(ratio)
            if abs(legacy_avg - preview_avg) <= 0.5:
                timing_tie += 1
            elif ratio > 1.0:
                preview_faster += 1
            else:
                legacy_faster += 1
    return {
        "function_count": len(functions),
        "preview_used_count": preview_used_count,
        "preview_fallback_count": preview_fallback_count,
        "preview_unstructured_count": preview_unstructured_count,
        "preview_better_on_goto_count": preview_better_on_goto,
        "preview_better_on_temp_count": preview_better_on_temp,
        "preview_better_on_cast_count": preview_better_on_cast,
        "preview_better_on_residue_count": preview_better_on_residue,
        "preview_better_on_helper_count": preview_better_on_helper,
        "preview_faster_count": preview_faster,
        "legacy_faster_count": legacy_faster,
        "timing_tie_count": timing_tie,
        "avg_legacy_ms": round(sum(legacy_avg_samples) / len(legacy_avg_samples), 3) if legacy_avg_samples else 0.0,
        "avg_preview_ms": round(sum(preview_avg_samples) / len(preview_avg_samples), 3) if preview_avg_samples else 0.0,
        "avg_speedup_ratio": round(sum(speedup_samples) / len(speedup_samples), 3) if speedup_samples else 0.0,
        "fallback_kind_counts": fallback_kind_counts,
        "promotion_candidate_count": promotion_candidate_total,
        "promoted_region_count": promoted_region_total,
        "promotion_rejected_by_shape_count": promotion_rejected_by_shape_total,
        "promotion_rejected_by_gate_count": promotion_rejected_by_gate_total,
        "discovery_seen_guarded_tail_like_shape_count": discovery_seen_guarded_tail_like_shape_total,
        "discovery_rejected_noncanonical_layout_count": discovery_rejected_noncanonical_layout_total,
        "canonicalized_guarded_tail_shape_count": canonicalized_guarded_tail_shape_total,
        "canonicalization_failed_multiple_payload_entries": canonicalization_failed_multiple_payload_entries_total,
        "canonicalization_failed_interleaved_join_uses": canonicalization_failed_interleaved_join_uses_total,
        "canonicalization_failed_nonterminal_join_label": canonicalization_failed_nonterminal_join_label_total,
        "canonicalization_failed_nested_tail_escape": canonicalization_failed_nested_tail_escape_total,
        "canonicalized_interleaved_join_use_count": canonicalized_interleaved_join_use_total,
        "canonicalization_failed_alias_not_fallthrough_count": canonicalization_failed_alias_not_fallthrough_total,
        "canonicalization_failed_join_has_external_ref_count": canonicalization_failed_join_has_external_ref_total,
        "canonicalization_failed_payload_crosses_join_count": canonicalization_failed_payload_crosses_join_total,
        "rejected_must_emit_label": rejected_must_emit_label_total,
        "rejected_not_single_pred_succ": rejected_not_single_pred_succ_total,
        "rejected_external_entry": rejected_external_entry_total,
        "rejected_loop_or_switch_target": rejected_loop_or_switch_target_total,
        "legacy_dependent_count": len(legacy_dependent_functions),
        "legacy_dependent_functions": legacy_dependent_functions,
    }


def write_markdown_report(report: dict[str, Any], output_path: Path) -> None:
    lines = [
        "# Legacy vs MLIL Preview Comparison",
        "",
        f"- Generated: {report['generated_at']}",
        f"- Binary: `{report['binary']}`",
        f"- Repeat count: {report['repeat']}",
        "",
        "## Summary",
        "",
        f"- Compared functions: {report['summary']['function_count']}",
        f"- Preview used count: {report['summary']['preview_used_count']}",
        f"- Preview fallback count: {report['summary']['preview_fallback_count']}",
        f"- Preview unstructured count: {report['summary'].get('preview_unstructured_count', 0)}",
        f"- Preview faster count: {report['summary']['preview_faster_count']}",
        f"- Preview helper wins: {report['summary']['preview_better_on_helper_count']}",
        f"- Legacy faster count: {report['summary']['legacy_faster_count']}",
        f"- Timing tie count: {report['summary']['timing_tie_count']}",
        f"- Average legacy ms: {report['summary']['avg_legacy_ms']}",
        f"- Average preview ms: {report['summary']['avg_preview_ms']}",
        f"- Average speedup ratio: {report['summary']['avg_speedup_ratio']}",
        f"- Fallback kind counts: {report['summary'].get('fallback_kind_counts', {})}",
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
        "## Function Table",
        "",
        "| Address | Legacy | Preview | Δ goto | Δ temp | Δ cast | Δ helper | Legacy avg ms | Preview avg ms | Speedup | Preview used/fallback | Surface | Promote cand/promoted | Reject shape/gate |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | --- |",
    ]
    for item in report["functions"]:
        legacy = item["legacy"]
        preview = item["preview"]
        delta = item["delta"]
        preview_state = f"{preview.get('engine_used', '')}/{str(bool(preview.get('fell_back'))).lower()}"
        lines.append(
            f"| `{item['address']}` | {'ok' if legacy.get('success') else 'fail'} | "
            f"{'ok' if preview.get('success') else 'fail'} | {delta['goto_count']} | "
            f"{delta['temp_surface_count']} | {delta['cast_chain_count']} | {delta['helper_call_total']} | "
            f"{legacy['timing_stats']['avg_ms']} | {preview['timing_stats']['avg_ms']} | "
            f"{delta['speedup_ratio']} | {preview_state} | {item.get('preview_surface_kind')} | "
            f"{item.get('promotion_candidate_count', 0)}/{item.get('promoted_region_count', 0)} | "
            f"{item.get('promotion_rejected_by_shape_count', 0)}/{item.get('promotion_rejected_by_gate_count', 0)} |"
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
                f"- Legacy timing stats: {item['legacy']['timing_stats']}",
                f"- Preview timing stats: {item['preview']['timing_stats']}",
                f"- Delta: {item['delta']}",
                f"- Promotion stats: candidates={item.get('promotion_candidate_count', 0)}, promoted={item.get('promoted_region_count', 0)}, rejected_shape={item.get('promotion_rejected_by_shape_count', 0)}, rejected_gate={item.get('promotion_rejected_by_gate_count', 0)}, seen_guarded_tail_like={item.get('discovery_seen_guarded_tail_like_shape_count', 0)}, rejected_noncanonical={item.get('discovery_rejected_noncanonical_layout_count', 0)}, canonicalized={item.get('canonicalized_guarded_tail_shape_count', 0)}, canonicalized_interleaved={item.get('canonicalized_interleaved_join_use_count', 0)}, failed_multi_payload={item.get('canonicalization_failed_multiple_payload_entries', 0)}, failed_interleaved_join={item.get('canonicalization_failed_interleaved_join_uses', 0)}, failed_alias_not_fallthrough={item.get('canonicalization_failed_alias_not_fallthrough_count', 0)}, failed_join_has_external_ref={item.get('canonicalization_failed_join_has_external_ref_count', 0)}, failed_payload_crosses_join={item.get('canonicalization_failed_payload_crosses_join_count', 0)}, failed_nonterminal_join={item.get('canonicalization_failed_nonterminal_join_label', 0)}, failed_nested_escape={item.get('canonicalization_failed_nested_tail_escape', 0)}, must_emit={item.get('rejected_must_emit_label', 0)}, not_single_pred_succ={item.get('rejected_not_single_pred_succ', 0)}, external_entry={item.get('rejected_external_entry', 0)}, loop_or_switch={item.get('rejected_loop_or_switch_target', 0)}",
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
        if ghidra_code := item["code"].get("ghidra"):
            lines.extend(["", "#### Ghidra", "```c", ghidra_code, "```"])
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
        "functions": results,
        "summary": summarize_results(results),
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
