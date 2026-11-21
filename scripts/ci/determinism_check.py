#!/usr/bin/env python3
"""Decompile a fixed DecBench sample several times and require identical text.

AGENTS.md core rule 4: outputs feed snapshots, metrics and benchmark
comparisons, so the same input must decompile to the same text whatever the
run or thread count. A nondeterministic function makes every diff of it noise.

The sample is chosen by rule, not by hand: manifest binaries in sha256
order (an arbitrary but fixed shuffle of sizes, optimisation levels and kinds)
until FUNCTION_BUDGET functions. Binaries are downloaded from the pinned
dataset revision, checked against the manifest's hash, and only decompiled
(static).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "quality"))
import decbench_full_sweep as sweep  # noqa: E402

FUNCTION_BUDGET = 5000
# Thread counts per run: one single-threaded run, the rest parallel.
THREADS = (1, 4, 4, 4)


def sample(entries: list[dict]) -> list[dict]:
    chosen, total = [], 0
    for entry in sorted(entries, key=lambda e: e["sha256"]):
        if total >= FUNCTION_BUDGET:
            break
        chosen.append(entry)
        total += len(entry["functions"])
    return chosen


def decompile(cli: Path, binary: Path, resources: Path, threads: int) -> dict[str, str]:
    env = dict(os.environ, RAYON_NUM_THREADS=str(threads), FISSION_BENCHMARK_NO_CACHE="1")
    result = subprocess.run(
        [str(cli), "--resource-root", str(resources), "--no-db", "decomp", str(binary),
         "--all", "--json", "--no-header", "--no-warnings"],
        capture_output=True, text=True, env=env, timeout=1800, check=True,
    )
    return {
        str(entry.get("address")): hashlib.sha256(str(entry.get("code")).encode()).hexdigest()
        for entry in json.loads(result.stdout)
    }


def differing(runs: list[dict[str, str]]) -> list[str]:
    addresses = set().union(*runs)
    return sorted(a for a in addresses if len({run.get(a) for run in runs}) != 1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--resources", type=Path, default=sweep.REPO / "utils")
    parser.add_argument("--only", help="Check one manifest binary path instead of the sample")
    args = parser.parse_args()

    entries = sweep.manifest(args.data)
    targets = [e for e in entries if e["binary_path"] == args.only] if args.only else sample(entries)
    if not targets:
        raise SystemExit("no binaries selected")
    failures = {}
    total = 0
    for entry in targets:
        binary = sweep.fetch(entry["binary_path"], args.data)
        if binary.stat().st_size != entry["size"] or sweep.digest(binary) != entry["sha256"]:
            raise SystemExit(f"dataset binary hash or size differs for {entry['binary_path']}")
        runs = [decompile(args.cli, binary, args.resources, threads) for threads in THREADS]
        total += len(runs[0])
        bad = differing(runs)
        print(f"{entry['binary_path']}: {len(runs[0])} functions, {len(bad)} differ", flush=True)
        if bad:
            failures[entry["binary_path"]] = bad
    summary = f"{total} functions in {len(targets)} binaries over {len(THREADS)} runs; {sum(map(len, failures.values()))} differ"
    print(summary)
    for path, addresses in failures.items():
        print(f"  {path}: {', '.join(addresses[:20])}")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(summary + "\n")
            for path, addresses in failures.items():
                out.write(f"- `{path}`: {', '.join(addresses[:20])}\n")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
