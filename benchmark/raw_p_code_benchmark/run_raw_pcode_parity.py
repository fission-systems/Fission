#!/usr/bin/env python3
"""Run Ghidra-vs-Fission raw instruction p-code parity."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def parse_int(value: str) -> int:
    return int(value, 0)


def run(cmd: list[str]) -> None:
    subprocess.run(cmd, cwd=ROOT, check=True)


def merge_defaults(defaults: dict[str, Any], values: dict[str, Any]) -> dict[str, Any]:
    merged = dict(defaults)
    merged.update(values)
    return merged


def expand_manifest_rows(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    """Return flat benchmark rows from either rows[] or binaries[] manifests.

    The legacy rows[] shape remains the canonical representation used by the
    runner. The binaries[] shape is a convenience layer for real-world suites:
    binary-level metadata is inherited by every row and then flattened before
    any filtering or execution happens.
    """
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


def increment_nested_total(
    totals: dict[str, dict[str, int]],
    key: str,
    buckets: dict[str, Any],
) -> None:
    nested = totals.setdefault(key, {})
    for bucket, count in buckets.items():
        nested[bucket] = nested.get(bucket, 0) + int(count)


def add_performance_totals(
    totals: dict[str, float],
    performance: dict[str, Any],
    *,
    prefix: str,
) -> None:
    wall_clock_sec = performance.get("wall_clock_sec")
    instruction_count = performance.get("instruction_count")
    pcode_op_count = performance.get("pcode_op_count")
    for key in (
        "process_startup_sec",
        "frontend_load_sec",
        "decode_lift_sec",
        "binary_load_sec",
        "rust_probe_sec",
    ):
        value = performance.get(key)
        if value is not None:
            totals[f"{prefix}_{key}"] = totals.get(f"{prefix}_{key}", 0.0) + float(value)
    if wall_clock_sec is not None:
        totals[f"{prefix}_wall_clock_sec"] = totals.get(f"{prefix}_wall_clock_sec", 0.0) + float(wall_clock_sec)
    if instruction_count is not None:
        totals[f"{prefix}_instruction_count"] = totals.get(f"{prefix}_instruction_count", 0.0) + float(instruction_count)
    if pcode_op_count is not None:
        totals[f"{prefix}_pcode_op_count"] = totals.get(f"{prefix}_pcode_op_count", 0.0) + float(pcode_op_count)


def finalize_performance_summary(totals: dict[str, float]) -> dict[str, Any]:
    ghidra_wall = totals.get("ghidra_wall_clock_sec", 0.0)
    fission_wall = totals.get("fission_wall_clock_sec", 0.0)
    ghidra_instructions = totals.get("ghidra_instruction_count", 0.0)
    fission_instructions = totals.get("fission_instruction_count", 0.0)
    ghidra_ops = totals.get("ghidra_pcode_op_count", 0.0)
    fission_ops = totals.get("fission_pcode_op_count", 0.0)

    summary = {
        "ghidra": {
            "wall_clock_sec": ghidra_wall,
            "instruction_count": int(ghidra_instructions),
            "pcode_op_count": int(ghidra_ops),
            "instructions_per_sec": ghidra_instructions / ghidra_wall if ghidra_wall > 0 else None,
            "pcode_ops_per_sec": ghidra_ops / ghidra_wall if ghidra_wall > 0 else None,
        },
        "fission": {
            "wall_clock_sec": fission_wall,
            "process_startup_sec": totals.get("fission_process_startup_sec"),
            "frontend_load_sec": totals.get("fission_frontend_load_sec"),
            "decode_lift_sec": totals.get("fission_decode_lift_sec"),
            "binary_load_sec": totals.get("fission_binary_load_sec"),
            "rust_probe_sec": totals.get("fission_rust_probe_sec"),
            "instruction_count": int(fission_instructions),
            "pcode_op_count": int(fission_ops),
            "instructions_per_sec": fission_instructions / fission_wall if fission_wall > 0 else None,
            "pcode_ops_per_sec": fission_ops / fission_wall if fission_wall > 0 else None,
            "decode_lift_instructions_per_sec": (
                fission_instructions / totals["fission_decode_lift_sec"]
                if totals.get("fission_decode_lift_sec", 0.0) > 0
                else None
            ),
            "decode_lift_pcode_ops_per_sec": (
                fission_ops / totals["fission_decode_lift_sec"]
                if totals.get("fission_decode_lift_sec", 0.0) > 0
                else None
            ),
        },
        "delta": {
            "wall_clock_delta_sec": fission_wall - ghidra_wall,
            "wall_clock_speedup_fission_over_ghidra": ghidra_wall / fission_wall if fission_wall > 0 else None,
            "instruction_throughput_ratio_fission_over_ghidra": (
                (fission_instructions / fission_wall) / (ghidra_instructions / ghidra_wall)
                if ghidra_wall > 0 and fission_wall > 0 and ghidra_instructions > 0
                else None
            ),
            "pcode_throughput_ratio_fission_over_ghidra": (
                (fission_ops / fission_wall) / (ghidra_ops / ghidra_wall)
                if ghidra_wall > 0 and fission_wall > 0 and ghidra_ops > 0
                else None
            ),
        },
    }
    return summary


def enforce_perfect_canonical_gate(
    aggregate: dict[str, Any],
    *,
    expected_full_match: int | None,
) -> list[str]:
    """Return gate failures for the canonical x86-64 raw p-code parity lane.

    This gate intentionally ignores rows already classified as
    both_decode_error_or_padding. Those rows stay visible in bucket totals, but
    they are not semantic parity rows. Everything else must be a full match
    sourced from decoded .sla ConstructTpl execution.
    """
    failures: list[str] = []
    buckets = aggregate.get("bucket_totals", {})
    invariants = aggregate.get("invariant_totals", {})
    similarity = aggregate.get("similarity_summary", {})
    template_sources = aggregate.get("template_source_totals", {})

    if similarity.get("average_similarity_score") != 1.0:
        failures.append(
            "average_similarity_score must be 1.0 "
            f"(got {similarity.get('average_similarity_score')!r})"
        )
    if similarity.get("average_parity_ratio") != 1.0:
        failures.append(
            "average_parity_ratio must be 1.0 "
            f"(got {similarity.get('average_parity_ratio')!r})"
        )
    if expected_full_match is not None and buckets.get("full_match", 0) != expected_full_match:
        failures.append(
            f"full_match must be {expected_full_match} "
            f"(got {buckets.get('full_match', 0)})"
        )

    for key in ("compat_emitter_used", "fake_placeholder_op", "invalid_pcode_shape"):
        if int(invariants.get(key, 0)) != 0:
            failures.append(f"{key} must be 0 (got {invariants.get(key)})")

    allowed_nonsemantic = {
        "full_match",
        "both_decode_error_or_padding",
        "ghidra_decode_error",
        "oracle_no_instruction",
    }
    for bucket, count in sorted(buckets.items()):
        if bucket not in allowed_nonsemantic and int(count) != 0:
            failures.append(f"semantic mismatch bucket {bucket} must be 0 (got {count})")

    unexpected_sources = {
        source: count
        for source, count in template_sources.items()
        if source != "sla_construct_tpl" and int(count) != 0
    }
    if unexpected_sources:
        failures.append(
            "successful rows must use only sla_construct_tpl template source "
            f"(unexpected {unexpected_sources})"
        )

    return failures


def run_one(
    *,
    binary: Path,
    addr: int,
    count: int,
    language: str,
    compiler: str,
    ghidra_dir: Path | None,
    output_dir: Path,
    no_analyze: bool,
    disassemble_missing: bool,
    fission_release: bool,
) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    ghidra_json = output_dir / "ghidra_raw_pcode.json"
    fission_json = output_dir / "fission_raw_pcode.json"
    report_json = output_dir / "raw_pcode_parity_report.json"

    run(
        [
            "python3",
            str(THIS_DIR / "ghidra_raw_pcode.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--count",
            str(count),
            "--language",
            language,
            "--compiler",
            compiler,
            *(["--ghidra-dir", str(ghidra_dir)] if ghidra_dir is not None else []),
            "--output",
            str(ghidra_json),
            *(["--no-analyze"] if no_analyze else []),
            *(["--disassemble-missing"] if disassemble_missing else []),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "fission_raw_pcode.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--count",
            str(count),
            "--output",
            str(fission_json),
            *(["--release"] if fission_release else []),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "compare_raw_pcode.py"),
            "--ghidra",
            str(ghidra_json),
            "--fission",
            str(fission_json),
            "--output",
            str(report_json),
        ]
    )
    report = json.loads(report_json.read_text())
    report["report"] = str(report_json)
    return report


def run_manifest(args: argparse.Namespace) -> int:
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
    if args.language_filter:
        rows = [row for row in rows if row.get("language", args.language) == args.language_filter]
        if not rows:
            raise SystemExit(f"no row with language {args.language_filter!r} in {manifest_path}")
    if args.feature:
        rows = [row for row in rows if row.get("feature") == args.feature]
        if not rows:
            raise SystemExit(f"no row with feature {args.feature!r} in {manifest_path}")
    if args.group:
        rows = [row for row in rows if row.get("feature_group") == args.group]
        if not rows:
            raise SystemExit(f"no row with feature_group {args.group!r} in {manifest_path}")
    if args.max_rows_per_binary is not None:
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

    aggregate_totals: dict[str, int] = {}
    owner_hint_totals: dict[str, int] = {}
    legacy_path_audit_totals: dict[str, int] = {}
    template_source_totals: dict[str, int] = {}
    compat_emitter_used_total = 0
    feature_totals: dict[str, dict[str, int]] = {}
    group_totals: dict[str, dict[str, int]] = {}
    binary_totals: dict[str, dict[str, int]] = {}
    language_totals: dict[str, dict[str, int]] = {}
    performance_totals: dict[str, float] = {}
    similarity_totals: dict[str, float] = {"sum_average_similarity_score": 0.0, "sum_parity_ratio": 0.0}
    similarity_component_totals: dict[str, float] = {}
    row_reports = []
    for row in rows:
        binary = Path(row.get("binary", args.binary or ""))
        if not binary:
            raise SystemExit(f"row {row.get('name', '<unnamed>')} is missing binary")
        binary = binary if binary.is_absolute() else ROOT / binary
        row_name = row.get("name", f"0x{parse_int(str(row['addr'])):x}")
        binary_id = row.get("binary_id") or str(binary)
        language = row.get("language", args.language)
        feature = row.get("feature", "unclassified")
        feature_group = row.get("feature_group", "ungrouped")
        output_dir = args.output_dir / row_name
        report = run_one(
            binary=binary,
            addr=parse_int(str(row["addr"])),
            count=int(row.get("count", args.count)),
            language=language,
            compiler=row.get("compiler", args.compiler),
            ghidra_dir=row.get("ghidra_dir")
            and Path(row["ghidra_dir"])
            or args.ghidra_dir,
            output_dir=output_dir,
            no_analyze=bool(row.get("no_analyze", args.no_analyze)),
            disassemble_missing=bool(
                row.get("disassemble_missing", args.disassemble_missing)
            ),
            fission_release=args.fission_release,
        )
        for bucket, count in report["bucket_totals"].items():
            aggregate_totals[bucket] = aggregate_totals.get(bucket, 0) + int(count)
            feature_bucket_totals = feature_totals.setdefault(feature, {})
            feature_bucket_totals[bucket] = feature_bucket_totals.get(bucket, 0) + int(count)
            group_bucket_totals = group_totals.setdefault(feature_group, {})
            group_bucket_totals[bucket] = group_bucket_totals.get(bucket, 0) + int(count)
        increment_nested_total(binary_totals, str(binary_id), report["bucket_totals"])
        increment_nested_total(language_totals, str(language), report["bucket_totals"])
        for hint, count in report.get("owner_hint_totals", {}).items():
            owner_hint_totals[hint] = owner_hint_totals.get(hint, 0) + int(count)
        for name, count in report.get("legacy_path_audit_totals", {}).items():
            legacy_path_audit_totals[name] = legacy_path_audit_totals.get(name, 0) + int(count)
        for entry in report.get("rows", []):
            if entry.get("compat_emitter_used"):
                compat_emitter_used_total += 1
            template_source = entry.get("template_source")
            if template_source:
                template_source_totals[template_source] = (
                    template_source_totals.get(template_source, 0) + 1
                )
        performance = report.get("performance", {})
        add_performance_totals(performance_totals, performance.get("ghidra", {}), prefix="ghidra")
        add_performance_totals(performance_totals, performance.get("fission", {}), prefix="fission")
        similarity = report.get("similarity_summary", {})
        similarity_totals["sum_average_similarity_score"] += similarity.get("average_similarity_score", 0.0)
        similarity_totals["sum_parity_ratio"] += similarity.get("parity_ratio", 0.0)
        for name, value in similarity.get("average_components", {}).items():
            similarity_component_totals[name] = similarity_component_totals.get(name, 0.0) + float(value)
        first_mismatch = next(
            (entry for entry in report["rows"] if entry.get("buckets") != ["full_match"]),
            None,
        )
        row_reports.append(
            {
                "name": row_name,
                "feature": feature,
                "feature_group": feature_group,
                "binary_id": binary_id,
                "owner": row.get("owner"),
                "notes": row.get("notes"),
                "binary": str(binary),
                "language": language,
                "addr": row["addr"],
                "report": report["report"],
                "total_instructions": report["total_instructions"],
                "bucket_totals": report["bucket_totals"],
                "legacy_path_audit_totals": report.get("legacy_path_audit_totals", {}),
                "similarity_summary": similarity,
                "performance": performance,
                "first_mismatch": first_mismatch,
            }
        )

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(row_reports),
        "binary_count": len({str(row["binary_id"]) for row in row_reports}),
        "language_count": len({str(row["language"]) for row in row_reports}),
        "bucket_totals": dict(sorted(aggregate_totals.items())),
        "compat_emitter_used_total": compat_emitter_used_total,
        "invariant_totals": {
            "compat_emitter_used": compat_emitter_used_total,
            "fake_placeholder_op": int(aggregate_totals.get("fake_placeholder_op", 0)),
            "invalid_pcode_shape": int(aggregate_totals.get("invalid_pcode_shape", 0)),
        },
        "owner_hint_totals": dict(sorted(owner_hint_totals.items())),
        "legacy_path_audit_totals": dict(sorted(legacy_path_audit_totals.items())),
        "template_source_totals": dict(sorted(template_source_totals.items())),
        "feature_totals": {
            feature: dict(sorted(buckets.items()))
            for feature, buckets in sorted(feature_totals.items())
        },
        "group_totals": {
            group: dict(sorted(buckets.items()))
            for group, buckets in sorted(group_totals.items())
        },
        "binary_totals": {
            binary: dict(sorted(buckets.items()))
            for binary, buckets in sorted(binary_totals.items())
        },
        "language_totals": {
            language: dict(sorted(buckets.items()))
            for language, buckets in sorted(language_totals.items())
        },
        "similarity_summary": {
            "average_similarity_score": similarity_totals["sum_average_similarity_score"] / len(rows) if rows else 0.0,
            "average_parity_ratio": similarity_totals["sum_parity_ratio"] / len(rows) if rows else 0.0,
            "average_components": {
                name: value / len(rows) if rows else 0.0
                for name, value in sorted(similarity_component_totals.items())
            },
        },
        "performance_summary": finalize_performance_summary(performance_totals),
        "rows": row_reports,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    aggregate_path = args.output_dir / "aggregate_raw_pcode_parity_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "report": str(aggregate_path),
        "row_count": aggregate["row_count"],
        "binary_count": aggregate["binary_count"],
        "bucket_totals": aggregate["bucket_totals"],
        "compat_emitter_used_total": aggregate["compat_emitter_used_total"],
        "invariant_totals": aggregate["invariant_totals"],
        "template_source_totals": aggregate["template_source_totals"],
        "similarity_summary": aggregate["similarity_summary"],
        "performance_summary": aggregate["performance_summary"],
        "feature_count": len(aggregate["feature_totals"]),
        "group_count": len(aggregate["group_totals"]),
        "language_count": len(aggregate["language_totals"]),
    }, indent=2, sort_keys=True))
    if args.require_perfect_canonical:
        failures = enforce_perfect_canonical_gate(
            aggregate,
            expected_full_match=args.expected_full_match,
        )
        if failures:
            print(
                json.dumps(
                    {
                        "perfect_canonical_gate": "failed",
                        "failures": failures,
                        "report": str(aggregate_path),
                    },
                    indent=2,
                    sort_keys=True,
                )
            )
            return 1
        print(
            json.dumps(
                {
                    "perfect_canonical_gate": "passed",
                    "report": str(aggregate_path),
                },
                indent=2,
                sort_keys=True,
            )
        )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--addr", type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/raw_p_code_benchmark/latest")
    parser.add_argument("--manifest", type=Path, help="Run every row from a raw p-code parity manifest")
    parser.add_argument("--row", help="Run one named manifest row")
    parser.add_argument("--binary-id", help="Run only rows belonging to one binaries[] manifest id")
    parser.add_argument("--language-filter", help="Run only rows matching one Ghidra language id")
    parser.add_argument(
        "--max-rows-per-binary",
        type=int,
        default=None,
        help="Limit manifest execution to N rows per binary after filtering.",
    )
    parser.add_argument("--feature", help="Run only rows matching one feature tag from the manifest")
    parser.add_argument("--group", help="Run only rows matching one feature_group tag from the manifest")
    parser.add_argument("--no-analyze", action="store_true")
    parser.add_argument("--disassemble-missing", action="store_true")
    parser.add_argument("--fission-release", action="store_true")
    parser.add_argument(
        "--require-perfect-canonical",
        action="store_true",
        help=(
            "Fail if the aggregate canonical raw p-code gate is not exact: "
            "similarity/parity 1.0, invariant counts 0, no semantic mismatch "
            "buckets, and only sla_construct_tpl success sources."
        ),
    )
    parser.add_argument(
        "--expected-full-match",
        type=int,
        default=None,
        help="Optional exact full_match count required by --require-perfect-canonical.",
    )
    args = parser.parse_args()
    if args.max_rows_per_binary is not None and args.max_rows_per_binary < 0:
        parser.error("--max-rows-per-binary must be >= 0")

    if args.manifest:
        return run_manifest(args)
    if args.binary is None or args.addr is None:
        parser.error("--binary and --addr are required unless --manifest is used")

    binary = args.binary if args.binary.is_absolute() else ROOT / args.binary
    report = run_one(
        binary=binary,
        addr=args.addr,
        count=args.count,
        language=args.language,
        compiler=args.compiler,
        ghidra_dir=args.ghidra_dir,
        output_dir=args.output_dir,
        no_analyze=args.no_analyze,
        disassemble_missing=args.disassemble_missing,
        fission_release=args.fission_release,
    )
    print(json.dumps({
        "report": report["report"],
        "total_instructions": report["total_instructions"],
        "bucket_totals": report["bucket_totals"],
        "performance": report["performance"],
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
