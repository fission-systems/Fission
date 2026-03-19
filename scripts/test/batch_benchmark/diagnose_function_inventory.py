#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from grand_finale_support.corpus_candidates import (
    aligned_explicit_candidate_entry,
    blocked_explicit_candidate_entry,
    candidate_passes_explicit_quality_prefilter,
    explicit_fact_total,
)
from grand_finale_support.inventory_reader import (
    load_source_inventory,
    run_function_facts_inventory,
)


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_DIAGNOSIS_JSON = (
    ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "inventory_diagnosis.json"
)
DEFAULT_MARKDOWN_SUMMARY = (
    ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "inventory_diagnosis.md"
)


def default_source_inventory_path() -> Path:
    candidates = [
        ROOT_DIR
        / "artifacts"
        / "batch_benchmark_scripts"
        / "corpora"
        / "preview_explicit_source_inventory.json",
        ROOT_DIR
        / "scripts"
        / "test"
        / "batch_benchmark"
        / "corpora"
        / "preview_explicit_source_inventory.json",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return candidates[0]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Diagnose whole-binary function inventory to find explicit-facts bottlenecks."
    )
    parser.add_argument("binaries", nargs="*", help="Aligned source binaries to diagnose")
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION_BIN)
    parser.add_argument("--source-inventory-file", type=Path, default=default_source_inventory_path())
    parser.add_argument("--output-json", type=Path, default=DEFAULT_DIAGNOSIS_JSON)
    parser.add_argument("--markdown-summary", type=Path, default=DEFAULT_MARKDOWN_SUMMARY)
    parser.add_argument("--functions-limit", type=int)
    parser.add_argument("--chunk-size", type=int, default=100)
    parser.add_argument("--timeout-ms", type=int, default=10000)
    parser.add_argument("--aligned-limit", type=int, default=5)
    parser.add_argument(
        "--no-markdown",
        action="store_true",
        help="Skip markdown summary emission",
    )
    return parser.parse_args()


def priority_rank(value: str | None) -> int:
    return {"high": 0, "medium": 1, "low": 2}.get((value or "").lower(), 3)


def resolve_binary_targets(
    binary_args: list[str],
    source_inventory: dict[str, dict[str, Any]],
    aligned_limit: int,
) -> list[Path]:
    if binary_args:
        return [Path(item).resolve() for item in binary_args]

    seen: set[str] = set()
    selected: list[tuple[tuple[int, str], Path]] = []
    for source in source_inventory.values():
        if not isinstance(source, dict):
            continue
        path = source.get("path")
        if not path:
            continue
        if source.get("admission_alignment") != "aligned":
            continue
        resolved = str(Path(path).resolve())
        if resolved in seen:
            continue
        seen.add(resolved)
        selected.append(
            ((priority_rank(source.get("rescan_priority")), source.get("binary", resolved)), Path(resolved))
        )
    selected.sort(key=lambda item: item[0])
    return [path for _, path in selected[:aligned_limit]]


def source_meta_for_binary(
    source_inventory: dict[str, dict[str, Any]],
    binary_path: Path,
) -> dict[str, Any] | None:
    return (
        source_inventory.get(str(binary_path.resolve()))
        or source_inventory.get(binary_path.name)
        or source_inventory.get(binary_path.stem)
    )


def count_rows_with_any_source(rows: list[dict[str, Any]]) -> int:
    total = 0
    for row in rows:
        sources = row.get("fact_sources_present") or {}
        if any(bool(value) for value in sources.values()):
            total += 1
    return total


def stage_counts(entries: list[dict[str, Any]]) -> dict[str, int]:
    counts: Counter[str] = Counter()
    for entry in entries:
        counts[str(entry.get("admission_block_stage") or "none")] += 1
    return dict(sorted(counts.items()))


