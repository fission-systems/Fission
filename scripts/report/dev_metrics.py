#!/usr/bin/env python3
"""Development-flow report: who acts next on each open PR, and how long PRs take.

Markdown on stdout (CI appends it to $GITHUB_STEP_SUMMARY). It reads GitHub
through `gh api` and writes nothing back: no issues, comments or commits.

    dev_metrics.py --days 7 --runner-metrics --json-out now.json
    dev_metrics.py --days 1                        # the daily run
    dev_metrics.py --compare docs/validation/2026-11-24-dev-infra-baseline.json

Sections
  Action required   only what someone can do now (see ACTION_RULES).
  Who acts next     every open non-draft PR in exactly one state, triagebot style.
  Flow metrics      lead time split by required checks and the merge queue,
                    merged PRs/day and /hour, queue ejections, CI failure rate,
                    issues, the maintainer-intervention proxy.
  Runner metrics    (--runner-metrics) job queue/run time by event, runner
                    minutes per merged PR, merge-group regenerations and the
                    PR-vs-merge-group duplicate cost.

`--raw-in FILE` replays a saved collection (`--raw-out`) instead of calling
GitHub; the tests use it with a canned fixture.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import subprocess
import sys
import time
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from pathlib import Path

REPO = "fission-systems/Fission"
CI_WORKFLOW = "ci.yml"
# Branch protection's required checks. The base-vs-head job is report-only.
REQUIRED = ("Detect changes", "CI Summary")
BEHIND_LIMIT = 50
STALE_DAYS = 7
MIN_JOB_SECONDS = 5
DEPENDENCY = re.compile(r"(?i)\b(?:after|depends on)\s+#(\d+)")
MERGE_MAIN = re.compile(r"(?i)^Merge (?:remote-tracking )?branch '(?:origin/)?main'")
QUEUE_BRANCH = re.compile(r"^gh-readonly-queue/[^/]+/pr-(\d+)-")
FAILED = {"failure", "timed_out", "cancelled", "action_required", "startup_failure", "stale"}

STATES = (
    ("queued", "Queued", "merge queue"),
    ("waiting_ci", "Waiting for CI", "Actions"),
    ("changes_requested", "Changes requested", "author"),
    ("waiting_dependency", "Waiting for dependency", "the other PR"),
    ("ready_to_enqueue", "Ready to enqueue", "merger"),
    ("waiting_review", "Waiting for review", "reviewer"),
)

# ---------------------------------------------------------------------------
# Collection (the only part that talks to GitHub)

PR_FIELDS = """
number title url isDraft createdAt mergedAt baseRefName headRefName headRefOid body
author { login }
mergeable reviewDecision
autoMergeRequest { enabledAt }
mergeQueueEntry { enqueuedAt }
reviewThreads(first: 100) { nodes { isResolved } }
reviews(last: 1) { nodes { state submittedAt } }
timelineItems(last: 100, itemTypes: [ADDED_TO_MERGE_QUEUE_EVENT, REMOVED_FROM_MERGE_QUEUE_EVENT,
    HEAD_REF_FORCE_PUSHED_EVENT, AUTO_MERGE_ENABLED_EVENT]) {
  nodes {
    __typename
    ... on AddedToMergeQueueEvent { createdAt actor { login } }
    ... on RemovedFromMergeQueueEvent { createdAt reason actor { login } }
    ... on HeadRefForcePushedEvent { createdAt actor { login } }
    ... on AutoMergeEnabledEvent { createdAt actor { login } }
  }
}
history: commits(last: 100) { nodes { commit { messageHeadline parents { totalCount } } } }
head: commits(last: 1) { nodes { commit { oid checkSuites(first: 30) { nodes {
  createdAt workflowRun { event }
  summary: checkRuns(first: 5, filterBy: {checkName: "CI Summary"}) {
    nodes { name status conclusion startedAt completedAt } }
  detect: checkRuns(first: 5, filterBy: {checkName: "Detect changes"}) {
    nodes { name status conclusion startedAt completedAt } }
} } } } }
"""


class GitHub:
    """`gh api` with an in-process cache, so one run never asks twice."""

    def __init__(self, repo: str):
        self.repo = repo
        self.owner, self.name = repo.split("/")
        self.cache: dict[str, object] = {}
        self.calls = 0

    def _gh(self, args: list[str]) -> object:
        key = "\0".join(args)
        if key not in self.cache:
            for attempt in range(4):
                self.calls += 1
                out = subprocess.run(["gh", "api", *args], capture_output=True, text=True)
                if out.returncode == 0:
                    break
                if "404" in out.stderr or "422" in out.stderr or attempt == 3:
                    raise subprocess.CalledProcessError(out.returncode, out.args, out.stdout, out.stderr)
                time.sleep(2 ** attempt * 5)  # transient 5xx or secondary rate limit
            self.cache[key] = json.loads(out.stdout) if out.stdout.strip() else None
        return self.cache[key]

    def graphql(self, query: str, **variables) -> dict:
        args = ["graphql", "-f", f"query={query}"]
        for k, v in sorted(variables.items()):
            if v is not None:
                args += ["-F" if isinstance(v, int) else "-f", f"{k}={v}"]
        data = self._gh(args)
        if data.get("errors"):
            raise RuntimeError(data["errors"])
        return data["data"]

    def rest(self, path: str) -> object:
        return self._gh([path])

    def rest_pages(self, path: str, key: str) -> list:
        """A paginated REST list; stops at the first short page."""
        items, page = [], 1
        sep = "&" if "?" in path else "?"
        while True:
            data = self.rest(f"{path}{sep}per_page=100&page={page}")
            batch = data[key]
            items += batch
            if len(batch) < 100:
                return items
            page += 1

    def search_prs(self, query: str) -> list[dict]:
        q = """query($q: String!, $after: String) { search(type: ISSUE, query: $q, first: 20, after: $after) {
          pageInfo { hasNextPage endCursor } nodes { ... on PullRequest { %s } } } }""" % PR_FIELDS
        nodes, after = [], None
        while True:
            data = self.graphql(q, q=query, after=after)["search"]
            nodes += data["nodes"]
            if not data["pageInfo"]["hasNextPage"]:
                return nodes
            after = data["pageInfo"]["endCursor"]

    def issue_count(self, query: str) -> int:
        q = "query($q: String!) { search(type: ISSUE, query: $q, first: 1) { issueCount } }"
        return self.graphql(q, q=f"repo:{self.repo} {query}")["search"]["issueCount"]

    def states(self, numbers: list[int]) -> dict[int, str]:
        if not numbers:
            return {}
        fields = " ".join(
            f"n{n}: issueOrPullRequest(number: {n}) {{ ... on Issue {{ state }} ... on PullRequest {{ state }} }}"
            for n in numbers
        )
        q = f'{{ repository(owner: "{self.owner}", name: "{self.name}") {{ {fields} }} }}'
        repo = self.graphql(q)["repository"]
        return {n: (repo.get(f"n{n}") or {}).get("state", "UNKNOWN") for n in numbers}


def iso(t: datetime) -> str:
    return t.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def collect(gh: GitHub, since: datetime, until: datetime, runner_metrics: bool) -> dict:
    span = f"{iso(since)}..{iso(until)}"
    merged = gh.search_prs(f"repo:{gh.repo} is:pr is:merged merged:{span}")
    # The open state is always the state now: GitHub has no history of it.
    open_prs = gh.search_prs(f"repo:{gh.repo} is:pr is:open")

    deps = sorted({int(n) for pr in open_prs for n in DEPENDENCY.findall(pr.get("body") or "")})
    behind = {}
    for pr in open_prs:
        try:
            cmp = gh.rest(f"repos/{gh.repo}/compare/main...{pr['headRefOid']}")
            behind[str(pr["number"])] = cmp["behind_by"]
        except subprocess.CalledProcessError:
            behind[str(pr["number"])] = None

    raw = {
        "repo": gh.repo,
        "since": since.isoformat(),
        "now": until.isoformat(),
        "collected_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "merged": merged,
        "open": open_prs,
        "dependency_states": {str(k): v for k, v in gh.states(deps).items()},
        "behind": behind,
        "issues": {
            "opened": gh.issue_count(f"is:issue created:{span}"),
            "closed": gh.issue_count(f"is:issue closed:{span}"),
            "open_unlabeled": gh.issue_count("is:issue is:open no:label"),
        },
    }

    # Required-check outcome per Fast Gate run, from the run's own job list.
    # With --runner-metrics every workflow's runs in the window are kept (the
    # concurrency limit is shared by all of them), plus a week of Fast Gate
    # runs before it to attribute older pull_request runs to PRs merged in it.
    lookback = timedelta(days=7) if runner_metrics else timedelta(0)
    first = iso(since - lookback)
    runs = gh.rest_pages(
        f"repos/{gh.repo}/actions/workflows/{CI_WORKFLOW}/runs?created={first}..{iso(until)}", "workflow_runs"
    )
    if runner_metrics:
        runs += gh.rest_pages(f"repos/{gh.repo}/actions/runs?created={span}", "workflow_runs")
    main_runs = gh.rest(
        f"repos/{gh.repo}/actions/workflows/{CI_WORKFLOW}/runs?branch=main&event=push&status=completed&per_page=5"
    )["workflow_runs"]
    keep = [
        {k: r.get(k) for k in ("id", "event", "status", "conclusion", "created_at", "run_attempt",
                                "head_sha", "head_branch", "path")}
        | {"pull_requests": [p["number"] for p in r.get("pull_requests") or []]}
        for r in runs + main_runs
    ]
    keep = list({r["id"]: r for r in keep}.values())
    main_ids = {r["id"] for r in main_runs[:1]}
    merged_numbers = {pr["number"] for pr in merged}
    merged_branches = {pr.get("headRefName") for pr in merged}
    start, end = iso(since), iso(until)

    def needed(r: dict) -> bool:
        if r["id"] in main_ids:
            return True
        if start <= r["created_at"] < end:
            return runner_metrics or (is_fast_gate(r) and r["event"] in ("pull_request", "merge_group"))
        if not is_fast_gate(r):
            return False
        # Before the window: only the runs of PRs merged in it, for attribution.
        if r["event"] == "pull_request":
            return bool(merged_numbers & set(r["pull_requests"])) or r.get("head_branch") in merged_branches
        m = QUEUE_BRANCH.match(r.get("head_branch") or "")
        return r["event"] == "merge_group" and bool(m) and int(m.group(1)) in merged_numbers

    wanted = [r for r in keep if needed(r)]

    def jobs(run: dict) -> tuple[int, list]:
        listing = gh.rest_pages(f"repos/{gh.repo}/actions/runs/{run['id']}/jobs?filter=latest", "jobs")
        return run["id"], [
            {k: j.get(k) for k in ("name", "status", "conclusion", "created_at", "started_at", "completed_at",
                                    "labels", "runner_name")}
            for j in listing
        ]

    with ThreadPoolExecutor(max_workers=4) as pool:
        job_map = dict(pool.map(jobs, wanted))
    for run in keep:
        run["jobs"] = job_map.get(run["id"])
    raw["runs"] = sorted(keep, key=lambda r: r["id"])
    raw["main_runs"] = sorted(main_ids)
    raw["runner_metrics"] = runner_metrics
    raw["api_calls"] = gh.calls
    return raw


def is_fast_gate(run: dict) -> bool:
    """A CI Fast Gate run (runs saved before `path` was kept are all Fast Gate)."""
    return run.get("path", f".github/workflows/{CI_WORKFLOW}").endswith(f"/{CI_WORKFLOW}")


# ---------------------------------------------------------------------------
# Analysis (pure: raw collection in, numbers out)


def ts(value: str | None) -> datetime | None:
    return datetime.fromisoformat(value.replace("Z", "+00:00")) if value else None


def hours(delta: timedelta) -> float:
    return delta.total_seconds() / 3600


def pct(values: list[float], q: float) -> float | None:
    """Nearest-rank percentile; None for an empty sample."""
    if not values:
        return None
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q / 100 * len(ordered)) - 1)]


def events(pr: dict, kind: str) -> list[dict]:
    return sorted(
        (n for n in pr["timelineItems"]["nodes"] if n.get("__typename") == kind),
        key=lambda n: n["createdAt"],
    )


def head_required_runs(pr: dict) -> dict[str, list[dict]]:
    """Required check runs on the head commit from pull_request suites, by name."""
    found: dict[str, list[dict]] = defaultdict(list)
    for node in pr["head"]["nodes"]:
        for suite in node["commit"]["checkSuites"]["nodes"]:
            if (suite.get("workflowRun") or {}).get("event") != "pull_request":
                continue
            for alias in ("summary", "detect"):
                for run in suite[alias]["nodes"]:
                    found[run["name"]].append(run | {"suiteCreatedAt": suite["createdAt"]})
    return found


def checks_state(pr: dict) -> tuple[str, datetime | None, datetime | None]:
    """('success' | 'pending' | 'failure', last push, checks done) for the head commit."""
    runs = head_required_runs(pr)
    pushes = [ts(r["suiteCreatedAt"]) for rs in runs.values() for r in rs]
    pushed = max(pushes) if pushes else None
    latest = {}
    for name in REQUIRED:
        candidates = sorted(runs.get(name, []), key=lambda r: (r["startedAt"] or "", r["completedAt"] or ""))
        if not candidates:
            return "pending", pushed, None
        latest[name] = candidates[-1]
    if any(r["status"] != "COMPLETED" for r in latest.values()):
        if any((r["conclusion"] or "").lower() in FAILED for r in latest.values()):
            return "failure", pushed, None
        return "pending", pushed, None
    if any((r["conclusion"] or "").lower() != "success" for r in latest.values()):
        return "failure", pushed, max(ts(r["completedAt"]) for r in latest.values())
    return "success", pushed, max(ts(r["completedAt"]) for r in latest.values())


def lead_time(pr: dict) -> dict | None:
    """Last push -> required checks done -> added to queue -> merged, in hours.

    None when the PR never entered the merge queue or has no required checks
    on its head; the caller counts those separately.
    """
    merged = ts(pr["mergedAt"])
    adds = [ts(e["createdAt"]) for e in events(pr, "AddedToMergeQueueEvent")]
    adds = [a for a in adds if a <= merged]
    if not adds:
        return None
    queued = adds[-1]
    runs = head_required_runs(pr)
    if not all(runs.get(name) for name in REQUIRED):
        return None
    pushed = min(ts(r["suiteCreatedAt"]) for rs in runs.values() for r in rs)
    done = [ts(r["completedAt"]) for rs in runs.values() for r in rs if r["completedAt"]]
    before = [d for d in done if d <= queued]
    # Auto-merge queues the moment checks finish: clamp to the queue time.
    checks_done = max(before) if before else queued
    return {
        "number": pr["number"],
        "push_to_checks": hours(checks_done - pushed),
        "checks_to_queue": hours(queued - checks_done),
        "queue_to_merged": hours(merged - queued),
        "total": hours(merged - pushed),
    }


def classify(pr: dict, dependency_states: dict[str, str]) -> tuple[str, datetime | None]:
    """Exactly one triage state for an open non-draft PR, and since when."""
    if pr.get("mergeQueueEntry") or pr.get("autoMergeRequest"):
        since = (pr.get("mergeQueueEntry") or {}).get("enqueuedAt") or pr["autoMergeRequest"]["enabledAt"]
        return "queued", ts(since)
    state, pushed, checks_done = checks_state(pr)
    if state == "pending":
        return "waiting_ci", pushed
    unresolved = sum(1 for t in pr["reviewThreads"]["nodes"] if not t["isResolved"])
    review = (pr["reviews"]["nodes"] or [{}])[-1]
    if (state == "failure" or pr.get("reviewDecision") == "CHANGES_REQUESTED" or unresolved
            or pr.get("mergeable") == "CONFLICTING"):
        since = ts(review.get("submittedAt")) if pr.get("reviewDecision") == "CHANGES_REQUESTED" else checks_done
        return "changes_requested", since
    deps = [n for n in DEPENDENCY.findall(pr.get("body") or "") if dependency_states.get(n) == "OPEN"]
    if pr.get("baseRefName") != "main" or deps:
        return "waiting_dependency", ts(pr["createdAt"])
    if pr.get("reviewDecision") in ("APPROVED", None) and pr.get("mergeable") == "MERGEABLE":
        return "ready_to_enqueue", checks_done
    return "waiting_review", checks_done


def analyze(raw: dict) -> dict:
    now = ts(raw["now"])
    since = ts(raw["since"]) if raw.get("since") else now - timedelta(days=raw["days"])
    days = (now - since).total_seconds() / 86400

    def in_window(value: str) -> bool:
        return since <= ts(value) < now

    # Flow: merged PRs.
    leads, no_queue, no_checks = [], [], []
    for pr in sorted(raw["merged"], key=lambda p: p["number"]):
        lt = lead_time(pr)
        if lt is not None:
            leads.append(lt)
        elif events(pr, "AddedToMergeQueueEvent"):
            no_checks.append(pr["number"])
        else:
            no_queue.append(pr["number"])
    per_hour = Counter(ts(pr["mergedAt"]).strftime("%Y-%m-%d %H:00") for pr in raw["merged"])

    def split(key: str) -> dict:
        values = [lt[key] for lt in leads]
        return {"p50": pct(values, 50), "p95": pct(values, 95), "max": max(values, default=None),
                "n": len(values)}

    # Merge queue: ejections are removals for any reason but a merge.
    ejections, requeues = Counter(), 0
    prs = {pr["number"]: pr for pr in raw["merged"] + raw["open"]}
    for pr in prs.values():
        removed = [e for e in events(pr, "RemovedFromMergeQueueEvent")
                   if e.get("reason") != "merged" and ts(e["createdAt"]) >= since]
        for e in removed:
            ejections[e.get("reason") or "unknown"] += 1
        added = events(pr, "AddedToMergeQueueEvent")
        requeues += sum(1 for a in added if any(r["createdAt"] < a["createdAt"] for r in removed))

    # CI failure rate: the required "CI Summary" job per run in the window.
    ci = {}
    for event in ("pull_request", "merge_group"):
        outcomes = Counter()
        for run in raw["runs"]:
            if (run["event"] != event or not in_window(run["created_at"]) or not run.get("jobs")
                    or not is_fast_gate(run)):
                continue
            summary = [j for j in run["jobs"] if j["name"] == "CI Summary"]
            outcomes[(summary[0]["conclusion"] if summary else None) or "not run"] += 1
        decided = outcomes["success"] + outcomes["failure"]
        ci[event] = {
            "runs": decided,
            "failed": outcomes["failure"],
            "failure_rate": outcomes["failure"] / decided if decided else None,
            "cancelled": outcomes["cancelled"],
            "other": sum(v for k, v in outcomes.items() if k not in ("success", "failure", "cancelled")),
        }

    # Maintainer-intervention proxy (see docs/validation baseline for scope).
    window_prs = [p for p in prs.values() if (ts(p.get("mergedAt")) or now) >= since]
    merge_main = sum(
        1 for p in window_prs for c in p["history"]["nodes"]
        if c["commit"]["parents"]["totalCount"] > 1 and MERGE_MAIN.match(c["commit"]["messageHeadline"])
    )
    force_pushes = sum(1 for p in window_prs for e in events(p, "HeadRefForcePushedEvent")
                       if ts(e["createdAt"]) >= since)
    queue_actors = Counter(
        (e.get("actor") or {}).get("login", "?") for p in window_prs
        for e in events(p, "AddedToMergeQueueEvent") if ts(e["createdAt"]) >= since
    )
    reruns = sum(1 for r in raw["runs"]
                 if (r.get("run_attempt") or 1) > 1 and in_window(r["created_at"]) and is_fast_gate(r))

    # Open PRs: who acts next.
    triage = defaultdict(list)
    drafts = []
    for pr in sorted(raw["open"], key=lambda p: p["number"]):
        if pr["isDraft"]:
            drafts.append(pr["number"])
            continue
        state, since_at = classify(pr, raw["dependency_states"])
        triage[state].append({
            "number": pr["number"], "title": pr["title"],
            "age_in_state_h": max(0.0, hours(now - since_at)) if since_at else None,
        })
    ages = [hours(now - ts(p["createdAt"])) for p in raw["open"]]

    # Action required.
    main_run = max((r for r in raw["runs"] if r["id"] in raw["main_runs"] and r.get("jobs")),
                   key=lambda r: r["id"], default=None)
    main_summary = None
    if main_run:
        summary = [j for j in main_run["jobs"] if j["name"] == "CI Summary"]
        main_summary = (summary[0]["conclusion"] or summary[0]["status"]) if summary else "not run"
    out_of_date = []
    for pr in sorted(raw["open"], key=lambda p: p["number"]):
        behind = raw["behind"].get(str(pr["number"]))
        if pr.get("mergeable") == "CONFLICTING" or (behind or 0) > BEHIND_LIMIT:
            out_of_date.append({"number": pr["number"], "title": pr["title"],
                                "conflicting": pr.get("mergeable") == "CONFLICTING", "behind": behind})
    stale = [{"number": p["number"], "title": p["title"], "age_d": hours(now - ts(p["createdAt"])) / 24}
             for p in sorted(raw["open"], key=lambda p: p["number"])
             if now - ts(p["createdAt"]) > timedelta(days=STALE_DAYS)]

    metrics = {
        "window": {"days": days, "since": since.isoformat(), "now": now.isoformat(),
                   "open_state_at": raw.get("collected_at", now.isoformat())},
        "lead_time_h": {k: split(k) for k in ("push_to_checks", "checks_to_queue", "queue_to_merged", "total")},
        "merged": len(raw["merged"]),
        "merged_per_day": len(raw["merged"]) / days,
        "merged_per_hour": len(raw["merged"]) / (days * 24),
        "merged_peak_hour": max(per_hour.values(), default=0),
        "merged_without_queue": no_queue,
        "merged_without_required_checks": no_checks,
        "queue": {"ejections": sum(ejections.values()), "by_reason": dict(sorted(ejections.items())),
                  "requeues_after_ejection": requeues},
        "ci": ci,
        "issues": raw["issues"],
        "open_prs": {"count": len(raw["open"]), "drafts": len(drafts),
                     "age_p50_d": (pct(ages, 50) or 0) / 24, "age_max_d": max(ages, default=0) / 24},
        "intervention": {"merge_main_commits": merge_main, "force_pushes": force_pushes,
                         "requeues_after_ejection": requeues, "workflow_reruns": reruns,
                         "queue_adds_by_actor": dict(sorted(queue_actors.items()))},
        "triage": {state: triage.get(state, []) for state, _, _ in STATES},
        "action": {"ready_to_enqueue": triage.get("ready_to_enqueue", []), "out_of_date": out_of_date,
                   "changes_requested": triage.get("changes_requested", []),
                   "main": main_summary, "stale": stale},
    }
    if raw.get("runner_metrics"):
        metrics["runners"] = runner_metrics(raw, since, now)
    return metrics


def concurrency(raw: dict, since: datetime, now: datetime) -> dict:
    """Peak concurrent jobs per UTC hour, by runner label, all workflows.

    A sweep over job start (+1) and completion (-1); a completion sorts
    before a start at the same instant. Jobs under MIN_JOB_SECONDS are left
    out (skipped jobs get a runner for a moment), and a job still running
    ends at the window end. Nothing here assumes a limit: the number is what
    ran at once.
    """
    sweeps: dict[str, list] = defaultdict(list)
    for run in raw["runs"]:
        for job in run.get("jobs") or []:
            if not job.get("started_at"):
                continue
            start = ts(job["started_at"])
            end = ts(job["completed_at"]) if job.get("completed_at") else now
            if (end - start).total_seconds() < MIN_JOB_SECONDS or end <= since or start >= now:
                continue
            label = ",".join(job.get("labels") or []) or "unknown"
            # macOS runners have their own, smaller concurrency limit.
            pools = (label, "all", "all but macOS") if not label.startswith("macos") else (label, "all")
            for label in pools:
                sweeps[label] += [(max(start, since), 1), (min(end, now), -1)]
    out = {}
    for label, points in sorted(sweeps.items()):
        current, per_hour, last = 0, {}, None
        for when, delta in sorted(points):
            # Hours the previous level ran through with no event of their
            # own: a job running across an hour counts in that hour.
            if last is not None and current:
                hour_start = last.replace(minute=0, second=0, microsecond=0) + timedelta(hours=1)
                while hour_start < when:
                    key = hour_start.strftime("%Y-%m-%d %H")
                    per_hour[key] = max(per_hour.get(key, 0), current)
                    hour_start += timedelta(hours=1)
            current += delta
            last = when
            if delta > 0:
                hour = when.strftime("%Y-%m-%d %H")
                per_hour[hour] = max(per_hour.get(hour, 0), current)
        peaks = list(per_hour.values())
        out[label] = {"peak": max(peaks, default=0), "hourly_peak_p50": pct(peaks, 50),
                      "per_hour": dict(sorted(per_hour.items()))}
    return out


def runner_metrics(raw: dict, since: datetime, now: datetime) -> dict:
    """Job queue and run time by event; runner minutes per merged PR."""
    window_h = (now - since).total_seconds() / 3600
    def ran(job: dict) -> bool:
        return bool(job.get("started_at") and job.get("completed_at")) and job.get("conclusion") != "skipped"

    def minutes(job: dict) -> float:
        return (ts(job["completed_at"]) - ts(job["started_at"])).total_seconds() / 60

    by_event = {}
    for event in ("pull_request", "merge_group", "push"):
        waits, runs, starts = [], [], []
        for run in raw["runs"]:
            if run["event"] != event or not since <= ts(run["created_at"]) < now:
                continue
            jobs = [j for j in run.get("jobs") or [] if ran(j)]
            for job in jobs:
                waits.append((ts(job["started_at"]) - ts(job["created_at"])).total_seconds() / 60)
                runs.append(minutes(job))
            if jobs:
                # Run created -> its first job on a runner: the wait a job's
                # own created_at cannot show when the run itself sat queued.
                first = min(ts(j["started_at"]) for j in jobs)
                starts.append((first - ts(run["created_at"])).total_seconds() / 60)
        # Runs still waiting when the report ran: their wait is not over, so
        # the percentiles above cannot include it (right-censored).
        # Only meaningful when the window ends at collection time: a run's
        # status is the status when collected.
        collected = ts(raw.get("collected_at") or raw["now"])
        current = abs((collected - now).total_seconds()) < 600
        waiting = [(collected - ts(r["created_at"])).total_seconds() / 60 for r in raw["runs"]
                   if r["event"] == event and r.get("status") in ("queued", "waiting", "pending")] if current else []
        by_event[event] = {"queued_now": len(waiting) if current else None, "oldest_queued_min": max(waiting, default=None),
                           "jobs": len(waits), "jobs_per_hour": len(waits) / window_h if window_h else None,
                           "queue_min": {"p50": pct(waits, 50), "p95": pct(waits, 95),
                                         "max": max(waits, default=None)},
                           "run_min": {"p50": pct(runs, 50), "p95": pct(runs, 95)},
                           "runs": len(starts),
                           "run_start_min": {"p50": pct(starts, 50), "p95": pct(starts, 95),
                                             "max": max(starts, default=None)}}

    # Attribution: a pull_request run belongs to the PRs it lists, or (the
    # API drops that list once a branch is gone) to the merged PR whose head
    # branch it ran on while that PR was open; a merge_group run belongs to
    # the PR its queue branch names (one run per queue entry, so a group size
    # of one: see the baseline's method note).
    branches = defaultdict(list)
    for pr in raw["merged"]:
        branches[pr.get("headRefName")].append(pr)
    pr_runs, group_runs = defaultdict(list), defaultdict(list)
    for run in raw["runs"]:
        if not is_fast_gate(run):
            continue
        if run["event"] == "pull_request":
            owners = set(run["pull_requests"])
            if not owners:
                owners = {p["number"] for p in branches.get(run.get("head_branch"), [])
                          if p["createdAt"] <= run["created_at"] <= p["mergedAt"]}
            for n in owners:
                pr_runs[n].append(run)
        elif run["event"] == "merge_group":
            m = QUEUE_BRANCH.match(run.get("head_branch") or "")
            if m:
                group_runs[int(m.group(1))].append(run)

    per_pr, regenerations, duplicate = [], [], Counter()
    cancelled_groups = sum(1 for r in raw["runs"] if r["event"] == "merge_group" and is_fast_gate(r)
                           and r.get("conclusion") == "cancelled" and since <= ts(r["created_at"]) < now)
    for pr in sorted(raw["merged"], key=lambda p: p["number"]):
        n = pr["number"]
        prs = [r for r in pr_runs.get(n, []) if r.get("jobs") is not None]
        groups = [r for r in group_runs.get(n, []) if r.get("jobs") is not None]
        total = sum(minutes(j) for r in prs + groups for j in r["jobs"] if ran(j))
        per_pr.append({"number": n, "runner_min": total, "pr_runs": len(prs), "merge_group_runs": len(groups)})
        regenerations.append(max(0, len(group_runs.get(n, [])) - 1))
        last_pr = max((r for r in prs if r["head_sha"] == pr["headRefOid"]), key=lambda r: r["id"], default=None)
        last_group = max(groups, key=lambda r: r["id"], default=None)
        if last_pr and last_group:
            pr_jobs = {j["name"] for j in last_pr["jobs"] if ran(j)}
            for job in last_group["jobs"]:
                if ran(job) and job["name"] in pr_jobs:
                    duplicate[job["name"]] += minutes(job)
    mins = [p["runner_min"] for p in per_pr]
    return {
        "by_event": by_event,
        "concurrency": concurrency(raw, since, now),
        "runner_min_per_merged_pr": {"p50": pct(mins, 50), "p95": pct(mins, 95),
                                     "mean": sum(mins) / len(mins) if mins else None},
        "per_pr": per_pr,
        "merge_group_regenerations": sum(regenerations),
        "merge_group_cancelled": cancelled_groups,
        "duplicate_min_total": sum(duplicate.values()),
        "duplicate_min_by_job": dict(sorted(duplicate.items(), key=lambda kv: (-kv[1], kv[0]))),
    }


# ---------------------------------------------------------------------------
# Rendering


def fmt(value, unit: str = "", digits: int = 1) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float):
        return f"{value:.{digits}f}{unit}"
    return f"{value}{unit}"


def pr_line(repo: str, item: dict, extra: str = "") -> str:
    return f"- #{item['number']} {item['title']}{extra}"


def render(metrics: dict, repo: str, sections: set[str]) -> str:
    m = metrics
    w = m["window"]
    out = [f"# Development flow, {w['since'][:16]}Z to {w['now'][:16]}Z ({w['days']:.2f} days)", "",
           f"{repo}. Open-PR state as of {w.get('open_state_at', w['now'])[:16]}Z.", ""]

    if "action" in sections:
        a = m["action"]
        out += ["## Action required", ""]
        empty = True
        if a["ready_to_enqueue"]:
            empty = False
            out += ["**Ready to enqueue** (merge conditions met, not queued): "
                    "`gh pr merge N --merge --match-head-commit <reviewed_sha>`", ""]
            out += [pr_line(repo, i) for i in a["ready_to_enqueue"]] + [""]
        if a["main"] not in (None, "success"):
            empty = False
            out += [f"**main is not green**: latest required CI Summary on main is `{a['main']}`.", ""]
        if a["out_of_date"]:
            empty = False
            out += [f"**Conflicting or more than {BEHIND_LIMIT} commits behind main** (author):", ""]
            out += [pr_line(repo, i, " (conflicting)" if i["conflicting"] else f" ({i['behind']} behind)")
                    for i in a["out_of_date"]] + [""]
        if a["changes_requested"]:
            empty = False
            out += ["**Failed required check, changes requested or unresolved threads** (author):", ""]
            out += [pr_line(repo, i) for i in a["changes_requested"]] + [""]
        if a["stale"]:
            empty = False
            out += [f"**Open more than {STALE_DAYS} days** (land, split or close):", ""]
            out += [pr_line(repo, i, f" ({i['age_d']:.0f} d)") for i in a["stale"]] + [""]
        if empty:
            out += ["Nothing.", ""]

        out += ["## Who acts next", "", "| State | Next actor | PRs | Oldest in state |", "|---|---|---|---|"]
        for key, label, actor in STATES:
            items = m["triage"][key]
            ages = [i["age_in_state_h"] for i in items if i["age_in_state_h"] is not None]
            out.append(f"| {label} | {actor} | {len(items)} | {fmt(max(ages) if ages else None, ' h')} |")
        out.append("")
        for key, label, _ in STATES:
            items = m["triage"][key]
            if items:
                out += [f"<details><summary>{label} ({len(items)})</summary>", ""]
                out += [pr_line(repo, i, f" ({fmt(i['age_in_state_h'], ' h')} in state)") for i in items]
                out += ["", "</details>", ""]

    if "flow" in sections:
        lt = m["lead_time_h"]
        out += ["## Flow", "",
                "| Lead time (h) | p50 | p95 | max | n |", "|---|---|---|---|---|"]
        for key, label in (("push_to_checks", "last push -> required checks done"),
                           ("checks_to_queue", "checks done -> added to merge queue"),
                           ("queue_to_merged", "queue -> merged"), ("total", "last push -> merged")):
            out.append(f"| {label} | {fmt(lt[key]['p50'])} | {fmt(lt[key]['p95'])} | {fmt(lt[key].get('max'))} "
                       f"| {lt[key]['n']} |")
        ci = m["ci"]
        out += ["",
                "Approval -> enqueue cannot be measured: no approvals are recorded (no review is required), "
                "so \"checks done -> added to merge queue\" is the proxy, and an auto-merge enabled before the "
                "checks finish counts as 0.",
                f"- Merged: {m['merged']} ({fmt(m['merged_per_day'])}/day, {fmt(m['merged_per_hour'], '', 2)}/hour, "
                f"peak {m['merged_peak_hour']} in one hour). Not through the queue: {len(m['merged_without_queue'])}"
                + (f" ({', '.join('#' + str(n) for n in m['merged_without_queue'])})"
                   if 0 < len(m["merged_without_queue"]) <= 10 else "")
                + f"; queued without required checks on head: {len(m['merged_without_required_checks'])}.",
                f"- Merge queue ejections: {m['queue']['ejections']} {m['queue']['by_reason'] or ''}; "
                f"re-queued after an ejection: {m['queue']['requeues_after_ejection']}.",
                f"- Required CI failure rate: pull_request {fmt(_rate(ci['pull_request']))} "
                f"({ci['pull_request']['failed']}/{ci['pull_request']['runs']}), merge_group "
                f"{fmt(_rate(ci['merge_group']))} ({ci['merge_group']['failed']}/{ci['merge_group']['runs']}); "
                f"cancelled runs not counted ({ci['pull_request']['cancelled']} + {ci['merge_group']['cancelled']}).",
                f"- Issues: {m['issues']['opened']} opened, {m['issues']['closed']} closed; "
                f"{m['issues']['open_unlabeled']} open without a label.",
                f"- Open PRs: {m['open_prs']['count']} ({m['open_prs']['drafts']} draft), age p50 "
                f"{fmt(m['open_prs']['age_p50_d'], ' d')}, oldest {fmt(m['open_prs']['age_max_d'], ' d')}.",
                f"- Intervention proxy: {m['intervention']['merge_main_commits']} merge-main commits, "
                f"{m['intervention']['force_pushes']} force pushes, {m['intervention']['requeues_after_ejection']} "
                f"re-queues, {m['intervention']['workflow_reruns']} workflow re-runs; queue adds by "
                f"{m['intervention']['queue_adds_by_actor'] or '{}'}.", ""]

    if "runners" in sections and "runners" in m:
        r = m["runners"]
        out += ["## Runners (all workflows)", "",
                "Runner minutes, regenerations and duplicate cost below are Fast Gate (ci.yml) only.", "",
                "| Event | Runs | Run start p50 / p95 / max (min) | Jobs | Jobs/hour | Job queue p50 / p95 / max "
                "(min) | Job run p50 / p95 (min) |", "|---|---|---|---|---|---|---|"]
        for event, v in r["by_event"].items():
            rs, q = v["run_start_min"], v["queue_min"]
            out.append(f"| {event} | {v['runs']} | {fmt(rs['p50'])} / {fmt(rs['p95'])} / {fmt(rs['max'])} | "
                       f"{v['jobs']} | {fmt(v.get('jobs_per_hour'))} | {fmt(q['p50'])} / {fmt(q['p95'])} / "
                       f"{fmt(q.get('max'))} | {fmt(v['run_min']['p50'])} / {fmt(v['run_min']['p95'])} |")
        conc = r.get("concurrency", {})
        if conc:
            out += ["", f"Peak concurrent jobs (jobs of at least {MIN_JOB_SECONDS} s; the plan's limit is not "
                    "read from the API, which needs admin:org):", "",
                    "| Runner label | Peak | Median hourly peak | Hours |", "|---|---|---|---|"]
            for label, v in conc.items():
                out.append(f"| {label} | {v['peak']} | {fmt(v['hourly_peak_p50'])} | {len(v['per_hour'])} |")
            hourly = conc.get("all", {}).get("per_hour", {})
            if hourly:
                out += ["", "<details><summary>Peak concurrent jobs per hour, all labels (UTC)</summary>", "",
                        "| Hour | Peak |", "|---|---|"]
                out += [f"| {h}:00 | {v} |" for h, v in hourly.items()]
                out += ["", "</details>"]
        if all(v["queued_now"] is not None for v in r["by_event"].values()):
            out += ["", "Runs still queued when the report ran (not in the percentiles above): "
                    + ", ".join(f"{e} {v['queued_now']} (oldest {fmt(v['oldest_queued_min'], ' min')})"
                                for e, v in r["by_event"].items()) + "."]
        top = list(r["duplicate_min_by_job"].items())[:5]
        out += ["",
                f"- Runner minutes per merged PR: p50 {fmt(r['runner_min_per_merged_pr']['p50'])}, "
                f"p95 {fmt(r['runner_min_per_merged_pr']['p95'])}, mean {fmt(r['runner_min_per_merged_pr']['mean'])}.",
                f"- Merge-group regenerations (runs beyond the first per merged PR): {r['merge_group_regenerations']}; "
                f"cancelled merge_group runs: {r['merge_group_cancelled']}.",
                f"- PR-vs-merge-group duplicate cost: {fmt(r['duplicate_min_total'])} runner-min; top jobs: "
                + (", ".join(f"{k} {v:.0f}" for k, v in top) or "none") + ".", ""]
    return "\n".join(out)


def _rate(ci: dict) -> float | None:
    return None if ci["failure_rate"] is None else ci["failure_rate"] * 100


COMPARED = (
    ("lead_time_h.push_to_checks.p50", "push -> checks p50 (h)"),
    ("lead_time_h.push_to_checks.p95", "push -> checks p95 (h)"),
    ("lead_time_h.checks_to_queue.p50", "checks -> queue p50 (h)"),
    ("lead_time_h.checks_to_queue.p95", "checks -> queue p95 (h)"),
    ("lead_time_h.queue_to_merged.p50", "queue -> merged p50 (h)"),
    ("lead_time_h.queue_to_merged.p95", "queue -> merged p95 (h)"),
    ("lead_time_h.queue_to_merged.max", "queue -> merged max (h)"),
    ("lead_time_h.queue_to_merged.n", "queued merges (n)"),
    ("merged_per_day", "merged / day"),
    ("merged_peak_hour", "merged in peak hour"),
    ("ci.pull_request.runs", "pull_request runs decided (n)"),
    ("ci.pull_request.failure_rate", "pull_request failure rate"),
    ("ci.merge_group.runs", "merge_group runs decided (n)"),
    ("ci.merge_group.failure_rate", "merge_group failure rate"),
    ("queue.ejections", "queue ejections"),
    ("open_prs.count", "open PRs"),
    ("issues.open_unlabeled", "open unlabeled issues"),
    ("intervention.merge_main_commits", "merge-main commits"),
    ("intervention.force_pushes", "force pushes"),
    ("intervention.requeues_after_ejection", "re-queues after ejection"),
    ("intervention.workflow_reruns", "workflow re-runs"),
    ("runners.by_event.pull_request.jobs_per_hour", "PR jobs / hour (workload)"),
    ("runners.by_event.pull_request.queue_min.p95", "PR job queue p95 (min)"),
    ("runners.by_event.pull_request.queue_min.max", "PR job queue max (min)"),
    ("runners.by_event.merge_group.jobs_per_hour", "merge_group jobs / hour (workload)"),
    ("runners.by_event.merge_group.queue_min.p95", "merge_group job queue p95 (min)"),
    ("runners.by_event.merge_group.queue_min.max", "merge_group job queue max (min)"),
    ("runners.concurrency.all.peak", "peak concurrent jobs"),
    ("runners.concurrency.all.hourly_peak_p50", "median hourly peak concurrent jobs"),
    ("runners.by_event.merge_group.run_start_min.p95", "merge_group run start p95 (min)"),
    ("runners.by_event.merge_group.queued_now", "merge_group runs queued at report time"),
    ("runners.runner_min_per_merged_pr.p50", "runner min / merged PR p50"),
    ("runners.merge_group_regenerations", "merge-group regenerations"),
    ("runners.duplicate_min_total", "PR-vs-group duplicate min"),
)


def lookup(metrics: dict, path: str):
    value = metrics
    for part in path.split("."):
        if not isinstance(value, dict) or part not in value:
            return None
        value = value[part]
    return value


def compare(metrics: dict, baseline: dict) -> str:
    out = ["## Against the baseline", "",
           f"Baseline window: {baseline['window']['since'][:16]}Z to {baseline['window']['now'][:16]}Z; now: "
           f"{metrics['window']['since'][:16]}Z to {metrics['window']['now'][:16]}Z. Workloads differ: read the "
           "n and jobs/hour rows next to each time.", "",
           "| Metric | Baseline | Now | Delta |", "|---|---|---|---|"]
    for path, label in COMPARED:
        before, after = lookup(baseline, path), lookup(metrics, path)
        delta = after - before if isinstance(before, (int, float)) and isinstance(after, (int, float)) else None
        digits = 3 if "rate" in path else 1
        out.append(f"| {label} | {fmt(before, digits=digits)} | {fmt(after, digits=digits)} | "
                   f"{'n/a' if delta is None else f'{delta:+.{digits}f}'} |")
    return "\n".join(out) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--days", type=int, default=7)
    parser.add_argument("--runner-metrics", action="store_true", help="add job-level runner metrics (one call per run)")
    parser.add_argument("--since", help="window start, ISO time (overrides --days)")
    parser.add_argument("--until", "--now", dest="until", help="window end, ISO time (default: now)")
    parser.add_argument("--raw-in", type=Path, help="replay a saved collection instead of calling GitHub")
    parser.add_argument("--raw-out", type=Path, help="save the collection")
    parser.add_argument("--json-out", type=Path, help="save the computed metrics (a --compare baseline)")
    parser.add_argument("--compare", type=Path, help="metrics JSON from an earlier run; print deltas")
    args = parser.parse_args(argv)

    if args.raw_in:
        raw = json.loads(args.raw_in.read_text())
    else:
        until = ts(args.until) if args.until else datetime.now(timezone.utc).replace(microsecond=0)
        since = ts(args.since) if args.since else until - timedelta(days=args.days)
        if since >= until:
            parser.error("--since must be before --until")
        raw = collect(GitHub(args.repo), since, until, args.runner_metrics)
    if args.raw_out:
        args.raw_out.write_text(json.dumps(raw, indent=1, sort_keys=True) + "\n")
    metrics = analyze(raw)
    if args.json_out:
        args.json_out.write_text(json.dumps(metrics, indent=1, sort_keys=True) + "\n")
    print(render(metrics, raw["repo"], {"action", "flow", "runners"}))
    if args.compare:
        print(compare(metrics, json.loads(args.compare.read_text())))
    if "api_calls" in raw:
        print(f"_{raw['api_calls']} GitHub API calls._", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
