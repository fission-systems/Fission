from __future__ import annotations

from pathlib import Path
from typing import Any

from .inventory_reader import normalize_address, run_function_facts_inventory


def run_candidate_inventory(
    root_dir: Path,
    binary_path: Path,
    fission_bin: Path,
    *,
    timeout_ms: int | None = None,
    limit: int | None = None,
    address: str | None = None,
) -> dict[str, Any]:
    rows, summary = run_function_facts_inventory(
        root_dir,
        binary_path,
        fission_bin,
        timeout_ms=timeout_ms or 10000,
        functions_limit=None if address else limit,
        chunk_size=min(limit or 100, 100),
    )
    if address:
        wanted = normalize_address(address)
        rows = [row for row in rows if normalize_address(row.get("address", "")) == wanted]
    return {
        "binary": binary_path.stem,
        "binary_path": str(binary_path),
        "candidate_count": len(rows),
        "candidates": rows,
        "summary": summary,
        "scan_mode": "function_facts_inventory",
    }


def explicit_fact_total(entry: dict[str, Any]) -> int:
    if "explicit_fact_total" in entry:
        return int(entry.get("explicit_fact_total", 0) or 0)
    return (
        int(entry.get("dwarf_param_count", 0))
        + int(entry.get("dwarf_local_count", 0))
        + (1 if entry.get("has_dwarf_return_type") else 0)
    )


def source_is_preview_aligned(source_meta: dict[str, Any] | None) -> bool:
    if not source_meta:
        return True
    alignment = source_meta.get("admission_alignment")
    if alignment:
        return alignment == "aligned"
    if not bool(source_meta.get("expected_preview_supported", True)):
        return False
    failure_kind = source_meta.get("observed_preview_failure_kind")
    if failure_kind in {"preview_architecture_unsupported", "preview_format_unsupported"}:
        return False
    return True


def preview_hint_total(entry: dict[str, Any]) -> int:
    stats = entry.get("preview_hint_stats") or {}
    return sum(int(value or 0) for value in stats.values())


def heuristic_surface_candidate(entry: dict[str, Any]) -> bool:
    if "heuristic_surface_candidate" in entry:
        return bool(entry.get("heuristic_surface_candidate"))
    reason_tags = set(entry.get("reason_tags") or [])
    return (
        bool(entry.get("preview_direct_success"))
        and not bool(entry.get("has_indirect_control_flow"))
        and bool(
            reason_tags.intersection(
                {"heuristic_pointer_alias", "heuristic_local_surface", "slot_alias_candidate"}
            )
        )
    )


def inventory_surface_gap(entry: dict[str, Any]) -> bool:
    return bool(entry.get("inventory_surface_gap"))


def candidate_passes_explicit_quality_prefilter(
    entry: dict[str, Any],
    source_meta: dict[str, Any] | None = None,
) -> bool:
    return (
        source_is_preview_aligned(source_meta)
        and explicit_fact_total(entry) >= 2
        and bool(entry.get("preview_direct_success"))
        and not bool(entry.get("has_indirect_control_flow"))
        and int(entry.get("pcode_op_count", 0) or 0) <= 800
    )


def candidate_passes_heuristic_quality_prefilter(entry: dict[str, Any]) -> bool:
    return heuristic_surface_candidate(entry)


def aligned_explicit_candidate_entry(
    entry: dict[str, Any],
    source_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "path": entry.get("binary_path"),
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "explicit_fact_total": explicit_fact_total(entry),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "preview_direct_success": bool(entry.get("preview_direct_success")),
        "preview_fallback_kind_refined": entry.get("preview_fallback_kind_refined"),
        "pcode_op_count": int(entry.get("pcode_op_count", 0) or 0),
        "preview_surface_kind": entry.get("preview_surface_kind"),
        "fact_sources_present": dict(entry.get("fact_sources_present") or {}),
        "explicit_fact_breakdown": dict(entry.get("explicit_fact_breakdown") or {}),
        "admission_block_stage": entry.get("admission_block_stage"),
        "inventory_surface_gap": inventory_surface_gap(entry),
        "reason_tags": list(entry.get("reason_tags") or []),
        "source_binary": source_meta.get("binary") if source_meta else entry.get("binary"),
        "source_admission_alignment": source_meta.get("admission_alignment") if source_meta else None,
    }


def blocked_explicit_candidate_entry(
    entry: dict[str, Any],
    source_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "path": entry.get("binary_path"),
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "explicit_fact_total": explicit_fact_total(entry),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "preview_direct_success": bool(entry.get("preview_direct_success")),
        "block_reason": entry.get("row_error_kind")
        or entry.get("preview_fallback_kind_refined")
        or "strict_filter_reject",
        "pcode_op_count": int(entry.get("pcode_op_count", 0) or 0),
        "has_indirect_control_flow": bool(entry.get("has_indirect_control_flow")),
        "fact_sources_present": dict(entry.get("fact_sources_present") or {}),
        "explicit_fact_breakdown": dict(entry.get("explicit_fact_breakdown") or {}),
        "admission_block_stage": entry.get("admission_block_stage"),
        "inventory_surface_gap": inventory_surface_gap(entry),
        "reason_tags": list(entry.get("reason_tags") or []),
        "source_admission_alignment": source_meta.get("admission_alignment") if source_meta else None,
    }


def candidate_sort_key(entry: dict[str, Any]) -> tuple[int, int, int, int]:
    return (
        int(entry.get("fact_density_score", 0) or 0),
        explicit_fact_total(entry),
        1 if entry.get("preview_direct_success") else 0,
        -int(entry.get("pcode_op_count", 0) or 0),
    )


def curated_quality_entry(entry: dict[str, Any]) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "quality_potential_score": int(entry.get("fact_density_score", 0) or 0),
        "reason_tags": list(entry.get("reason_tags") or []),
    }
