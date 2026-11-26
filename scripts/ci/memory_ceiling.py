#!/usr/bin/env python3
"""Decompile the largest FreshBench M1 binaries under a memory ceiling.

Memory blow-ups have reached main more than once (switch recovery re-pushing
the same node until OOM, a DAG printed as a tree to 61M statements, a machine
pushed to 123 GB) and no check notices them: a test timeout catches a hang,
not a process that grows. This decompiles the binaries most likely to grow,
watches the process tree's resident memory, kills it at the ceiling, and fails
if any binary reached it.

The sample is chosen by rule: per project, the largest stripped binary in the
corpus (by size, across every build); then the LARGEST projects by that size.
Binaries come from the pinned FreshBench revision, hash-checked
(freshbench_corpus.py), and are only decompiled (static).
"""

from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import freshbench_corpus as corpus

LARGEST = 8
POLL_SECONDS = 0.25


def sample(entries: list[dict], count: int = LARGEST) -> list[dict]:
    best: dict[str, dict] = {}
    for entry in corpus.stripped(entries):
        project = entry["project"]
        key = (entry["size"], entry["path"])
        if project not in best or key > (best[project]["size"], best[project]["path"]):
            best[project] = entry
    ranked = sorted(best.values(), key=lambda e: (-e["size"], e["path"]))
    return ranked[:count]


def tree_rss_kib(root: int) -> int:
    """Resident memory of `root` and its descendants, in KiB (ps works on
    Linux and macOS alike)."""
    listing = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,rss="], capture_output=True, text=True).stdout
    children: dict[int, list[int]] = {}
    rss: dict[int, int] = {}
    for line in listing.splitlines():
        fields = line.split()
        if len(fields) != 3:
            continue
        pid, ppid, kib = map(int, fields)
        children.setdefault(ppid, []).append(pid)
        rss[pid] = kib
    total, pending = 0, [root]
    while pending:
        pid = pending.pop()
        total += rss.get(pid, 0)
        pending.extend(children.get(pid, []))
    return total


def run_under_ceiling(command: list[str], ceiling_kib: int, timeout: float) -> tuple[str, int, float]:
    """Returns (outcome, peak KiB, seconds); outcome is ok, error, ceiling or timeout."""
    start = time.monotonic()
    with subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                          start_new_session=True) as proc:
        peak = 0
        outcome = None
        while proc.poll() is None:
            peak = max(peak, tree_rss_kib(proc.pid))
            elapsed = time.monotonic() - start
            if peak >= ceiling_kib:
                outcome = "ceiling"
            elif elapsed >= timeout:
                outcome = "timeout"
            if outcome:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
                break
            time.sleep(POLL_SECONDS)
        if outcome is None:
            outcome = "ok" if proc.returncode == 0 else "error"
    return outcome, peak, time.monotonic() - start


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--resources", type=Path, default=corpus.REPO / "utils")
    parser.add_argument("--ceiling-gib", type=float, default=8.0)
    parser.add_argument("--timeout", type=float, default=1200.0, help="seconds per binary")
    parser.add_argument("--count", type=int, default=LARGEST)
    args = parser.parse_args()

    ceiling = int(args.ceiling_gib * 1024 * 1024)
    rows = []
    for entry in sample(corpus.manifest(args.data), args.count):
        binary = corpus.fetch(entry, args.data)
        command = [str(args.cli), "--resource-root", str(args.resources), "--no-db", "decomp",
                   str(binary), "--all", "--json", "--no-header", "--no-warnings"]
        outcome, peak, seconds = run_under_ceiling(command, ceiling, args.timeout)
        rows.append((entry["path"], entry["size"], outcome, peak, seconds))
        print(f"{entry['path']}: {outcome}, peak {peak / 1048576:.2f} GiB, {seconds:.0f} s", flush=True)

    over = [row for row in rows if row[2] == "ceiling"]
    # A CLI that errors or hangs on a binary never reached its peak, so that
    # binary says nothing about memory: it fails the check too.
    unmeasured = [row for row in rows if row[2] in ("error", "timeout")]
    lines = [f"| binary | bytes | outcome | peak RSS (GiB) | seconds |", "|---|---|---|---|---|"]
    lines += [f"| `{b}` | {n} | {o} | {p / 1048576:.2f} | {s:.0f} |" for b, n, o, p, s in rows]
    summary = (f"Memory ceiling {args.ceiling_gib:g} GiB: {len(over)} of {len(rows)} binaries reached it; "
               f"{len(unmeasured)} did not finish (error or timeout).")
    print(summary)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(summary + "\n\n" + "\n".join(lines) + "\n")
    return 1 if over or unmeasured else 0


if __name__ == "__main__":
    sys.exit(main())
