#!/usr/bin/env python3
"""Dump Fission raw instruction p-code through fission-sleigh."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


def parse_int(value: str) -> int:
    return int(value, 0)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", required=True, type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--window-bytes", type=int, default=32)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--release", action="store_true", help="Run the Rust probe in release mode")
    args = parser.parse_args()

    cmd = ["cargo", "run", "-p", "fission-sleigh"]
    if args.release:
        cmd.append("--release")
    cmd += [
        "--example",
        "raw_pcode_probe",
        "--",
        "--binary",
        str(args.binary),
        "--addr",
        hex(args.addr),
        "--count",
        str(args.count),
        "--window-bytes",
        str(args.window_bytes),
    ]
    proc = subprocess.run(cmd, text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise SystemExit(
            f"fission raw p-code probe failed with exit={proc.returncode}\n"
            f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )

    # Cargo warnings go to stderr. stdout should be the JSON report.
    report = json.loads(proc.stdout)
    report["tool"] = "fission"
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
