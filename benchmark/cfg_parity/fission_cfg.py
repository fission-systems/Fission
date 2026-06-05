#!/usr/bin/env python3
"""Dump Fission address-keyed CFG snapshots through fission-sleigh."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def parse_int(value: str) -> int:
    return int(value, 0)


def run_cfg_probe(
    *,
    binary: Path,
    addr: int | None,
    model: str,
    release: bool,
    all_functions: bool,
) -> dict:
    if all_functions:
        cmd = ["cargo", "run", "-p", "fission-sleigh"]
        if release:
            cmd.append("--release")
        cmd += [
            "--example",
            "cfg_probe_sweep",
            "--",
            "--binary",
            str(binary),
            "--model",
            model,
        ]
    else:
        cmd = ["cargo", "run", "-p", "fission-sleigh"]
        if release:
            cmd.append("--release")
        cmd += [
            "--example",
            "cfg_probe",
            "--",
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--model",
            model,
        ]

    proc = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)
    print(proc.stderr, file=sys.stderr)
    if proc.returncode != 0:
        raise SystemExit(
            f"fission cfg probe failed with exit={proc.returncode}\n"
            f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )
    return json.loads(proc.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", type=parse_int, default=None)
    parser.add_argument(
        "--all-functions",
        action="store_true",
        help="Dump instruction CFG snapshots for every loader-known function.",
    )
    parser.add_argument(
        "--model",
        choices=("pcode_cfg_builder", "pcode_structuring", "pcode_instruction_cfg"),
        default="pcode_instruction_cfg",
    )
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    if not args.all_functions and args.addr is None:
        parser.error("one of --addr or --all-functions is required")
    if args.all_functions and args.model != "pcode_instruction_cfg":
        parser.error("--all-functions currently requires --model pcode_instruction_cfg")

    started_at = time.perf_counter()
    report = run_cfg_probe(
        binary=args.binary,
        addr=args.addr,
        model=args.model,
        release=args.release,
        all_functions=args.all_functions,
    )
    elapsed_sec = time.perf_counter() - started_at
    if args.all_functions:
        report["timing"] = {
            "wall_clock_sec": elapsed_sec,
            "function_count": report.get("function_count"),
        }
    else:
        probe_timing = report.get("timing") if isinstance(report.get("timing"), dict) else {}
        rust_probe_sec = probe_timing.get("rust_probe_sec")
        process_startup_sec = None
        if isinstance(rust_probe_sec, (int, float)):
            process_startup_sec = max(0.0, elapsed_sec - float(rust_probe_sec))
        report["timing"] = {
            "wall_clock_sec": elapsed_sec,
            "process_startup_sec": process_startup_sec,
            "frontend_load_sec": probe_timing.get("frontend_load_sec"),
            "decode_lift_sec": probe_timing.get("decode_lift_sec"),
            "binary_load_sec": probe_timing.get("binary_load_sec"),
            "rust_probe_sec": rust_probe_sec,
            "block_count": report.get("block_count"),
            "edge_count": report.get("edge_count"),
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
