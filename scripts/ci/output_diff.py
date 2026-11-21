#!/usr/bin/env python3
"""Decompile a fixed DecBench sample with a PR's base and head CLIs and report
what changed: output text first, scores second.

A decompiler change is judged by what it prints. Scores alone have ranked a
regression as a win, and a flat total distance has hidden real changes, so the
report leads with how many functions' text changed and lists them, then gives
GED and type_match before and after on those functions and over the sample.

Report-only: it never fails on a score. It fails only if it cannot run.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "quality"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import decbench_full_sweep as sweep  # noqa: E402
import determinism_check  # noqa: E402

BUDGET = 3000
LISTED = 25


def key(row: dict) -> tuple[str, str]:
    return row["binary"], row["function"]


def totals(rows: list[dict]) -> dict:
    ged = [r["ged"] for r in rows if "ged" in r]
    types = [r["type_match"] for r in rows if "type_match" in r]
    return {
        "rows": len(rows),
        "ok": sum(r["status"] == "ok" for r in rows),
        "ged_perfect": ged.count(0),
        "ged_total": sum(ged),
        "type_perfect": types.count(1),
        "type_mean": round(sum(types) / len(types), 4) if types else None,
    }


def compare(base: list[dict], head: list[dict]) -> dict:
    before = {key(r): r for r in base}
    after = {key(r): r for r in head}
    if before.keys() != after.keys():
        raise SystemExit("base and head scored different rows")
    changed = sorted(
        k for k in before
        if before[k].get("code_sha256") != after[k].get("code_sha256")
        or before[k]["status"] != after[k]["status"]
    )
    status_flips = [k for k in changed if before[k]["status"] != after[k]["status"]]
    return {
        "sample": {"base": totals(base), "head": totals(head)},
        "changed": len(changed),
        "status_flips": len(status_flips),
        "changed_rows": {
            "base": totals([before[k] for k in changed]),
            "head": totals([after[k] for k in changed]),
        },
        "listed": [
            {"binary": b, "function": f,
             "status": [before[(b, f)]["status"], after[(b, f)]["status"]],
             "ged": [before[(b, f)].get("ged"), after[(b, f)].get("ged")],
             "type_match": [before[(b, f)].get("type_match"), after[(b, f)].get("type_match")]}
            for b, f in changed[:LISTED]
        ],
    }


def markdown(report: dict) -> str:
    s, c = report["sample"], report["changed_rows"]
    lines = [
        "### Decompiler output diff (fixed DecBench sample)",
        "",
        f"**{report['changed']}** of {s['base']['rows']} functions print differently "
        f"({report['status_flips']} changed status).",
        "",
        "| | GED perfect | GED total distance | type perfect | type mean | decompiled |",
        "|---|---|---|---|---|---|",
    ]
    for label, part in (("sample, base", s["base"]), ("sample, head", s["head"]),
                        ("changed, base", c["base"]), ("changed, head", c["head"])):
        lines.append(f"| {label} | {part['ged_perfect']} | {part['ged_total']:g} | "
                     f"{part['type_perfect']} | {part['type_mean']} | {part['ok']} |")
    if report["listed"]:
        lines += ["", f"First {len(report['listed'])} changed functions:", "",
                  "| binary | function | status | GED | type_match |", "|---|---|---|---|---|"]
        for row in report["listed"]:
            lines.append(f"| `{row['binary']}` | `{row['function']}` | {row['status'][0]} → {row['status'][1]} | "
                         f"{row['ged'][0]} → {row['ged'][1]} | {row['type_match'][0]} → {row['type_match'][1]} |")
    lines += ["", "Report only: scores here never fail the check. Open the changed functions before trusting a score."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--base-cli", type=Path, required=True)
    parser.add_argument("--head-cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--budget", type=int, default=BUDGET)
    parser.add_argument("--base-resources", type=Path, help="the base checkout's utils/ (default: this one's)")
    args = parser.parse_args()

    from decbench_scoring import load

    os.environ["FISSION_BENCHMARK_NO_CACHE"] = "1"
    os.environ["DECBENCH_NO_CACHE"] = "1"
    api = load(sweep.REPO)
    entries = determinism_check.sample(sweep.manifest(args.data), args.budget)
    head_resources = sweep.RESOURCES
    base_rows, head_rows = [], []
    for entry in entries:
        for cli, rows, resources in ((args.base_cli, base_rows, args.base_resources or head_resources),
                                     (args.head_cli, head_rows, head_resources)):
            sweep.RESOURCES = resources
            scored, _ = sweep.score(entry, args.data, cli.resolve(), api)
            rows.extend(scored)
        print(f"{entry['binary_path']}: scored", flush=True)
    report = compare(base_rows, head_rows)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "output_diff.json").write_text(json.dumps(report, indent=2) + "\n")
    text = markdown(report)
    (args.out / "output_diff.md").write_text(text)
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
