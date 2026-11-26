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
freshbench_corpus.py. The base side can come from main's output ledger
(output_ledger.py), which runs the same sweep on main. With
`--fetch-base-run`, the ledger run is fetched for the newest first-parent
commit at or before the base that changed a decompiler input. The base
decompiles exactly as that commit does. The run is used only if it is clean
and has the head run's scorer, corpus and function count; otherwise the base
is decompiled here.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import freshbench_corpus as corpus

BUDGET = 3000

# What decides a commit's decompiler output on the sample. output-ledger.yml
# records main only when one of these changed (its `paths` filter lists the
# same).
DECOMPILER_INPUTS = ("crates", "utils", "Cargo.toml", "Cargo.lock", ".cargo", "fission.toml")


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


def ledger_commit(repo: Path) -> str:
    """The newest commit at or before `repo`'s HEAD, along first parents,
    that changed a decompiler input. HEAD decompiles exactly as it does, and
    on main it is the push that recorded a ledger entry."""
    return subprocess.run(["git", "-C", str(repo), "log", "-1", "--first-parent", "--format=%H", "HEAD", "--",
                           *DECOMPILER_INPUTS], check=True, capture_output=True, text=True).stdout.strip()


def fetch_ledger_run(commit: str, target: Path) -> Path | None:
    """Download main's `output-ledger-run-<commit>` artifact. None when there
    is none (the entry was never recorded, or expired)."""
    name = f"output-ledger-run-{commit}"
    try:
        listed = json.loads(subprocess.run(
            ["gh", "api", f"repos/{{owner}}/{{repo}}/actions/artifacts?name={name}&per_page=1"],
            check=True, capture_output=True, text=True).stdout)
        artifacts = [a for a in listed.get("artifacts", []) if not a.get("expired")]
        if not artifacts:
            print(f"base: no output ledger run for {commit[:12]}", flush=True)
            return None
        subprocess.run(["gh", "run", "download", str(artifacts[0]["workflow_run"]["id"]), "-n", name,
                        "-D", str(target)], check=True, capture_output=True, text=True)
    except (subprocess.CalledProcessError, OSError, json.JSONDecodeError) as err:
        print(f"base: could not fetch the output ledger run ({err})", flush=True)
        return None
    return target


def reusable(base_run: Path | None, head_run: Path, base_commit: str) -> bool:
    """Whether a ledger sweep can stand in for the base side: it measured
    `base_commit`, clean, with the head run's scorer, corpus and functions."""
    if base_run is None or not (base_run / "meta.json").exists():
        return False
    base, head = (json.loads((r / "meta.json").read_text()) for r in (base_run, head_run))
    return (base["git"]["commit"] == base_commit and not base["git"].get("dirty")
            and all(base[k] == head[k] for k in ("scorer", "corpus_id", "functions")))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--freshbench", type=Path, required=True, help="freshbench built at FRESHBENCH_COMMIT")
    parser.add_argument("--base-cli", type=Path, required=True)
    parser.add_argument("--head-cli", type=Path, required=True)
    parser.add_argument("--base-repo", type=Path, required=True, help="the base checkout (commit, utils/)")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--budget", type=int, default=BUDGET)
    parser.add_argument("--fetch-base-run", action="store_true",
                        help="use main's output ledger for the base side when it has the run (needs gh)")
    args = parser.parse_args()

    entries = corpus.manifest(args.data)
    chosen = units(entries, args.data, args.budget)
    corpus.fetch_all(corpus.scoring_files(entries, chosen), args.data)
    print(f"{len(chosen)} units: {', '.join(f'{b}/{p}' for b, p in sorted(chosen))}", flush=True)

    runs = Path(tempfile.mkdtemp(prefix="output-diff-runs-"))
    # Separate SLEIGH frontend caches, so head never decodes with frontends
    # the base CLI compiled (the cache is keyed by spec, not by code version).
    caches = Path(tempfile.mkdtemp(prefix="output-diff-sleigh-cache-"))
    for label, cli, repo in (("head", args.head_cli, corpus.REPO), ("base", args.base_cli, args.base_repo)):
        if label == "base" and args.fetch_base_run:
            commit = ledger_commit(repo)
            ledger = fetch_ledger_run(commit, Path(tempfile.mkdtemp(prefix="output-diff-ledger-")))
            if reusable(ledger, runs / "head", commit):
                shutil.copytree(ledger, runs / "base")
                print(f"base: reusing the output ledger's sweep of {commit[:12]}", flush=True)
                continue
            if ledger is not None:
                print("base: the output ledger run does not match this sample; decompiling the base", flush=True)
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