def classify_diagnosis(
    *,
    rows_emitted: int,
    source_present_rows: int,
    explicit_nonzero_rows: int,
    inventory_surface_gap_count: int,
    blocked_stage_counts: dict[str, int],
    source_presence_counts: dict[str, int],
    provenance_surface_totals: dict[str, int],
) -> tuple[str, str, str]:
    rows = max(rows_emitted, 1)
    source_presence_ratio = source_present_rows / rows
    explicit_nonzero_ratio = explicit_nonzero_rows / rows
    surface_gap_ratio = inventory_surface_gap_count / rows
    preview_stage_blocks = blocked_stage_counts.get("preview", 0) + blocked_stage_counts.get(
        "admission", 0
    )
    pdb_sources = int(source_presence_counts.get("pdb", 0) or 0)
    pdb_surface_rows = int(provenance_surface_totals.get("pdb_nonzero_rows", 0) or 0)
    native_surface_rows = int(provenance_surface_totals.get("native_nonzero_rows", 0) or 0)

    if pdb_sources > 0 and pdb_surface_rows == 0 and native_surface_rows > 0:
        return (
            "mixed_or_inconclusive",
            "factstore_inventory_patch",
            "PDB source presence is visible, but surfaced explicit rows are still being supplied by native inferred facts instead of PDB-derived facts",
        )

    if explicit_nonzero_rows > 0 and preview_stage_blocks > 0:
        return (
            "preview_stage_block",
            "preview_side_patch",
            "explicit facts exist, but blocked candidates concentrate in preview/admission stages",
        )
    if source_present_rows > 0 and explicit_nonzero_rows == 0 and inventory_surface_gap_count > 0:
        return (
            "factstore_or_inventory_surface_gap",
            "factstore_inventory_patch",
            "source provenance is present while explicit rows stay at zero and inventory surface gaps are observed",
        )
    if source_present_rows == 0 and explicit_nonzero_rows == 0 and inventory_surface_gap_count == 0:
        return (
            "source_facts_absent",
            "source_expansion",
            "no source provenance or explicit rows were observed in the inventory",
        )
    if source_presence_ratio <= 0.05 and explicit_nonzero_ratio == 0 and surface_gap_ratio == 0:
        return (
            "source_facts_absent",
            "source_expansion",
            "source provenance coverage is negligible and explicit rows remain absent",
        )
    if inventory_surface_gap_count > 0 and explicit_nonzero_rows == 0:
        return (
            "factstore_or_inventory_surface_gap",
            "factstore_inventory_patch",
            "provenance reaches inventory rows, but explicit surfacing is still absent",
        )
    if preview_stage_blocks > 0:
        return (
            "preview_stage_block",
            "preview_side_patch",
            "blocked explicit candidates cluster in preview/admission stages",
        )
    return (
        "mixed_or_inconclusive",
        "source_expansion" if source_present_rows == 0 else "factstore_inventory_patch",
        "inventory does not show a single dominant bottleneck yet",
    )


def diagnosis_entry(
    binary_path: Path,
    rows: list[dict[str, Any]],
    summary: dict[str, Any],
    source_meta: dict[str, Any] | None,
) -> dict[str, Any]:
    aligned_candidates = [
        aligned_explicit_candidate_entry(row, source_meta)
        for row in rows
        if source_meta and source_meta.get("admission_alignment") == "aligned"
    ]
    blocked_candidates = [
        blocked_explicit_candidate_entry(row, source_meta)
        for row in rows
        if explicit_fact_total(row) > 0
        and not candidate_passes_explicit_quality_prefilter(row, source_meta)
    ]

    rows_emitted = int(summary.get("rows_emitted", len(rows)) or len(rows))
    explicit_nonzero_rows = int(summary.get("explicit_fact_nonzero_count", 0) or 0)
    inventory_surface_gap_count = int(summary.get("inventory_surface_gap_count", 0) or 0)
    aligned_with_zero_explicit_count = int(summary.get("aligned_with_zero_explicit_count", 0) or 0)
    source_present_rows = count_rows_with_any_source(rows)
    blocked_admission_stage_counts = stage_counts(blocked_candidates)
    source_presence_counts = dict(summary.get("source_presence_counts") or {})
    provenance_surface_totals = dict(summary.get("provenance_surface_totals") or {})
    diagnosis_bucket, next_action, rationale = classify_diagnosis(
        rows_emitted=rows_emitted,
        source_present_rows=source_present_rows,
        explicit_nonzero_rows=explicit_nonzero_rows,
        inventory_surface_gap_count=inventory_surface_gap_count,
        blocked_stage_counts=blocked_admission_stage_counts,
        source_presence_counts=source_presence_counts,
        provenance_surface_totals=provenance_surface_totals,
    )

    return {
        "binary": binary_path.name,
        "binary_path": str(binary_path),
        "source_meta": {
            "admission_alignment": source_meta.get("admission_alignment") if source_meta else None,
            "rescan_priority": source_meta.get("rescan_priority") if source_meta else None,
            "expected_preview_supported": source_meta.get("expected_preview_supported")
            if source_meta
            else None,
            "observed_preview_supported": source_meta.get("observed_preview_supported")
            if source_meta
            else None,
            "observed_preview_failure_kind": source_meta.get("observed_preview_failure_kind")
            if source_meta
            else None,
        },
        "inventory_summary": summary,
        "derived_metrics": {
            "source_present_rows": source_present_rows,
            "explicit_nonzero_rows": explicit_nonzero_rows,
            "inventory_surface_gap_count": inventory_surface_gap_count,
            "aligned_with_zero_explicit_count": aligned_with_zero_explicit_count,
            "aligned_candidate_count": len(aligned_candidates),
            "blocked_candidate_count": len(blocked_candidates),
            "blocked_admission_stage_counts": blocked_admission_stage_counts,
            "source_presence_counts": source_presence_counts,
            "provenance_surface_totals": provenance_surface_totals,
            "pdb_source_without_pdb_surface": bool(
                int(source_presence_counts.get("pdb", 0) or 0) > 0
                and int(provenance_surface_totals.get("pdb_nonzero_rows", 0) or 0) == 0
            ),
        },
        "diagnosis_bucket": diagnosis_bucket,
        "next_action": next_action,
        "diagnosis_rationale": rationale,
        "top_aligned_candidates": sorted(
            aligned_candidates,
            key=lambda item: (
                int(item.get("explicit_fact_total", 0) or 0),
                int(item.get("fact_density_score", 0) or 0),
                -int(item.get("pcode_op_count", 0) or 0),
            ),
            reverse=True,
        )[:5],
        "blocked_candidates": sorted(
            blocked_candidates,
            key=lambda item: (
                int(item.get("explicit_fact_total", 0) or 0),
                int(item.get("fact_density_score", 0) or 0),
                -int(item.get("pcode_op_count", 0) or 0),
            ),
            reverse=True,
        )[:5],
    }


