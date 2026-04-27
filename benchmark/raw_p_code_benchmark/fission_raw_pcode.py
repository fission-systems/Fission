#!/usr/bin/env python3
"""Dump Fission raw instruction p-code through fission-sleigh."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
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
    started_at = time.perf_counter()
    proc = subprocess.run(cmd, text=True, capture_output=True, check=False)
    elapsed_sec = time.perf_counter() - started_at
    print(proc.stderr, file=sys.stderr)
    if proc.returncode != 0:
        raise SystemExit(
            f"fission raw p-code probe failed with exit={proc.returncode}\n"
            f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )

    # Cargo warnings go to stderr. stdout should be the JSON report.
    report = json.loads(proc.stdout)
    instructions = report.get("instructions", [])
    instruction_count = sum(1 for instruction in instructions if instruction.get("status") == "ok")
    pcode_op_count = sum(len(instruction.get("pcode", [])) for instruction in instructions)
    report["timing"] = {
        "wall_clock_sec": elapsed_sec,
        "instruction_count": instruction_count,
        "pcode_op_count": pcode_op_count,
        "instructions_per_sec": instruction_count / elapsed_sec if elapsed_sec > 0 else None,
        "pcode_ops_per_sec": pcode_op_count / elapsed_sec if elapsed_sec > 0 else None,
    }
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
