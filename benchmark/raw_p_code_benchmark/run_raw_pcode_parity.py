#!/usr/bin/env python3
"""Run Ghidra-vs-Fission raw instruction p-code parity for one window."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def parse_int(value: str) -> int:
    return int(value, 0)


def run(cmd: list[str]) -> None:
    subprocess.run(cmd, cwd=ROOT, check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", required=True, type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/raw_p_code_benchmark/latest")
    parser.add_argument("--no-analyze", action="store_true")
    parser.add_argument("--fission-release", action="store_true")
    args = parser.parse_args()

    out_dir = args.output_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    ghidra_json = out_dir / "ghidra_raw_pcode.json"
    fission_json = out_dir / "fission_raw_pcode.json"
    report_json = out_dir / "raw_pcode_parity_report.json"

    binary = args.binary if args.binary.is_absolute() else ROOT / args.binary
    run(
        [
            "python3",
            str(THIS_DIR / "ghidra_raw_pcode.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(args.addr),
            "--count",
            str(args.count),
            "--language",
            args.language,
            "--compiler",
            args.compiler,
            "--output",
            str(ghidra_json),
            *(["--no-analyze"] if args.no_analyze else []),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "fission_raw_pcode.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(args.addr),
            "--count",
            str(args.count),
            "--output",
            str(fission_json),
            *(["--release"] if args.fission_release else []),
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
    print(json.dumps({
        "report": str(report_json),
        "total_instructions": report["total_instructions"],
        "bucket_totals": report["bucket_totals"],
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
