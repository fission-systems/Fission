#!/usr/bin/env python3
"""Parallel bisection: measure every main commit between a good and a bad one
at once, then name the first commit that regressed.

A sequential bisect needs log2(N) rounds of builds. With one job per commit,
up to MAX_COMMITS at once, the answer comes out of one round. A longer range
is sampled evenly, and the report narrows it for a second run.

    parallel_bisect.py plan --good G --bad B [--max N]     JSON list of commits, good first
    parallel_bisect.py probe output --data D --freshbench F --cli C --build B [--only S] --out FILE
    parallel_bisect.py probe speed --data D --cli C --out FILE
    parallel_bisect.py verdict --probe output|speed --input DIR --out DIR

Probes are those of the nightly checks this follows up:
- `output`: a FreshBench sweep of one build or slice. A commit is bad when
  fewer rows have output than on the good commit, or more tool calls fail.
- `speed`: the cachegrind sample of speed_compare.py. Each probe also
  measures the good commit's CLI on its own runner, because instruction
  counts differ a little between CPUs. A commit is bad at +THRESHOLD over
  that.

Corpus binaries are only decompiled; nothing is executed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import freshbench_corpus as corpus
import freshbench_sweep
import speed_compare

MAX_COMMITS = 48


def plan(commits: list[str], limit: int = MAX_COMMITS) -> list[str]:
    """`commits` oldest first, the good one first and the bad one last. At
    most `limit`, keeping both ends and spacing the rest evenly."""
    if len(commits) <= limit:
        return commits
    step = (len(commits) - 1) / (limit - 1)
    picked = sorted({round(i * step) for i in range(limit)})
    return [commits[i] for i in picked]


def first_bad(results: list[dict], probe: str, threshold: float = speed_compare.THRESHOLD) -> dict | None:
    """The first result, oldest first, that regressed against the first (good)
    one. None when nothing did."""
    good = results[0]
    for result in results[1:]:
        if probe == "output":
            if (result["rows_with_output"] < good["rows_with_output"]
                    or result["failed_calls"] > good["failed_calls"]):
                return result
        elif result["instructions"] >= result.get("good_instructions", good["instructions"]) * (1 + threshold):
            return result
    return None


def probe_output(args) -> dict:
    entries = corpus.manifest(args.data)
    corpus.fetch_all(corpus.scoring_files(entries, freshbench_sweep.units_for(entries, args.build, args.only)),
                     args.data)
    runs = Path(tempfile.mkdtemp(prefix="bisect-runs-"))
    only = ["--only", args.only] if args.only else []
    subprocess.run([str(args.freshbench), "sweep", "run", str(args.data / "m1"), "--cli", str(args.cli),
                    "--repo", str(args.repo), "--resource-root", str(args.repo / "utils"),
                    "--label", "probe", "--builds", args.build, *only,
                    "--runs", str(runs)], check=True, env=corpus.telemetry_free_env())
    report = freshbench_sweep.run_json(args.freshbench, "sweep", "report", str(runs / "probe"))
    rows = [json.loads(line) for line in (runs / "probe" / "rows.jsonl").read_text().splitlines() if line.strip()]
    hashes = hashlib.sha256("".join(sorted(f"{r.get('key')}{r.get('code_sha256')}\n" for r in rows)).encode())
    no_output = sum(report["failures"].values())
    return {
        "rows": report["overall"]["rows"],
        "rows_with_output": report["overall"]["rows"] - no_output,
        "failed_calls": report["timing"]["failed_calls"],
        "perfect": report["overall"]["perfect"],
        "distance": report["overall"]["distance"],
        "outputs_sha256": hashes.hexdigest()[:16],
    }


def probe_speed(args) -> dict:
    entries = sorted(corpus.stripped(corpus.manifest(args.data)), key=lambda e: e["sha256"])
    caches = Path(tempfile.mkdtemp(prefix="bisect-sleigh-cache-"))
    total, good = 0, 0
    for entry in entries[: speed_compare.BINARIES]:
        binary = corpus.fetch(entry, args.data)
        total += speed_compare.cost(args.cli, binary, args.repo / "utils", cache=caches / "measured")
        if args.good_cli:
            good += speed_compare.cost(args.good_cli, binary, args.good_repo / "utils", cache=caches / "good")
    return {"instructions": total, **({"good_instructions": good} if args.good_cli else {})}


def verdict(args) -> int:
    results = [json.loads(p.read_text()) for p in sorted(args.input.glob("*/probe.json"))]
    results.sort(key=lambda r: r["index"])
    if not results or results[0]["index"] != 0:
        raise SystemExit("the good commit's probe is missing")
    bad = first_bad(results, args.probe)
    lines = [f"### Bisect ({args.probe}): {len(results)} commits measured", ""]
    if args.probe == "output":
        lines += ["| # | commit | rows with output | failed calls | perfect | distance | outputs |",
                  "|---:|---|---:|---:|---:|---:|---|"]
        for r in results:
            lines.append(f"| {r['index']} | `{r['commit'][:12]}` | {r['rows_with_output']} | {r['failed_calls']} | "
                         f"{r['perfect']} | {r['distance']:g} | `{r['outputs_sha256']}` |")
    else:
        lines += ["| # | commit | instructions | good commit, same runner | ratio |", "|---:|---|---:|---:|---:|"]
        for r in results:
            g = r.get("good_instructions", results[0]["instructions"])
            lines.append(f"| {r['index']} | `{r['commit'][:12]}` | {r['instructions']:,} | {g:,} | "
                         f"{r['instructions'] / g:.4f} |")
    lines.append("")
    if bad is None:
        lines.append("No commit in the range regressed against the first one.")
    else:
        previous = next(r for r in reversed(results) if r["index"] < bad["index"])
        sampled = bad["index"] - previous["index"] > 1
        lines.append(f"**First regressing commit:** `{bad['commit']}`"
                     + (f" (measured commits only: the range `{previous['commit'][:12]}..{bad['commit'][:12]}` "
                        "was sampled, run the bisect again on it)" if sampled else ""))
    text = "\n".join(lines) + "\n"
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "bisect.md").write_text(text)
    (args.out / "bisect.json").write_text(json.dumps({"results": results, "first_bad": bad}, indent=2) + "\n")
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--good", required=True)
    p.add_argument("--bad", required=True)
    p.add_argument("--max", type=int, default=MAX_COMMITS)
    pr = sub.add_parser("probe")
    pr.add_argument("kind", choices=("output", "speed"))
    pr.add_argument("--data", type=Path, required=True)
    pr.add_argument("--cli", type=Path, required=True)
    pr.add_argument("--repo", type=Path, default=corpus.REPO,
                    help="the checkout the CLI was built from (its commit and utils/)")
    pr.add_argument("--freshbench", type=Path)
    pr.add_argument("--good-cli", type=Path, help="with speed: the good commit's CLI, measured on the same runner")
    pr.add_argument("--good-repo", type=Path, help="with --good-cli: its checkout (utils/)")
    pr.add_argument("--build", default="gcc-O2")
    pr.add_argument("--only")
    pr.add_argument("--commit", required=True)
    pr.add_argument("--index", type=int, required=True)
    pr.add_argument("--out", type=Path, required=True)
    v = sub.add_parser("verdict")
    v.add_argument("--probe", choices=("output", "speed"), required=True)
    v.add_argument("--input", type=Path, required=True)
    v.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "plan":
        listed = subprocess.run(["git", "rev-list", "--first-parent", "--reverse", f"{args.good}..{args.bad}"],
                                check=True, capture_output=True, text=True).stdout.split()
        good = subprocess.run(["git", "rev-parse", args.good], check=True, capture_output=True,
                              text=True).stdout.strip()
        commits = plan([good, *listed], args.max)
        print(json.dumps([{"index": i, "commit": c} for i, c in enumerate(commits)]))
        return 0
    if args.command == "probe":
        if args.kind == "output" and not args.freshbench:
            raise SystemExit("probe output needs --freshbench")
        result = probe_output(args) if args.kind == "output" else probe_speed(args)
        result.update(commit=args.commit, index=args.index)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result) + "\n")
        print(json.dumps(result))
        return 0
    return verdict(args)


if __name__ == "__main__":
    sys.exit(main())
