#!/usr/bin/env python3
"""Daily usage report of the free-model drafts, from the metrics artifacts of normal runs.

    free_agent_report.py [--days N] [--repo OWNER/REPO]        download and report (gh)
    free_agent_report.py --metrics-dir DIR                     report the metrics.json files under DIR

Each `free-model-draft.yml` run uploads `free-agent-metrics-<run_id>` (metrics.json, see
free_agent_metrics.py). This prints, per UTC day and model: runs, invocations, LLM requests,
tokens (in, out, reasoning, cache read), 429 count, rate-limited runs, accepted drafts and
the mean seconds per invocation. A token total of `?` means no event carried that field;
unknown is never shown as 0. It only reads artifacts: it sends no request to a model.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

DEFAULT_REPO = "fission-systems/Fission"
WORKFLOW = "free-model-draft.yml"
TOTAL = "(all models)"


def load_metrics(root: Path) -> list[dict]:
    found = []
    for path in sorted(root.rglob("metrics.json")):
        try:
            item = json.loads(path.read_text())
        except (OSError, ValueError):
            continue
        if isinstance(item, dict) and isinstance(item.get("invocations"), list):
            found.append(item)
    return found


def _add(total, value):
    if value is None:
        return total
    return value if total is None else total + value


def _empty():
    return {"runs": set(), "invocations": 0, "requests": 0, "tokens_in": None, "tokens_out": None,
            "tokens_reasoning": None, "cache_read": None, "c429": 0, "rate_limited": set(),
            "accepted": 0, "seconds": 0}


def aggregate(metrics: list[dict]) -> dict:
    """{(day, model): totals}, with a `TOTAL` row per day. A run counts for the UTC day it started."""
    rows: dict = {}
    for m in metrics:
        day = str(m.get("started", ""))[:10] or "unknown"
        run = m.get("run_id")
        invocations = [i for i in m.get("invocations", []) if isinstance(i, dict)]
        total = rows.setdefault((day, TOTAL), _empty())
        total["runs"].add(run)
        if m.get("outcome") == "ACCEPTED":
            total["accepted"] += 1
        if m.get("outcome") == "RATE_LIMITED":
            total["rate_limited"].add(run)
        if invocations and m.get("outcome") == "ACCEPTED":
            rows.setdefault((day, invocations[-1].get("model", "unknown")), _empty())["accepted"] += 1
        for inv in invocations:
            model = str(inv.get("model", "unknown"))
            for row in (rows.setdefault((day, model), _empty()), total):
                row["runs"].add(run)
                row["invocations"] += 1
                row["requests"] += inv.get("llm_requests") or 0
                row["seconds"] += inv.get("seconds") or 0
                for key, field in (("tokens_in", "tokens_in"), ("tokens_out", "tokens_out"),
                                   ("tokens_reasoning", "tokens_reasoning"), ("cache_read", "cache_read")):
                    row[key] = _add(row[key], inv.get(field))
                if 429 in (inv.get("status_codes") or []):
                    row["c429"] += 1
                if inv.get("class") == "RATE_LIMITED":
                    row["rate_limited"].add(run)
    return rows


def _show(value) -> str:
    return "?" if value is None else str(value)


def render(rows: dict) -> str:
    lines = ["### Free-model usage per day and model (UTC)", "",
             "| day | model | runs | invocations | LLM requests | tokens in | out | reasoning | cache read | "
             "429s | rate-limited runs | accepted drafts | mean s/invocation |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for (day, model) in sorted(rows, key=lambda k: (k[0], k[1] != TOTAL, k[1])):
        r = rows[(day, model)]
        mean = f"{r['seconds'] / r['invocations']:.0f}" if r["invocations"] else "-"
        lines.append(f"| {day} | {model} | {len(r['runs'])} | {r['invocations']} | {r['requests']} | "
                     f"{_show(r['tokens_in'])} | {_show(r['tokens_out'])} | {_show(r['tokens_reasoning'])} | "
                     f"{_show(r['cache_read'])} | {r['c429']} | {len(r['rate_limited'])} | {r['accepted']} | {mean} |")
    if len(lines) == 4:
        lines.append("| - | no metrics found | | | | | | | | | | | |")
    return "\n".join(lines) + "\n"


def day_totals(rows: dict, day: str) -> dict | None:
    return rows.get((day, TOTAL))


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def collect(repo: str, days: int, now: datetime.datetime | None = None) -> list[dict]:
    """The metrics of the draft runs of the last `days` UTC days, downloaded with gh.
    A run without the artifact (skipped by the guard, or older than this feature) is ignored."""
    now = now or datetime.datetime.now(datetime.timezone.utc)
    since = (now - datetime.timedelta(days=days - 1)).strftime("%Y-%m-%d")
    runs = json.loads(gh("run", "list", "--repo", repo, "--workflow", WORKFLOW, "--limit", "300",
                         "--created", f">={since}", "--json", "databaseId,createdAt"))
    with tempfile.TemporaryDirectory() as tmp:
        for run in runs:
            run_id = str(run["databaseId"])
            try:
                gh("run", "download", run_id, "--repo", repo, "-n", f"free-agent-metrics-{run_id}",
                   "-D", str(Path(tmp) / run_id))
            except (OSError, subprocess.CalledProcessError):
                continue
        return load_metrics(Path(tmp))


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", DEFAULT_REPO))
    ap.add_argument("--metrics-dir", type=Path, help="report the metrics.json files under this directory")
    args = ap.parse_args(argv)
    metrics = load_metrics(args.metrics_dir) if args.metrics_dir else collect(args.repo, args.days)
    text = render(aggregate(metrics))
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
