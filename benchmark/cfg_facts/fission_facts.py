#!/usr/bin/env python3
"""Dump Fission ControlFlowFacts slices through fission-static facts_probe."""

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


def run_facts_probe(
    *,
    binary: Path,
    addr: int | None,
    release: bool,
    all_functions: bool,
) -> dict:
    cmd = ["cargo", "run", "-p", "fission-static"]
    if release:
        cmd.append("--release")
    cmd += ["--example", "facts_probe", "--", "--binary", str(binary)]
    if all_functions:
        cmd.append("--all-functions")
    else:
        cmd += ["--addr", hex(addr)]

    proc = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)
    print(proc.stderr, file=sys.stderr)
    if proc.returncode != 0:
        raise SystemExit(
            f"fission facts probe failed with exit={proc.returncode}\n"
            f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )
    return json.loads(proc.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", type=parse_int, default=None)
    parser.add_argument("--all-functions", action="store_true")
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    if not args.all_functions and args.addr is None:
        parser.error("one of --addr or --all-functions is required")

    started_at = time.perf_counter()
    report = run_facts_probe(
        binary=args.binary,
        addr=args.addr,
        release=args.release,
        all_functions=args.all_functions,
    )
    report["timing"] = {
        "wall_clock_sec": time.perf_counter() - started_at,
        **(report.get("timing") or {}),
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
