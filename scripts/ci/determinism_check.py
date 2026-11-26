#!/usr/bin/env python3
"""Decompile a fixed FreshBench M1 sample several times and require identical text.

AGENTS.md core rule 4: outputs feed snapshots, metrics and benchmark
comparisons, so the same input must decompile to the same text whatever the
run or thread count. A nondeterministic function makes every diff of it noise.

The sample is chosen by rule, not by hand: the corpus's stripped binaries in
sha256 order (an arbitrary but fixed shuffle of architectures, compilers,
optimisation levels and projects), decompiled until FUNCTION_BUDGET functions.
Binaries come from the pinned FreshBench revision, checked against its
manifest (freshbench_corpus.py), and are only decompiled (static).
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import freshbench_corpus as corpus
from decompiler_smoke import decompile_codes

FUNCTION_BUDGET = 5000
# Thread counts per run: one single-threaded run, the rest parallel.
THREADS = (1, 4, 4, 4)


def ordered(entries: list[dict]) -> list[dict]:
    """The stripped binaries in the fixed sample order."""
    return sorted(corpus.stripped(entries), key=lambda e: e["sha256"])


def differing(runs: list[dict[str, str]]) -> list[str]:
    addresses = set().union(*runs)
    return sorted(a for a in addresses if len({run.get(a) for run in runs}) != 1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--resources", type=Path, default=corpus.REPO / "utils")
    parser.add_argument("--budget", type=int, default=FUNCTION_BUDGET)
    parser.add_argument("--only", help="Check one manifest path (stripped/...) instead of the sample")
    args = parser.parse_args()

    entries = corpus.manifest(args.data)
    targets = [e for e in entries if e["path"] == args.only] if args.only else ordered(entries)
    if not targets:
        raise SystemExit("no binaries selected")
    failures = {}
    total, checked = 0, 0
    for entry in targets:
        if total >= args.budget:
            break
        binary = corpus.fetch(entry, args.data)
        runs = [decompile_codes(args.cli, binary, workers, resources=args.resources, timeout=1800)
                for workers in THREADS]
        total += len(runs[0])
        checked += 1
        bad = differing(runs)
        print(f"{entry['path']}: {len(runs[0])} functions, {len(bad)} differ", flush=True)
        if bad:
            failures[entry["path"]] = bad
    summary = f"{total} functions in {checked} binaries over {len(THREADS)} runs; {sum(map(len, failures.values()))} differ"
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
