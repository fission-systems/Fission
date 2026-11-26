#!/usr/bin/env python3
"""CI side of the FreshBench sweep (docs/BENCHMARK_FRESHBENCH.md).

The nightly sweep runs one shard per M1 build. Fast Gate's smoke runs one
binary through the same jobs when the sweep's definition changes. The
decompiling and scoring are `freshbench sweep run`, built at the commit
pinned in freshbench_corpus.py; this script fetches what a shard needs and
reads the results.

    freshbench_sweep.py builds                  JSON list of every M1 build
    freshbench_sweep.py fetch --data D --build B [--only S]
    freshbench_sweep.py smoke RUN               require every stage to have produced data
    freshbench_sweep.py report --freshbench F --input DIR [--baseline DIR] --out DIR

`report` fails on a missing shard or on a row that had output on the last
passing main sweep and has none now; score deltas never fail it.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

import freshbench_corpus as corpus


def builds(entries: list[dict]) -> list[str]:
    return sorted({corpus.build_of(e) for e in entries if e["kind"] == "ground_truth"})


def units_for(entries: list[dict], build: str, only: str | None) -> set[tuple[str, str]]:
    """The build's (build, project) units that `sweep run --builds build --only
    only` can select: `only` is a substring of `<build>/<project>/<binary>`,
    so a unit is needed when the substring could fall inside it."""
    chosen = set()
    for entry in entries:
        if entry["kind"] != "ground_truth" or corpus.build_of(entry) != build:
            continue
        unit = f"{build}/{entry['project']}"
        if only is None or only in unit or only.startswith(unit + "/") or "/" not in only:
            chosen.add((build, entry["project"]))
    return chosen


def smoke(report: dict) -> list[str]:
    """Reasons a smoke run did not exercise every stage: decompiled functions,
    GED against a source CFG, a type score, and no failed tool call. A broken
    scorer shows up as rows with no scores, not as an error."""
    overall, timing = report["overall"], report["timing"]
    problems = []
    if not overall["rows"]:
        problems.append("no rows")
    if not overall["scored"]:
        problems.append("no row scored for GED")
    if not overall["types_perfect"]:
        problems.append("no row got a perfect type score")
    if timing["failed_calls"]:
        problems.append(f"{timing['failed_calls']} failed tool calls")
    return problems


def table(reports: dict[str, dict]) -> str:
    lines = ["| build | rows | perfect | distance | scored | types perfect | failed calls | CPU s |",
             "|---|---:|---:|---:|---:|---:|---:|---:|"]
    for build, r in sorted(reports.items()):
        o, t = r["overall"], r["timing"]
        lines.append(f"| {build} | {o['rows']} | {o['perfect']} | {o['distance']:g} | {o['scored']} | "
                     f"{o['types_perfect']} | {t['failed_calls']} | {t['decompile_cpu_s']:.0f} |")
    total = {k: sum(r["overall"][k] for r in reports.values()) for k in ("rows", "perfect", "scored", "types_perfect")}
    distance = sum(r["overall"]["distance"] for r in reports.values())
    failed = sum(r["timing"]["failed_calls"] for r in reports.values())
    cpu = sum(r["timing"]["decompile_cpu_s"] for r in reports.values())
    lines.append(f"| **all** | {total['rows']} | {total['perfect']} | {distance:g} | {total['scored']} | "
                 f"{total['types_perfect']} | {failed} | {cpu:.0f} |")
    return "\n".join(lines) + "\n"


def run_json(freshbench: Path, *args: str) -> dict:
    out = subprocess.run([str(freshbench), *args, "--json"], check=True, capture_output=True, text=True,
                         env=corpus.telemetry_free_env()).stdout
    return json.loads(out)


def report(args) -> int:
    expected = builds(corpus.manifest(args.data))
    runs = {p.name: p for p in sorted(args.input.iterdir()) if (p / "meta.json").exists()}
    reports = {name: run_json(args.freshbench, "sweep", "report", str(path)) for name, path in runs.items()}
    missing = [b for b in expected if b not in runs]
    args.out.mkdir(parents=True, exist_ok=True)
    text = "### FreshBench M1 sweep\n\n" + table(reports)
    if missing:
        text += f"\n**Missing shards:** {', '.join(missing)}\n"
    diffs = {}
    if args.baseline and args.baseline.is_dir():
        for name, path in runs.items():
            base = args.baseline / name
            if (base / "meta.json").exists():
                try:
                    diffs[name] = run_json(args.freshbench, "sweep", "diff", str(base), str(path))
                except subprocess.CalledProcessError as err:
                    # Different scorer: no comparison, said so instead.
                    diffs[name] = {"error": (err.stderr or "").strip()[:500]}
        changed = {n: d for n, d in diffs.items() if "error" not in d}
        if changed:
            text += ("\n### Against the last passing main sweep\n\n"
                     "| build | changed outputs | perfect gained | perfect lost | distance delta |\n"
                     "|---|---:|---:|---:|---:|\n")
            for name, d in sorted(changed.items()):
                text += (f"| {name} | {d['changed_outputs']} | {d['perfect_gained']} | {d['perfect_lost']} | "
                         f"{d['distance_delta']:+g} |\n")
        refused = sorted(n for n, d in diffs.items() if "error" in d)
        if refused:
            text += f"\nNot compared (different scorer or function set): {', '.join(refused)}\n"
        lost = {n: d["lost_output"] for n, d in changed.items() if d["lost_output"]}
        if lost:
            text += "\n**Rows that had output on the baseline and lost it:** " + ", ".join(
                f"{n} ({count})" for n, count in sorted(lost.items())) + "\n"
    else:
        text += "\nNo baseline sweep to compare with.\n"
    lost_total = sum(d.get("lost_output", 0) for d in diffs.values() if "error" not in d)
    lost_builds = sorted(n for n, d in diffs.items() if "error" not in d and d.get("lost_output"))
    baseline_commit = ""
    for name in lost_builds:
        meta = args.baseline / name / "meta.json"
        if meta.exists():
            baseline_commit = json.loads(meta.read_text()).get("git", {}).get("commit", "")
            break
    if os.environ.get("GITHUB_OUTPUT"):
        # For the nightly's bisect: which build lost rows, and since when.
        with open(os.environ["GITHUB_OUTPUT"], "a") as out:
            out.write(f"lost={lost_total}\nlost_build={lost_builds[0] if lost_builds else ''}\n"
                      f"baseline_commit={baseline_commit}\n")
    (args.out / "summary.json").write_text(json.dumps({"reports": reports, "diffs": diffs, "missing": missing,
                                                       "lost_output": lost_total}, indent=2) + "\n")
    (args.out / "summary.md").write_text(text)
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    # Blocking: a missing shard, or a row that had output on the last passing
    # main sweep and has none now. Score deltas are reported, never blocking.
    return 1 if missing or lost_total else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", type=Path, default=Path(os.environ.get("RUNNER_TEMP", ".")) / "freshbench")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("builds")
    fetch = sub.add_parser("fetch")
    fetch.add_argument("--build", required=True)
    fetch.add_argument("--only")
    check = sub.add_parser("smoke")
    check.add_argument("run", type=Path)
    check.add_argument("--freshbench", type=Path, required=True)
    rep = sub.add_parser("report")
    rep.add_argument("--freshbench", type=Path, required=True)
    rep.add_argument("--input", type=Path, required=True)
    rep.add_argument("--baseline", type=Path)
    rep.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "builds":
        print(json.dumps(builds(corpus.manifest(args.data))))
        return 0
    if args.command == "fetch":
        entries = corpus.manifest(args.data)
        units = units_for(entries, args.build, args.only)
        if not units:
            raise SystemExit(f"no units in build {args.build} for {args.only!r}")
        files = corpus.scoring_files(entries, units)
        corpus.fetch_all(files, args.data)
        print(f"{args.build}: {len(units)} units, {len(files)} files, "
              f"{sum(f['size'] for f in files) / 1e6:.0f} MB", flush=True)
        return 0
    if args.command == "smoke":
        problems = smoke(run_json(args.freshbench, "sweep", "report", str(args.run)))
        if problems:
            raise SystemExit("smoke run did not exercise every sweep stage: " + "; ".join(problems))
        print("smoke: every stage produced data")
        return 0
    return report(args)


if __name__ == "__main__":
    sys.exit(main())
