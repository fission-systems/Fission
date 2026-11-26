#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Send an output-ledger entry's totals to Sentry, so a dashboard can plot
them across main commits.

One transaction per entry (`output_ledger.py record`), named `output-ledger`:
- measurements: the sample's totals (rows, rows with output, failed calls,
  perfect, distance, types perfect) and the speed sample's instructions;
- tags: the commit and the runner's CPU model. Instruction counts are only
  comparable within one CPU model, so a chart of them should group by `cpu`.

Only those numbers, the commit hash and the CPU model leave the runner. It
sends no function key, output hash, decompiled text or binary name. The
commit must be 40 hex digits and the CPU model a short plain string;
anything else is rejected, never cleaned up.

Sends only from push and workflow_dispatch runs of fission-systems/Fission
on main, and only when SENTRY_DSN is set; otherwise it is a dry run. Either
way it writes what was (or would be) sent to the job summary and exits 0
once its arguments parse: reporting never changes a job's result.

    sentry_ledger.py ENTRY.json [--dry-run]
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import re
import sys
import urllib.error
import urllib.request
import uuid
from pathlib import Path

import sentry_defects

REPOSITORY = "fission-systems/Fission"
TRANSACTION = "output-ledger"
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
CPU_RE = re.compile(r"^[A-Za-z0-9 ()@.,_+-]{1,100}$")
# Summary field -> Sentry measurement unit.
MEASUREMENTS = {
    "rows": "none",
    "rows_with_output": "none",
    "failed_calls": "none",
    "perfect": "none",
    "distance": "none",
    "types_perfect": "none",
}


def build_event(entry: dict, now: _dt.datetime, event_id: str | None = None, trace_id: str | None = None) -> dict:
    """The transaction for one entry. ValueError when a value fails its check."""
    commit, cpu = entry["commit"], entry["cpu"]
    if not COMMIT_RE.fullmatch(commit):
        raise ValueError("commit is not a 40-digit hex hash")
    if not CPU_RE.fullmatch(cpu):
        raise ValueError("cpu model is not a short plain string")
    measurements = {}
    for key, unit in MEASUREMENTS.items():
        value = entry["summary"][key]
        if not isinstance(value, (int, float)) or isinstance(value, bool):
            raise ValueError(f"{key} is not a number")
        measurements[key] = {"value": value, "unit": unit}
    if entry.get("speed"):
        instructions = entry["speed"]["instructions"]
        if not isinstance(instructions, int) or isinstance(instructions, bool):
            raise ValueError("instructions is not an integer")
        measurements["instructions"] = {"value": instructions, "unit": "none"}
    stamp = now.timestamp()
    return {
        "event_id": event_id or uuid.uuid4().hex,
        "type": "transaction",
        "transaction": TRANSACTION,
        "platform": "other",
        "environment": "ci",
        "start_timestamp": stamp,
        "timestamp": stamp,
        "contexts": {"trace": {"trace_id": trace_id or uuid.uuid4().hex, "span_id": uuid.uuid4().hex[:16],
                               "op": "ci.ledger", "status": "ok"}},
        "tags": {"commit": commit, "cpu": cpu, "dirty": str(bool(entry.get("dirty"))).lower()},
        "measurements": measurements,
    }


def envelope(event: dict) -> bytes:
    payload = json.dumps(event, separators=(",", ":"), sort_keys=True).encode()
    sent_at = _dt.datetime.fromtimestamp(event["timestamp"], _dt.timezone.utc).isoformat()
    header = json.dumps({"event_id": event["event_id"], "sent_at": sent_at}).encode()
    item = json.dumps({"type": "transaction", "length": len(payload), "content_type": "application/json"}).encode()
    return header + b"\n" + item + b"\n" + payload + b"\n"


def trusted(env: dict) -> bool:
    return (env.get("GITHUB_REPOSITORY") == REPOSITORY
            and env.get("GITHUB_EVENT_NAME") in ("push", "workflow_dispatch")
            and env.get("GITHUB_REF") == "refs/heads/main")


def send(event: dict, dsn: str, opener=None) -> str:
    url, auth = sentry_defects.envelope_target(dsn)
    request = urllib.request.Request(url, data=envelope(event), method="POST", headers={
        "Content-Type": "application/x-sentry-envelope",
        "X-Sentry-Auth": auth,
        "User-Agent": sentry_defects.CLIENT,
    })
    try:
        with (opener or urllib.request.urlopen)(request, timeout=15) as response:
            return f"sent (HTTP {response.status})"
    except urllib.error.HTTPError as err:
        return f"failed (HTTP {err.code})"
    except (urllib.error.URLError, OSError) as err:
        return f"failed ({type(err).__name__})"


def run(entry_path: Path, dry_run: bool, env: dict, opener=None, now: _dt.datetime | None = None) -> tuple[str, str]:
    """(outcome, summary markdown)."""
    try:
        event = build_event(json.loads(entry_path.read_text()), now or _dt.datetime.now(_dt.timezone.utc))
    except (OSError, ValueError, KeyError, TypeError) as err:
        return "rejected", f"## Sentry ledger metrics\n\nNot sent: {err}.\n"
    dsn = env.get("SENTRY_DSN", "").strip()
    if dry_run or not dsn or not trusted(env):
        outcome = "dry run"
    else:
        try:
            outcome = send(event, dsn, opener)
        except ValueError as err:
            outcome = f"not sent ({err})"
    lines = ["## Sentry ledger metrics", "", f"Mode: **{outcome}**, commit `{event['tags']['commit'][:12]}`, "
             f"cpu `{event['tags']['cpu']}`.", "", "| measurement | value |", "|---|---:|"]
    lines += [f"| {k} | {v['value']:g} |" for k, v in event["measurements"].items()]
    return outcome, "\n".join(lines) + "\n"


def main(argv: list[str] | None = None, env: dict | None = None, opener=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("entry", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    env = os.environ if env is None else env
    _, text = run(args.entry, args.dry_run, env, opener)
    print(text)
    if env.get("GITHUB_STEP_SUMMARY"):
        with open(env["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
