from __future__ import annotations

from collections import Counter
from typing import Any

from .metrics import (
    classify_failure_kind,
    collect_top_residue_offenders,
    compute_residue_score,
    normalize_address,
)


def summarize_binary(
    binary_name: str,
    functions: list[tuple[str, str]],
    fission_entries: dict[str, dict[str, Any]],
    ghidra_entries: dict[str, dict[str, Any]],
    preview_entries: dict[str, dict[str, Any]] | None = None,
) -> dict[str, Any]:
    preview_entries = preview_entries or {}
    shared_success = [
        normalize_address(addr)
        for addr, _ in functions
        if normalize_address(addr) in fission_entries
        and normalize_address(addr) in ghidra_entries
        and fission_entries[normalize_address(addr)].get("success")
        and ghidra_entries[normalize_address(addr)].get("success")
    ]

    def sum_metric(entries: dict[str, dict[str, Any]], addrs: list[str], key: str) -> int:
        return sum(int(entries[addr]["metrics"].get(key, 0)) for addr in addrs)

    ghidra_gotos = sum_metric(ghidra_entries, shared_success, "goto_count")
    fission_gotos = sum_metric(fission_entries, shared_success, "goto_count")
    goto_reduction_pct = (
        round((ghidra_gotos - fission_gotos) * 100.0 / ghidra_gotos, 2) if ghidra_gotos else 0.0
    )

    def aggregate_type_hits(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get("type_hits", {}))
        return total

    def aggregate_type_preservation(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get("type_preservation_hits", {}))
        return total

    def aggregate_residues(entries: dict[str, dict[str, Any]], key: str) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get(key, {}))
        return total

    def aggregate_fallbacks(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if entry.get("success"):
                total.update(entry.get("metrics", {}).get("fallback_counts", {}))
            else:
                total.update(entry.get("fallback_counts", {}))
        return total

    def preview_engine_entries(entries: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
        return [
            entry
            for entry in entries.values()
            if entry.get("success") and entry.get("engine_used") == "mlil_preview"
        ]

    fission_successes = sum(1 for entry in fission_entries.values() if entry.get("success"))
    ghidra_successes = sum(1 for entry in ghidra_entries.values() if entry.get("success"))
    top_residue_offenders = collect_top_residue_offenders(fission_entries)
    preview_successes = sum(1 for entry in preview_entries.values() if entry.get("success"))
    preview_engine_used = preview_engine_entries(preview_entries)
    preview_engine_used_count = len(preview_engine_used)
    preview_fallback_count = sum(1 for entry in preview_entries.values() if entry.get("fell_back"))
    preview_goto_count = sum(
        int(entry.get("metrics", {}).get("goto_count", 0)) for entry in preview_engine_used
    )
    preview_temp_surface_count = sum(
        int(entry.get("metrics", {}).get("temp_surface_count", 0)) for entry in preview_engine_used
    )
    preview_cast_density = sum(
        int(entry.get("metrics", {}).get("cast_chain_count", 0))
        for entry in preview_engine_used
    )
    preview_helper_call_total = sum(
        int(entry.get("metrics", {}).get("helper_call_total", 0))
        for entry in preview_engine_used
    )
    preview_helper_call_counts: Counter[str] = Counter()
    for entry in preview_engine_used:
        preview_helper_call_counts.update(entry.get("metrics", {}).get("helper_call_counts", {}))
    preview_residue_total = sum(compute_residue_score(entry) for entry in preview_engine_used)

    return {
        "binary": binary_name,
        "function_count": len(functions),
        "shared_success_count": len(shared_success),
        "fission_success_count": fission_successes,
        "ghidra_success_count": ghidra_successes,
        "fission_failure_count": len(functions) - fission_successes,
        "ghidra_failure_count": len(functions) - ghidra_successes,
        "control_flow": {
            "ghidra_gotos": ghidra_gotos,
            "fission_gotos": fission_gotos,
            "goto_reduction_pct": goto_reduction_pct,
            "ghidra_switches": sum_metric(ghidra_entries, shared_success, "switch_count"),
            "fission_switches": sum_metric(fission_entries, shared_success, "switch_count"),
            "ghidra_for_loops": sum_metric(ghidra_entries, shared_success, "for_count"),
            "fission_for_loops": sum_metric(fission_entries, shared_success, "for_count"),
            "ghidra_do_while": sum_metric(ghidra_entries, shared_success, "do_while_count"),
            "fission_do_while": sum_metric(fission_entries, shared_success, "do_while_count"),
        },
        "type_promotion": {
            "fission_hits": dict(aggregate_type_hits(fission_entries).most_common()),
            "ghidra_hits": dict(aggregate_type_hits(ghidra_entries).most_common()),
        },
        "type_preservation_counts": {
            "fission": dict(aggregate_type_preservation(fission_entries).most_common()),
            "ghidra": dict(aggregate_type_preservation(ghidra_entries).most_common()),
        },
        "fallback_counts": {
            "fission": dict(aggregate_fallbacks(fission_entries).most_common()),
            "ghidra": dict(aggregate_fallbacks(ghidra_entries).most_common()),
        },
        "cast_chain_counts": {
            "fission": sum(
                int(entry.get("metrics", {}).get("cast_chain_count", 0))
                for entry in fission_entries.values()
                if entry.get("success")
            ),
            "ghidra": sum(
                int(entry.get("metrics", {}).get("cast_chain_count", 0))
                for entry in ghidra_entries.values()
                if entry.get("success")
            ),
        },
        "mlil_preview_success": preview_successes,
        "preview_engine_used_count": preview_engine_used_count,
        "preview_fallback_count": preview_fallback_count,
        "preview_goto_count": preview_goto_count,
        "preview_temp_surface_count": preview_temp_surface_count,
        "mlil_preview_residue": preview_residue_total,
        "mlil_preview_cast_density": preview_cast_density,
        "mlil_preview_helper_call_total": preview_helper_call_total,
        "mlil_preview_helper_call_counts": dict(preview_helper_call_counts),
        "residue_rankings": {
            "single_assign_temps": dict(
                aggregate_residues(fission_entries, "single_assign_temps").most_common(20)
            ),
            "residue_names": dict(
                aggregate_residues(fission_entries, "residue_names").most_common(20)
            ),
            "residue_families": dict(
                aggregate_residues(fission_entries, "residue_families").most_common()
            ),
        },
        "top_residue_offenders": top_residue_offenders,
        "mlil_preview_top_residue_offenders": collect_top_residue_offenders(preview_entries),
        "failure_class_counts": {
            "fission": dict(
                Counter(
                    classify_failure_kind(entry.get("failure_kind") or entry.get("failure_detail"))
                    for entry in fission_entries.values()
                    if not entry.get("success")
                )
            ),
            "ghidra": dict(
                Counter(
                    classify_failure_kind(entry.get("failure_kind") or entry.get("error"))
                    for entry in ghidra_entries.values()
                    if not entry.get("success")
                )
            ),
        },
        "timings": {
            "fission_total_decomp_sec": round(
                sum(float(entry.get("decomp_sec", 0.0)) for entry in fission_entries.values()), 6
            ),
            "fission_total_postprocess_sec": round(
                sum(float(entry.get("postprocess_sec", 0.0)) for entry in fission_entries.values()),
                6,
            ),
            "ghidra_total_decomp_sec": round(
                sum(float(entry.get("decomp_sec", 0.0)) for entry in ghidra_entries.values()), 6
            ),
        },
    }


def aggregate_global_report(binary_reports: list[dict[str, Any]]) -> dict[str, Any]:
    global_report = {
        "shared_success_count": 0,
        "fission_success_count": 0,
        "ghidra_success_count": 0,
        "control_flow": Counter(),
        "failure_class_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "type_preservation_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "fallback_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "cast_chain_counts": {
            "fission": 0,
            "ghidra": 0,
        },
        "mlil_preview_success": 0,
        "preview_engine_used_count": 0,
        "preview_fallback_count": 0,
        "preview_goto_count": 0,
        "preview_temp_surface_count": 0,
        "mlil_preview_residue": 0,
        "mlil_preview_cast_density": 0,
        "mlil_preview_helper_call_total": 0,
        "mlil_preview_helper_call_counts": Counter(),
        "residue_rankings": {
            "single_assign_temps": Counter(),
            "residue_names": Counter(),
            "residue_families": Counter(),
        },
        "top_residue_offenders": [],
        "mlil_preview_top_residue_offenders": [],
    }

    for report in binary_reports:
        global_report["shared_success_count"] += report["shared_success_count"]
        global_report["fission_success_count"] += report["fission_success_count"]
        global_report["ghidra_success_count"] += report["ghidra_success_count"]
        global_report["control_flow"].update(report["control_flow"])
        global_report["failure_class_counts"]["fission"].update(report["failure_class_counts"]["fission"])
        global_report["failure_class_counts"]["ghidra"].update(report["failure_class_counts"]["ghidra"])
        global_report["type_preservation_counts"]["fission"].update(report["type_preservation_counts"]["fission"])
        global_report["type_preservation_counts"]["ghidra"].update(report["type_preservation_counts"]["ghidra"])
        global_report["fallback_counts"]["fission"].update(report["fallback_counts"]["fission"])
        global_report["fallback_counts"]["ghidra"].update(report["fallback_counts"]["ghidra"])
        global_report["cast_chain_counts"]["fission"] += report["cast_chain_counts"]["fission"]
        global_report["cast_chain_counts"]["ghidra"] += report["cast_chain_counts"]["ghidra"]
        global_report["mlil_preview_success"] += report.get("mlil_preview_success", 0)
        global_report["preview_engine_used_count"] += report.get("preview_engine_used_count", 0)
        global_report["preview_fallback_count"] += report.get("preview_fallback_count", 0)
        global_report["preview_goto_count"] += report.get("preview_goto_count", 0)
        global_report["preview_temp_surface_count"] += report.get("preview_temp_surface_count", 0)
        global_report["mlil_preview_residue"] += report.get("mlil_preview_residue", 0)
        global_report["mlil_preview_cast_density"] += report.get("mlil_preview_cast_density", 0)
        global_report["mlil_preview_helper_call_total"] += report.get("mlil_preview_helper_call_total", 0)
        global_report["mlil_preview_helper_call_counts"].update(
            report.get("mlil_preview_helper_call_counts", {})
        )
        for key in global_report["residue_rankings"]:
            global_report["residue_rankings"][key].update(report["residue_rankings"][key])
        for offender in report.get("top_residue_offenders", []):
            global_report["top_residue_offenders"].append({"binary": report["binary"], **offender})
        for offender in report.get("mlil_preview_top_residue_offenders", []):
            global_report["mlil_preview_top_residue_offenders"].append(
                {"binary": report["binary"], **offender}
            )

    ghidra_gotos = global_report["control_flow"]["ghidra_gotos"]
    fission_gotos = global_report["control_flow"]["fission_gotos"]
    global_report["control_flow"]["goto_reduction_pct"] = (
        round((ghidra_gotos - fission_gotos) * 100.0 / ghidra_gotos, 2) if ghidra_gotos else 0.0
    )
    for key in global_report["residue_rankings"]:
        global_report["residue_rankings"][key] = dict(
            global_report["residue_rankings"][key].most_common(20)
        )
    global_report["top_residue_offenders"] = sorted(
        global_report["top_residue_offenders"],
        key=lambda item: (
            -int(item["residue_score"]),
            -int(item["raw_pointer_fallback"]),
            -int(item["single_assign_temp_total"]),
            item["binary"],
            item["address"],
        ),
    )[:10]
    global_report["mlil_preview_top_residue_offenders"] = sorted(
        global_report["mlil_preview_top_residue_offenders"],
        key=lambda item: (
            -int(item["residue_score"]),
            -int(item["raw_pointer_fallback"]),
            -int(item["single_assign_temp_total"]),
            item["binary"],
            item["address"],
        ),
    )[:10]
    global_report["failure_class_counts"] = {
        side: dict(counter)
        for side, counter in global_report["failure_class_counts"].items()
    }
    global_report["type_preservation_counts"] = {
        side: dict(counter)
        for side, counter in global_report["type_preservation_counts"].items()
    }
    global_report["fallback_counts"] = {
        side: dict(counter)
        for side, counter in global_report["fallback_counts"].items()
    }
    global_report["control_flow"] = dict(global_report["control_flow"])
    global_report["mlil_preview_helper_call_counts"] = dict(
        global_report["mlil_preview_helper_call_counts"].most_common()
    )
    return global_report
