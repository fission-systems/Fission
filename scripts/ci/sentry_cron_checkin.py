#!/usr/bin/env python3
"""Send one Sentry Crons check-in for the nightly Heavy run.

``ci-heavy.yml`` calls this twice per scheduled run: ``in_progress`` when the
run starts and ``ok``/``error`` once ``heavy-summary`` has a result. Every
check-in carries ``monitor_config``, so Sentry creates or updates the monitor
from this file (upsert) and nobody has to configure it by hand.

The check-in goes to Relay's HTTP endpoint, addressed by the DSN in the
``SENTRY_DSN`` environment variable (``/api/<project>/cron/<slug>/<key>/``,
JSON body = Relay's ``CheckIn``). Both check-ins of one run attempt share a
``check_in_id`` derived from the run id and attempt, so they pair without
passing state between jobs, and a re-run attempt reports as a check-in of its
own instead of rewriting a finished one.

Monitoring never decides the run: a missing DSN, a malformed DSN or a failed
request prints a GitHub annotation and the script still exits 0.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid

MONITOR_SLUG = "ci-heavy-nightly"

# Must match `on.schedule` in .github/workflows/ci-heavy.yml.
SCHEDULE = "30 2 * * *"

# GitHub starts this schedule late, and by a lot: over 100 nightly runs
# (2026-08-18 .. 2026-11-25) the run was created 43-717 minutes after 02:30
# UTC, and 400-614 minutes late through November. A margin under that would
# report a missed run every night. 900 minutes still flags a night with no
# run at 17:30 UTC the same day.
CHECKIN_MARGIN_MINUTES = 900

# The longest needs-chain is the Windows heavy build (30 min timeout) then the
# Windows heavy tests (120 min); add queueing slack. Real runs take 12-33 min.
MAX_RUNTIME_MINUTES = 180

MONITOR_CONFIG = {
    "schedule": {"type": "crontab", "value": SCHEDULE},
    "checkin_margin": CHECKIN_MARGIN_MINUTES,
    "max_runtime": MAX_RUNTIME_MINUTES,
    "timezone": "UTC",
    "failure_issue_threshold": 1,
    "recovery_threshold": 1,
}

STATUSES = ("in_progress", "ok", "error")
TIMEOUT_SECONDS = 15


class DsnError(ValueError):
    pass


def checkin_url(dsn: str, slug: str = MONITOR_SLUG) -> str:
    """Relay's cron endpoint for ``slug`` under the project the DSN names."""
    parts = urllib.parse.urlsplit(dsn.strip())
    if parts.scheme not in ("http", "https") or not parts.hostname:
        raise DsnError("DSN is not an http(s) URL with a host")
    key = parts.username
    if not key:
        raise DsnError("DSN has no public key")
    prefix, _, project_id = parts.path.rstrip("/").rpartition("/")
    if not project_id.isdigit():
        raise DsnError("DSN does not end in a numeric project id")
    netloc = parts.hostname + (f":{parts.port}" if parts.port else "")
    return f"{parts.scheme}://{netloc}{prefix}/api/{project_id}/cron/{slug}/{key}/"


def checkin_id(server_url: str, repository: str, run_id: str, run_attempt: str) -> str:
    """Same id for both check-ins of one run attempt, a new one per attempt."""
    name = f"{server_url}/{repository}/actions/runs/{run_id}/attempts/{run_attempt}"
    return str(uuid.uuid5(uuid.NAMESPACE_URL, name))


def status_for_result(result: str) -> str:
    """Map a GitHub job result to a final check-in status."""
    return "ok" if result == "success" else "error"


def payload(status: str, check_in: str) -> dict:
    if status not in STATUSES:
        raise ValueError(f"unknown status {status!r}")
    return {
        "check_in_id": check_in,
        "status": status,
        "monitor_config": MONITOR_CONFIG,
    }


def send(url: str, body: dict, opener=urllib.request.urlopen) -> int:
    request = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with opener(request, timeout=TIMEOUT_SECONDS) as response:
        return response.status


def main(argv=None, env=None, opener=urllib.request.urlopen) -> int:
    env = os.environ if env is None else env
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--status", choices=STATUSES)
    group.add_argument(
        "--result",
        help="GitHub job result of the aggregating job; success -> ok, anything else -> error",
    )
    args = parser.parse_args(argv)
    status = args.status or status_for_result(args.result)

    dsn = env.get("SENTRY_DSN", "")
    if not dsn.strip():
        print("::notice title=Sentry cron check-in skipped::SENTRY_DSN is not set; "
              f"no {status} check-in for {MONITOR_SLUG}.")
        return 0
    try:
        url = checkin_url(dsn)
    except DsnError as err:
        print(f"::warning title=Sentry cron check-in skipped::SENTRY_DSN is malformed ({err}).")
        return 0

    check_in = checkin_id(
        env.get("GITHUB_SERVER_URL", "https://github.com"),
        env.get("GITHUB_REPOSITORY", ""),
        env.get("GITHUB_RUN_ID", ""),
        env.get("GITHUB_RUN_ATTEMPT", "1"),
    )
    try:
        code = send(url, payload(status, check_in), opener=opener)
    except (urllib.error.URLError, OSError, ValueError) as err:
        # URLError covers HTTPError. Name the error, never str(err): some
        # messages quote the URL, and the URL carries the DSN key.
        detail = getattr(err, "code", None) or getattr(err, "reason", None) \
            or getattr(err, "strerror", None) or ""
        print(f"::warning title=Sentry cron check-in failed::{status} for {MONITOR_SLUG}: "
              f"{type(err).__name__} {detail}".rstrip())
        return 0
    print(f"Sentry cron check-in sent: monitor={MONITOR_SLUG} status={status} "
          f"check_in_id={check_in} http={code}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
