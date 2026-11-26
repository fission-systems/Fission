#!/usr/bin/env python3
"""Usage metrics of one free-model-draft run (free-model-draft.yml).

    free_agent_metrics.py record --draft DIR --n N --model M --kind draft|repair --seconds S --class C
    free_agent_metrics.py reclass --draft DIR --class C
    free_agent_metrics.py finish --draft DIR [--run-id ID] [--issue N]   write metrics.json, print a table

The loop (free_model_draft_loop.sh) runs every `opencode run` with `--format json` and keeps
its stdout in `DIR/metrics/<n>.jsonl`. Each line is `{type, timestamp, sessionID, ...}`;
`step_finish` is one LLM request and its `part.tokens` holds input, output, reasoning and
`cache.read/write`; `error` carries `error.data.statusCode` and maybe response headers.
Everything is parsed defensively: a field a line lacks is unknown (null), never 0.

metrics.json holds numbers, model ids, class names and status codes only: no prompt, issue
or model text. It is measured from normal runs; nothing probes the quota.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import sys
from pathlib import Path

import free_model_draft as fmd

CLASSES = {"OK", "NONE", fmd.RATE_LIMITED, fmd.MODEL_UNAVAILABLE, fmd.PROVIDER_UNAVAILABLE, fmd.INFRA_FAILURE,
           fmd.COMPILATION_ERROR, fmd.NO_CHANGES, fmd.POLICY_VIOLATION, fmd.BUDGET_EXHAUSTED}
OUTCOMES = CLASSES | {"ACCEPTED", "NO_ACCEPTABLE_DRAFT", "OUT_OF_TIME", "INCOMPLETE"}
MODEL_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/:+-]{0,79}$")


def safe_model(value) -> str:
    return value if isinstance(value, str) and MODEL_RE.match(value) else "unknown"


def safe_class(value, allowed=CLASSES, default="NONE") -> str:
    return value if value in allowed else default


def safe_count(value) -> int | None:
    return fmd._int(value)


def usage_of(jsonl: str) -> dict:
    """Totals of one invocation's JSON events. A token total is None when no `step_finish`
    carried that field (unknown); `llm_requests` counts `step_finish` events."""
    requests, totals, codes, retry, retries = 0, {}, set(), None, 0
    for event in fmd.parse_events(jsonl):
        if event["type"] == "step_finish":
            requests += 1
            for key, value in fmd.event_usage(event).items():
                if value is not None:
                    totals[key] = totals.get(key, 0) + value
        elif event["type"] in ("error", "retry"):
            found = fmd.event_error(event)
            if event["type"] == "retry":
                retries += 1
            if found["status"] is not None:
                codes.add(found["status"])
            if found["retry_after"] is not None:
                retry = found["retry_after"]
    return {"llm_requests": requests, "tokens_in": totals.get("input"), "tokens_out": totals.get("output"),
            "tokens_reasoning": totals.get("reasoning"), "cache_read": totals.get("cache_read"),
            "status_codes": sorted(codes), "retry_after": retry, "retry_events": retries}


def invocation_record(jsonl: str, model: str, kind: str, seconds: int, cls: str, n: int) -> dict:
    record = usage_of(jsonl)
    record.update({"n": safe_count(n) or 0, "model": safe_model(model),
                   "kind": kind if kind in ("draft", "repair") else "draft",
                   "seconds": safe_count(seconds) or 0, "class": safe_class(cls)})
    return record


def sanitize_invocation(raw: dict) -> dict:
    """Re-clean a record read back from disk, so metrics.json only ever holds the safe fields."""
    codes = raw.get("status_codes")
    codes = sorted({c for c in codes if isinstance(c, int) and 100 <= c <= 599}) if isinstance(codes, list) else []
    return {"n": safe_count(raw.get("n")) or 0, "model": safe_model(raw.get("model")),
            "kind": raw.get("kind") if raw.get("kind") in ("draft", "repair") else "draft",
            "seconds": safe_count(raw.get("seconds")) or 0,
            "llm_requests": safe_count(raw.get("llm_requests")) or 0,
            "tokens_in": safe_count(raw.get("tokens_in")), "tokens_out": safe_count(raw.get("tokens_out")),
            "tokens_reasoning": safe_count(raw.get("tokens_reasoning")),
            "cache_read": safe_count(raw.get("cache_read")),
            "status_codes": codes, "retry_after": safe_count(raw.get("retry_after")),
            "retry_events": safe_count(raw.get("retry_events")) or 0,
            "class": safe_class(raw.get("class"))}


def iso(epoch: int) -> str:
    return datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def build_metrics(invocations: list[dict], run_id, issue, outcome, started: int, finished: int) -> dict:
    return {"run_id": safe_count(int(run_id)) if str(run_id).isdigit() else None,
            "issue": safe_count(int(issue)) if str(issue).isdigit() else None,
            "outcome": safe_class(outcome, OUTCOMES, "INCOMPLETE"),
            "started": iso(started), "finished": iso(finished),
            "invocations": [sanitize_invocation(i) for i in invocations]}


def _show(value) -> str:
    return "?" if value is None else str(value)


def summary_table(metrics: dict) -> str:
    lines = [f"### Free-model usage (outcome `{metrics['outcome']}`)", "",
             "| # | model | kind | seconds | LLM requests | tokens in | out | reasoning | cache read | status codes | class |",
             "|---|---|---|---|---|---|---|---|---|---|---|"]
    for i in metrics["invocations"]:
        lines.append(f"| {i['n']} | `{i['model']}` | {i['kind']} | {i['seconds']} | {i['llm_requests']} | "
                     f"{_show(i['tokens_in'])} | {_show(i['tokens_out'])} | {_show(i['tokens_reasoning'])} | "
                     f"{_show(i['cache_read'])} | {','.join(map(str, i['status_codes'])) or '-'} | {i['class']} |")
    if not metrics["invocations"]:
        lines.append("| - | no opencode invocation | | | | | | | | | |")
    return "\n".join(lines) + "\n"


def _metrics_dir(draft: Path) -> Path:
    return draft / "metrics"


def read_records(draft: Path) -> list[dict]:
    path = _metrics_dir(draft) / "invocations.jsonl"
    records = []
    if path.exists():
        for line in path.read_text(errors="replace").splitlines():
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if isinstance(item, dict):
                records.append(item)
    return records


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("record")
    r.add_argument("--draft", type=Path, required=True)
    r.add_argument("--n", type=int, required=True)
    r.add_argument("--model", required=True)
    r.add_argument("--kind", required=True)
    r.add_argument("--seconds", type=int, required=True)
    r.add_argument("--class", dest="cls", required=True)
    c = sub.add_parser("reclass")
    c.add_argument("--draft", type=Path, required=True)
    c.add_argument("--class", dest="cls", required=True)
    f = sub.add_parser("finish")
    f.add_argument("--draft", type=Path, required=True)
    f.add_argument("--run-id", default=os.environ.get("GITHUB_RUN_ID", ""))
    f.add_argument("--issue", default=os.environ.get("ISSUE", ""))
    args = parser.parse_args(argv)
    directory = _metrics_dir(args.draft)

    if args.command == "record":
        source = directory / f"{args.n}.jsonl"
        text = source.read_text(errors="replace") if source.exists() else ""
        record = invocation_record(text, args.model, args.kind, args.seconds, args.cls, args.n)
        directory.mkdir(parents=True, exist_ok=True)
        with (directory / "invocations.jsonl").open("a") as fh:
            fh.write(json.dumps(record) + "\n")
        return 0
    if args.command == "reclass":
        records = read_records(args.draft)
        if records:
            records[-1]["class"] = safe_class(args.cls)
            (directory / "invocations.jsonl").write_text("".join(json.dumps(x) + "\n" for x in records))
        return 0
    state = args.draft / "state"
    now = int(datetime.datetime.now(datetime.timezone.utc).timestamp())
    try:
        started = int((state / "start").read_text().strip())
    except (OSError, ValueError):
        started = now
    try:
        outcome = (state / "outcome").read_text().strip()
    except OSError:
        outcome = "INCOMPLETE"
    metrics = build_metrics(read_records(args.draft), args.run_id, args.issue, outcome, started, now)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "metrics.json").write_text(json.dumps(metrics, indent=1) + "\n")
    print(summary_table(metrics))
    return 0


if __name__ == "__main__":
    sys.exit(main())