def aggregate_diagnosis(entries: list[dict[str, Any]]) -> dict[str, Any]:
    bucket_counts: Counter[str] = Counter()
    next_action_counts: Counter[str] = Counter()
    for entry in entries:
        bucket_counts[entry["diagnosis_bucket"]] += 1
        next_action_counts[entry["next_action"]] += 1

    dominant_bucket = None
    dominant_next_action = None
    if bucket_counts:
        dominant_bucket = sorted(bucket_counts.items(), key=lambda item: (-item[1], item[0]))[0][0]
    if next_action_counts:
        dominant_next_action = sorted(next_action_counts.items(), key=lambda item: (-item[1], item[0]))[0][0]

    return {
        "diagnosis_bucket_counts": dict(sorted(bucket_counts.items())),
        "next_action_counts": dict(sorted(next_action_counts.items())),
        "dominant_diagnosis": dominant_bucket,
        "recommended_next_patch": dominant_next_action,
    }


def markdown_summary(report: dict[str, Any]) -> str:
    lines = ["# Inventory Diagnosis", ""]
    aggregate = report.get("aggregate", {})
    lines.append("## Aggregate")
    lines.append("")
    lines.append(f"- Dominant diagnosis: `{aggregate.get('dominant_diagnosis')}`")
    lines.append(f"- Recommended next patch: `{aggregate.get('recommended_next_patch')}`")
    lines.append(f"- Diagnosis bucket counts: `{aggregate.get('diagnosis_bucket_counts', {})}`")
    lines.append("")
    lines.append("## Binaries")
    lines.append("")
    for entry in report.get("binaries", []):
        metrics = entry.get("derived_metrics", {})
        lines.append(f"### {entry['binary']}")
        lines.append("")
        lines.append(f"- Diagnosis: `{entry['diagnosis_bucket']}`")
        lines.append(f"- Next action: `{entry['next_action']}`")
        lines.append(f"- Rationale: {entry['diagnosis_rationale']}")
        lines.append(
            f"- Source-present rows: `{metrics.get('source_present_rows')}`, explicit-nonzero rows: `{metrics.get('explicit_nonzero_rows')}`, surface-gap rows: `{metrics.get('inventory_surface_gap_count')}`"
        )
        lines.append(
            f"- Source presence counts: `{metrics.get('source_presence_counts', {})}`, provenance surface totals: `{metrics.get('provenance_surface_totals', {})}`"
        )
        lines.append(
            f"- Blocked admission stages: `{metrics.get('blocked_admission_stage_counts', {})}`"
        )
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")

    source_inventory = load_source_inventory(args.source_inventory_file)
    binary_paths = resolve_binary_targets(args.binaries, source_inventory, args.aligned_limit)
    if not binary_paths:
        raise SystemExit("no binaries selected for diagnosis")

    diagnosis_entries: list[dict[str, Any]] = []
    for binary_path in binary_paths:
        rows, summary = run_function_facts_inventory(
            ROOT_DIR,
            binary_path,
            args.fission_bin,
            timeout_ms=args.timeout_ms,
            functions_limit=args.functions_limit,
            chunk_size=args.chunk_size,
            quiet_batch_errors=True,
        )
        source_meta = source_meta_for_binary(source_inventory, binary_path)
        diagnosis_entries.append(diagnosis_entry(binary_path, rows, summary, source_meta))

    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_inventory_file": str(args.source_inventory_file),
        "binaries": diagnosis_entries,
        "aggregate": aggregate_diagnosis(diagnosis_entries),
    }

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(report, indent=2))
    print(f"[+] Wrote inventory diagnosis JSON to {args.output_json}")

    if not args.no_markdown:
        args.markdown_summary.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_summary.write_text(markdown_summary(report))
        print(f"[+] Wrote inventory diagnosis Markdown to {args.markdown_summary}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
