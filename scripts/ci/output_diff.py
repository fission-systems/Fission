#!/usr/bin/env python3
"""Decompile a fixed FreshBench M1 sample with a PR's base and head CLIs and
report what changed: output text first, scores second.

A decompiler change is judged by what it prints. Scores alone have ranked a
regression as a win, and a flat total distance has hidden real changes. So the
report is `freshbench sweep diff`:
- how many functions' text changed;
- the score gained and lost on those functions only;
- the largest moves.

Report-only: it never fails on a score. It fails only if it cannot run.

The sample is chosen by rule: the corpus's (build, project) units in the
sha256 order of their targets file, until BUDGET target functions. Each side
is a `freshbench sweep run` over those units with the scorer pinned in
freshbench_corpus.py.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import freshbench_corpus as corpus

BUDGET = 3000


def units(entries: list[dict], root: Path, budget: int = BUDGET) -> set[tuple[str, str]]:
    """(build, project) units in the fixed order, until `budget` functions."""
    truths = sorted((e for e in entries if e["kind"] == "ground_truth"), key=lambda e: e["sha256"])
    chosen: set[tuple[str, str]] = set()
    total = 0
    for entry in truths:
        if total >= budget:
            break
        targets = json.loads(corpus.fetch(entry, root).read_text())
        total += sum(len(binary["functions"]) for binary in targets["binaries"].values())
        chosen.add((corpus.build_of(entry), entry["project"]))
    return chosen


def sweep(freshbench: Path, root: Path, cli: Path, repo: Path, label: str, runs: Path) -> None:
    subprocess.run(
        [str(freshbench), "sweep", "run", str(root / "m1"), "--cli", str(cli), "--repo", str(repo),
         "--resource-root", str(repo / "utils"), "--label", label, "--runs", str(runs)],
        check=True, env=corpus.telemetry_free_env(),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--freshbench", type=Path, required=True, help="freshbench built at FRESHBENCH_COMMIT")
    parser.add_argument("--base-cli", type=Path, required=True)
    parser.add_argument("--head-cli", type=Path, required=True)
    parser.add_argument("--base-repo", type=Path, required=True, help="the base checkout (commit, utils/)")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--budget", type=int, default=BUDGET)
    args = parser.parse_args()

    entries = corpus.manifest(args.data)
    chosen = units(entries, args.data, args.budget)
    corpus.fetch_all(corpus.scoring_files(entries, chosen), args.data)
    print(f"{len(chosen)} units: {', '.join(f'{b}/{p}' for b, p in sorted(chosen))}", flush=True)

    runs = Path(tempfile.mkdtemp(prefix="output-diff-runs-"))
    # Separate SLEIGH frontend caches, so head never decodes with frontends
    # the base CLI compiled (the cache is keyed by spec, not by code version).
    caches = Path(tempfile.mkdtemp(prefix="output-diff-sleigh-cache-"))
    for label, cli, repo in (("base", args.base_cli, args.base_repo), ("head", args.head_cli, corpus.REPO)):
        os.environ["FISSION_SLEIGH_CACHE_DIR"] = str(caches / label)
        sweep(args.freshbench, args.data, cli.resolve(), repo.resolve(), label, runs)

    diff = [str(args.freshbench), "sweep", "diff", str(runs / "base"), str(runs / "head")]
    text = subprocess.run(diff, check=True, capture_output=True, text=True).stdout
    data = subprocess.run([*diff, "--json"], check=True, capture_output=True, text=True).stdout
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "output_diff.json").write_text(data)
    header = (f"### Decompiler output diff (fixed FreshBench M1 sample, {len(chosen)} units)\n\n"
              "Report only: scores here never fail the check. Open the changed functions before trusting a score.\n\n")
    (args.out / "output_diff.md").write_text(header + text)
    print(header + text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(header + text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
