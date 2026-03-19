from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

from .metrics import normalize_address


def run_candidate_inventory(
    root_dir: Path,
    binary_path: Path,
    fission_bin: Path,
    *,
    timeout_ms: int | None = None,
    limit: int | None = None,
    address: str | None = None,
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        str(binary_path),
        "--preview-candidate-inventory",
        "--json",
    ]
    if timeout_ms is not None:
        cmd.extend(["--timeout-ms", str(timeout_ms)])
    if limit is not None:
        cmd.extend(["--preview-candidate-limit", str(limit)])
    if address:
        cmd.extend(["--address", address])
    res = subprocess.run(
        cmd,
        cwd=root_dir,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    return json.loads(res.stdout)


def explicit_fact_total(entry: dict[str, Any]) -> int:
    return (
        int(entry.get("dwarf_param_count", 0))
        + int(entry.get("dwarf_local_count", 0))
        + (1 if entry.get("has_dwarf_return_type") else 0)
    )


def preview_hint_total(entry: dict[str, Any]) -> int:
    stats = entry.get("preview_hint_stats") or {}
    return sum(int(value or 0) for value in stats.values())


def source_is_preview_aligned(source_meta: dict[str, Any] | None) -> bool:
    if not source_meta:
        return True
    if not bool(source_meta.get("expected_preview_supported", True)):
        return False
    failure_kind = source_meta.get("observed_preview_failure_kind")
    if failure_kind in {"preview_architecture_unsupported", "preview_format_unsupported"}:
        return False
    return True


def candidate_passes_explicit_quality_prefilter(
    entry: dict[str, Any],
    source_meta: dict[str, Any] | None = None,
) -> bool:
    return (
        source_is_preview_aligned(source_meta)
        and
        explicit_fact_total(entry) >= 2
        and bool(entry.get("preview_direct_success"))
        and not bool(entry.get("has_indirect_control_flow"))
        and int(entry.get("pcode_op_count", 0) or 0) <= 800
    )


def candidate_passes_heuristic_quality_prefilter(entry: dict[str, Any]) -> bool:
    reason_tags = set(entry.get("reason_tags") or [])
    return (
        bool(entry.get("preview_direct_success"))
        and not bool(entry.get("has_indirect_control_flow"))
        and (
            preview_hint_total(entry) > 0
            or bool(
                reason_tags.intersection(
                    {"heuristic_pointer_alias", "heuristic_local_surface", "slot_alias_candidate"}
                )
            )
        )
    )


def curated_quality_entry(entry: dict[str, Any]) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "quality_potential_score": int(entry.get("quality_potential_score", 0) or 0),
        "reason_tags": list(entry.get("reason_tags") or []),
    }
