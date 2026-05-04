#!/usr/bin/env python3
"""Run the Stage Parity Benchmark over a manifest."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
FULL_BENCHMARK_DIR = ROOT / "benchmark" / "full_benchmark"

sys.path.insert(0, str(FULL_BENCHMARK_DIR))

from grand_finale_support.runners import run_fission_function
from grand_finale_support.metrics import normalize_address

from stage_metrics import build_stage_report


def parse_int(value: str) -> int:
    return int(value, 0)


def merge_defaults(defaults: dict[str, Any], values: dict[str, Any]) -> dict[str, Any]:
    merged = dict(defaults)
    merged.update(values)
    return merged


def expand_manifest_rows(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    defaults = manifest.get("defaults", {})
    if defaults is None:
        defaults = {}
    if not isinstance(defaults, dict):
        raise SystemExit("manifest defaults must be an object")

    expanded: list[dict[str, Any]] = []
    for row in manifest.get("rows", []):
        if not isinstance(row, dict):
            raise SystemExit("manifest rows[] entries must be objects")
        expanded.append(merge_defaults(defaults, row))

    binary_ids: set[str] = set()
    for binary in manifest.get("binaries", []):
        if not isinstance(binary, dict):
            raise SystemExit("manifest binaries[] entries must be objects")
        binary_id = str(binary.get("id") or binary.get("name") or "").strip()
        if not binary_id:
            raise SystemExit("manifest binaries[] entry is missing id")
        if binary_id in binary_ids:
            raise SystemExit(f"duplicate binary id {binary_id!r}")
        binary_ids.add(binary_id)

        binary_rows = binary.get("rows", [])
        if not isinstance(binary_rows, list):
            raise SystemExit(f"binary {binary_id!r} rows must be an array")

        binary_defaults = merge_defaults(defaults, {k: v for k, v in binary.items() if k != "rows"})
        binary_path = binary_defaults.get("binary") or binary_defaults.get("path")
        if not binary_path:
            raise SystemExit(f"binary {binary_id!r} is missing path")

        for row in binary_rows:
            if not isinstance(row, dict):
                raise SystemExit(f"binary {binary_id!r} has a non-object row")
            row_name = str(row.get("name") or row.get("addr") or len(expanded)).strip()
            if not row_name:
                raise SystemExit(f"binary {binary_id!r} has a row without name or addr")
            expanded_row = merge_defaults(binary_defaults, row)
            expanded_row["binary"] = str(binary_path)
            expanded_row["binary_id"] = binary_id
            expanded_row["name"] = str(row.get("qualified_name") or f"{binary_id}-{row_name}")
            expanded.append(expanded_row)

    return expanded


def increment_bucket_totals(totals: dict[str, int], buckets: list[str]) -> None:
    for bucket in buckets:
        totals[bucket] = totals.get(bucket, 0) + 1


def run_row(
    row: dict[str, Any],
    *,
    output_dir: Path,
    fission_bin: Path,
    timeout_sec: int,
    engine: str,
) -> dict[str, Any]:
    binary = Path(row.get("binary", ""))
    if not binary:
        raise SystemExit(f"row {row.get('name', '<unnamed>')} is missing binary")
    binary = binary if binary.is_absolute() else ROOT / binary

    addr = parse_int(str(row.get("addr") or row.get("address") or "0"))
    address = normalize_address(hex(addr))
    row_dir = output_dir / str(row.get("name") or address)
    row_dir.mkdir(parents=True, exist_ok=True)

    entry = run_fission_function(
        ROOT,
        binary,
        address,
        fission_bin,
        timeout_sec,
        struct_ptr_aliases={},
        engine=engine,
    )
    stage_report = build_stage_report(entry)

    report = {
        "name": row.get("name"),
        "binary_id": row.get("binary_id"),
        "binary": str(binary),
        "address": address,
        "feature_group": row.get("feature_group"),
        "feature": row.get("feature"),
        "owner": row.get("owner"),
        "notes": row.get("notes"),
        "fission": {
            "success": entry.get("success", False),
            "failure_kind": entry.get("failure_kind"),
            "failure_detail": entry.get("failure_detail"),
            "engine_used": entry.get("engine_used"),
            "fell_back": entry.get("fell_back", False),
            "fallback_reason": entry.get("fallback_reason"),
            "preview_build_stats": entry.get("preview_build_stats"),
        },
        "code_metrics": entry.get("metrics", {}),
        "stages": stage_report,
    }

    report_path = row_dir / "stage_parity_report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    report["report"] = str(report_path)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/stage_parity_benchmark/latest")
    parser.add_argument("--fission-bin", type=Path, default=ROOT / "target" / "release" / "fission_cli")
    parser.add_argument("--timeout-sec", type=int, default=120)
    parser.add_argument("--engine", default="auto")
    parser.add_argument("--row", help="Run one named manifest row")
    parser.add_argument("--binary-id", help="Run only rows belonging to one binaries[] manifest id")
    parser.add_argument("--feature", help="Run only rows matching one feature tag from the manifest")
    parser.add_argument("--group", help="Run only rows matching one feature_group tag from the manifest")
    parser.add_argument("--max-rows-per-binary", type=int, default=None)
    args = parser.parse_args()

    if not args.fission_bin.exists():
        raise SystemExit(
            f"fission_cli release binary not found at {args.fission_bin}. "
            "Run `cargo build -p fission-cli --release` first."
        )

    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    manifest = json.loads(manifest_path.read_text())
    rows = expand_manifest_rows(manifest)

    if args.row:
        rows = [row for row in rows if row.get("name") == args.row]
        if not rows:
            raise SystemExit(f"no row named {args.row!r} in {manifest_path}")
    if args.binary_id:
        rows = [row for row in rows if row.get("binary_id") == args.binary_id]
        if not rows:
            raise SystemExit(f"no row with binary_id {args.binary_id!r} in {manifest_path}")
    if args.feature:
        rows = [row for row in rows if row.get("feature") == args.feature]
        if not rows:
            raise SystemExit(f"no row with feature {args.feature!r} in {manifest_path}")
    if args.group:
        rows = [row for row in rows if row.get("feature_group") == args.group]
        if not rows:
            raise SystemExit(f"no row with feature_group {args.group!r} in {manifest_path}")
    if args.max_rows_per_binary is not None:
        if args.max_rows_per_binary < 0:
            raise SystemExit("--max-rows-per-binary must be >= 0")
        limited_rows: list[dict[str, Any]] = []
        seen_by_binary: dict[str, int] = {}
        for row in rows:
            binary_key = str(row.get("binary_id") or row.get("binary") or "")
            seen = seen_by_binary.get(binary_key, 0)
            if seen >= args.max_rows_per_binary:
                continue
            seen_by_binary[binary_key] = seen + 1
            limited_rows.append(row)
        rows = limited_rows

    output_dir = args.output_dir if args.output_dir.is_absolute() else ROOT / args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    owner_bucket_totals: dict[str, int] = {}
    stage_status_totals: dict[str, int] = {}
    row_reports = []
    for row in rows:
        report = run_row(
            row,
            output_dir=output_dir,
            fission_bin=args.fission_bin,
            timeout_sec=args.timeout_sec,
            engine=args.engine,
        )
        row_reports.append(report)
        for bucket in report.get("stages", {}).get("owner_bucket", []):
            owner_bucket_totals[bucket] = owner_bucket_totals.get(bucket, 0) + 1
        for stage, status in report.get("stages", {}).get("status", {}).items():
            key = f"{stage}_{status}"
            stage_status_totals[key] = stage_status_totals.get(key, 0) + 1

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(row_reports),
        "owner_bucket_totals": dict(sorted(owner_bucket_totals.items())),
        "stage_status_totals": dict(sorted(stage_status_totals.items())),
        "rows": row_reports,
    }
    aggregate_path = output_dir / "aggregate_stage_parity_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "report": str(aggregate_path),
        "row_count": aggregate["row_count"],
        "owner_bucket_totals": aggregate["owner_bucket_totals"],
        "stage_status_totals": aggregate["stage_status_totals"],
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
