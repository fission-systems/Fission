#!/usr/bin/env python3
"""Run Ghidra-vs-Fission CFG parity for one function or a manifest."""

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


def run(cmd: list[str], *, check: bool = True) -> None:
    subprocess.run(cmd, cwd=ROOT, check=check)


def run_one(
    *,
    binary: Path,
    addr: int,
    ghidra_model: str,
    fission_model: str,
    language: str,
    compiler: str,
    ghidra_dir: Path | None,
    output_dir: Path,
    fission_release: bool,
    decompile_timeout_sec: int,
) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    ghidra_json = output_dir / "ghidra_cfg.json"
    fission_json = output_dir / "fission_cfg.json"
    report_json = output_dir / "cfg_parity_report.json"

    run(
        [
            "python3",
            str(THIS_DIR / "ghidra_cfg.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--model",
            ghidra_model,
            "--language",
            language,
            "--compiler",
            compiler,
            "--decompile-timeout-sec",
            str(decompile_timeout_sec),
            *(["--ghidra-dir", str(ghidra_dir)] if ghidra_dir is not None else []),
            "--output",
            str(ghidra_json),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "fission_cfg.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--model",
            fission_model,
            *(["--release"] if fission_release else []),
            "--output",
            str(fission_json),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "compare_cfg.py"),
            "--ghidra",
            str(ghidra_json),
            "--fission",
            str(fission_json),
            "--output",
            str(report_json),
        ],
        check=False,
    )
    report = json.loads(report_json.read_text())
    report["report"] = str(report_json)
    report["ghidra_json"] = str(ghidra_json)
    report["fission_json"] = str(fission_json)
    return report


def run_manifest(args: argparse.Namespace) -> int:
    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    manifest = json.loads(manifest_path.read_text())
    defaults = manifest.get("defaults", {}) or {}
    rows = manifest.get("rows", [])
    if args.row:
        rows = [row for row in rows if row.get("name") == args.row]
        if not rows:
            raise SystemExit(f"no row named {args.row!r} in {manifest_path}")

    row_reports = []
    bucket_totals: dict[str, int] = {}
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
            ghidra_model=row.get("ghidra_model", defaults.get("ghidra_model", args.ghidra_model)),
            fission_model=row.get("fission_model", defaults.get("fission_model", args.fission_model)),
            language=row.get("language", defaults.get("language", args.language)),
            compiler=row.get("compiler", defaults.get("compiler", args.compiler)),
            ghidra_dir=args.ghidra_dir,
            output_dir=output_dir,
            fission_release=args.fission_release,
            decompile_timeout_sec=args.decompile_timeout_sec,
        )
        for bucket in report.get("buckets", []):
            bucket_totals[bucket] = bucket_totals.get(bucket, 0) + 1
        row_reports.append(
            {
                "name": row_name,
                "binary": str(binary),
                "addr": row["addr"],
                "notes": row.get("notes"),
                "report": report["report"],
                "buckets": report.get("buckets", []),
                "ghidra_block_count": report.get("ghidra_block_count"),
                "fission_block_count": report.get("fission_block_count"),
                "ghidra_edge_count": report.get("ghidra_edge_count"),
                "fission_edge_count": report.get("fission_edge_count"),
                "missing_edges": report.get("missing_edges", [])[:5],
                "extra_edges": report.get("extra_edges", [])[:5],
            }
        )

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(row_reports),
        "bucket_totals": dict(sorted(bucket_totals.items())),
        "rows": row_reports,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    aggregate_path = args.output_dir / "aggregate_cfg_parity_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            {
                "report": str(aggregate_path),
                "row_count": aggregate["row_count"],
                "bucket_totals": aggregate["bucket_totals"],
            },
            indent=2,
            sort_keys=True,
        )
    )

    if args.require_full_match:
        full_match = bucket_totals.get("full_match", 0)
        if full_match != len(row_reports):
            print(
                json.dumps(
                    {
                        "full_match_gate": "failed",
                        "expected_full_match": len(row_reports),
                        "actual_full_match": full_match,
                        "report": str(aggregate_path),
                    },
                    indent=2,
                    sort_keys=True,
                )
            )
            return 1
        print(json.dumps({"full_match_gate": "passed", "report": str(aggregate_path)}, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--addr", type=parse_int)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--ghidra-model", default="ghidra_basic_block_model")
    parser.add_argument("--fission-model", default="pcode_instruction_cfg")
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/cfg_parity/latest")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--row")
    parser.add_argument("--fission-release", action="store_true")
    parser.add_argument("--decompile-timeout-sec", type=int, default=120)
    parser.add_argument("--require-full-match", action="store_true")
    args = parser.parse_args()

    if args.manifest:
        return run_manifest(args)
    if args.binary is None or args.addr is None:
        parser.error("--binary and --addr are required unless --manifest is used")

    binary = args.binary if args.binary.is_absolute() else ROOT / args.binary
    report = run_one(
        binary=binary,
        addr=args.addr,
        ghidra_model=args.ghidra_model,
        fission_model=args.fission_model,
        language=args.language,
        compiler=args.compiler,
        ghidra_dir=args.ghidra_dir,
        output_dir=args.output_dir,
        fission_release=args.fission_release,
        decompile_timeout_sec=args.decompile_timeout_sec,
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report.get("buckets") == ["full_match"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
