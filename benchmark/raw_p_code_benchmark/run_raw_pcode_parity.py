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


def run_one(
    *,
    binary: Path,
    addr: int,
    count: int,
    language: str,
    compiler: str,
    output_dir: Path,
    no_analyze: bool,
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
            "--output",
            str(ghidra_json),
            *(["--no-analyze"] if no_analyze else []),
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
    rows = manifest.get("rows", [])
    if args.row:
        rows = [row for row in rows if row.get("name") == args.row]
        if not rows:
            raise SystemExit(f"no row named {args.row!r} in {manifest_path}")

    aggregate_totals: dict[str, int] = {}
    row_reports = []
    for row in rows:
        binary = Path(row.get("binary", args.binary or ""))
        if not binary:
            raise SystemExit(f"row {row.get('name', '<unnamed>')} is missing binary")
        binary = binary if binary.is_absolute() else ROOT / binary
        row_name = row.get("name", f"0x{parse_int(str(row['addr'])):x}")
        output_dir = args.output_dir / row_name
        report = run_one(
            binary=binary,
            addr=parse_int(str(row["addr"])),
            count=int(row.get("count", args.count)),
            language=row.get("language", args.language),
            compiler=row.get("compiler", args.compiler),
            output_dir=output_dir,
            no_analyze=bool(row.get("no_analyze", args.no_analyze)),
            fission_release=args.fission_release,
        )
        for bucket, count in report["bucket_totals"].items():
            aggregate_totals[bucket] = aggregate_totals.get(bucket, 0) + int(count)
        first_mismatch = next(
            (entry for entry in report["rows"] if entry.get("buckets") != ["full_match"]),
            None,
        )
        row_reports.append(
            {
                "name": row_name,
                "binary": str(binary),
                "addr": row["addr"],
                "report": report["report"],
                "total_instructions": report["total_instructions"],
                "bucket_totals": report["bucket_totals"],
                "first_mismatch": first_mismatch,
            }
        )

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(row_reports),
        "bucket_totals": dict(sorted(aggregate_totals.items())),
        "rows": row_reports,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    aggregate_path = args.output_dir / "aggregate_raw_pcode_parity_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "report": str(aggregate_path),
        "row_count": aggregate["row_count"],
        "bucket_totals": aggregate["bucket_totals"],
    }, indent=2, sort_keys=True))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--addr", type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/raw_p_code_benchmark/latest")
    parser.add_argument("--manifest", type=Path, help="Run every row from a raw p-code parity manifest")
    parser.add_argument("--row", help="Run one named manifest row")
    parser.add_argument("--no-analyze", action="store_true")
    parser.add_argument("--fission-release", action="store_true")
    args = parser.parse_args()

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
        output_dir=args.output_dir,
        no_analyze=args.no_analyze,
        fission_release=args.fission_release,
    )
    print(json.dumps({
        "report": report["report"],
        "total_instructions": report["total_instructions"],
        "bucket_totals": report["bucket_totals"],
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
