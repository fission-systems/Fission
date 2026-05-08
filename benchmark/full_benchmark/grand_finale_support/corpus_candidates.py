from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

from common.batch_scan import run_preview_candidate_scan_batch_report

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
    if address or limit is not None:
        try:
            rows, summary = run_preview_candidate_scan_batch_report(
                root_dir,
                binary_path,
                fission_bin,
                addresses=[address] if address else None,
                functions_limit=None if address else limit,
                chunk_size=max(1, min(limit or 50, 50)),
                inventory_timeout_ms=timeout_ms or 10000,
                subprocess_timeout_sec=max(60, int((timeout_ms or 10000) / 1000) + 10),
            )
            return {
                "binary": binary_path.stem,
                "binary_path": str(binary_path),
                "candidate_count": len(rows),
                "candidates": rows,
                "summary": summary,
                "scan_mode": "rust_batch",
            }
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, json.JSONDecodeError, OSError):
            pass

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
    report = json.loads(res.stdout)
    report["scan_mode"] = "legacy_inventory"
    return report


def explicit_fact_total(entry: dict[str, Any]) -> int:
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


def aligned_explicit_candidate_entry(
    entry: dict[str, Any],
    source_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "path": entry.get("path"),
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "explicit_fact_total": explicit_fact_total(entry),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "preview_direct_success": bool(entry.get("preview_direct_success")),
        "pcode_op_count": int(entry.get("pcode_op_count", 0) or 0),
        "preview_surface_kind": entry.get("preview_surface_kind"),
        "reason_tags": list(entry.get("reason_tags") or []),
        "source_binary": source_meta.get("binary") if source_meta else entry.get("binary"),
        "source_admission_alignment": source_meta.get("admission_alignment") if source_meta else None,
        "source_rescan_priority": source_meta.get("rescan_priority") if source_meta else None,
    }


def curated_quality_entry(entry: dict[str, Any]) -> dict[str, Any]:
    return {
        "binary": entry["binary"],
        "address": f"0x{normalize_address(entry['address'])}",
        "name": entry.get("name", ""),
        "fact_density_score": int(entry.get("fact_density_score", 0) or 0),
        "quality_potential_score": int(entry.get("quality_potential_score", 0) or 0),
        "reason_tags": list(entry.get("reason_tags") or []),
    }
