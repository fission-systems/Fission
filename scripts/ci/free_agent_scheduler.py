#!/usr/bin/env python3
"""Scheduler and duplicate-PR guard of the free-model draft flow.

    free_agent_scheduler.py tick [--dry-run] [--repo R] [--policy FILE]
    free_agent_scheduler.py guard --issue N [--github-output FILE] [--repo R]

State lives in GitHub only (issues, labels, workflow runs, PRs, artifacts). A workflow
`concurrency:` group keeps one pending run and cancels the rest, so it cannot queue tasks;
this scheduler (cron, every 30 minutes) starts at most ONE `free-model-draft.yml` run per
tick instead. Policy is `.github/free-agent-policy.json`:

    max_daily_tasks         draft runs started per UTC day (all triggers count): the hard cap
    max_concurrent          draft runs queued or in progress at once
    max_pending_draft_prs   open `agent:free-draft` PRs: reviews are the bottleneck

A tick, in order:
  1. removes `agent:deferred` from issues that have carried it for 6 hours or more (the
     time of the `labeled` event in the issue timeline);
  2. exits if the concurrent, daily or pending-PR limit is reached;
  3. starts the oldest open `agent:free` issue with no open draft PR and no `agent:blocked`
     or `agent:deferred` label.

`guard` is the duplicate-PR check of free-model-draft.yml: an open PR labelled
`agent:free-draft` whose body says `Fixes #N`, or one from a branch `agent-free/issue-N-*`,
means a draft already exists; it comments once on the issue and reports exists=true.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

DEFAULT_REPO = "fission-systems/Fission"
WORKFLOW = "free-model-draft.yml"
FREE, DRAFT, DEFERRED, BLOCKED = "agent:free", "agent:free-draft", "agent:deferred", "agent:blocked"
ACTIVE = {"queued", "in_progress", "waiting", "requested", "pending"}
DEFER_HOURS = 6
POLICY_FILE = Path(__file__).resolve().parents[2] / ".github" / "free-agent-policy.json"
POLICY_KEYS = ("max_daily_tasks", "max_concurrent", "max_pending_draft_prs")


def load_policy(path: Path) -> dict:
    data = json.loads(path.read_text())
    for key in POLICY_KEYS:
        value = data.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            raise SystemExit(f"{path}: `{key}` must be a non-negative integer")
    if data["max_concurrent"] < 1:
        raise SystemExit(f"{path}: `max_concurrent` must be at least 1")
    return {key: data[key] for key in POLICY_KEYS}


def names(item: dict) -> set[str]:
    return {label["name"] for label in item.get("labels", [])}


def parse_time(text: str) -> datetime.datetime:
    return datetime.datetime.fromisoformat(text.replace("Z", "+00:00"))


def find_open_draft(prs: list[dict], issue: int) -> dict | None:
    """An open draft PR for `issue`: from a branch `agent-free/issue-<n>-*`, or labelled
    `agent:free-draft` with `Fixes #<n>` in its body (not `#<n>0`)."""
    fixes = re.compile(rf"\bFixes\s+#{issue}(?!\d)", re.IGNORECASE)
    for pr in prs:
        if str(pr.get("headRefName", "")).startswith(f"agent-free/issue-{issue}-"):
            return pr
        if DRAFT in names(pr) and fixes.search(pr.get("body") or ""):
            return pr
    return None


def labeled_at(timeline: list[dict], label: str) -> datetime.datetime | None:
    """When `label` was last added, from the issue timeline (`labeled` events)."""
    times = [parse_time(e["created_at"]) for e in timeline
             if e.get("event") == "labeled" and (e.get("label") or {}).get("name") == label and e.get("created_at")]
    return max(times) if times else None


def deferred_expired(timeline: list[dict], now: datetime.datetime, hours: int = DEFER_HOURS) -> tuple[bool, str]:
    """Whether `agent:deferred` is old enough to remove, and why. With no `labeled` event to read
    the label is removed too: a label nobody can date would block the issue for good."""
    since = labeled_at(timeline, DEFERRED)
    if since is None:
        return True, "no labeled event found"
    age = now - since
    return age >= datetime.timedelta(hours=hours), f"labeled {int(age.total_seconds() // 60)} min ago"


@dataclass
class Decision:
    action: str  # "start" or "skip"
    reason: str
    issue: int | None = None
    facts: dict = field(default_factory=dict)
    considered: list = field(default_factory=list)  # (issue, why not)


def eligible(issues: list[dict], prs: list[dict]) -> tuple[int | None, list]:
    """The oldest open `agent:free` issue that may be drafted, and why each older one may not."""
    skipped = []
    for item in sorted(issues, key=lambda i: (i.get("createdAt", ""), i["number"])):
        labels = names(item)
        if BLOCKED in labels or DEFERRED in labels:
            skipped.append((item["number"], "labelled " + (BLOCKED if BLOCKED in labels else DEFERRED)))
        elif (pr := find_open_draft(prs, item["number"])) is not None:
            skipped.append((item["number"], f"open draft PR #{pr['number']}"))
        else:
            return item["number"], skipped
    return None, skipped


def decide(policy: dict, runs: list[dict], now: datetime.datetime, prs: list[dict], issues: list[dict]) -> Decision:
    today = now.astimezone(datetime.timezone.utc).strftime("%Y-%m-%d")
    facts = {"active_runs": sum(1 for r in runs if r.get("status") in ACTIVE),
             # Every label added to any issue starts free-model-draft.yml; runs for
             # other labels are skipped at the job level and did no work.
             "runs_today": sum(1 for r in runs if str(r.get("createdAt", "")).startswith(today)
                               and r.get("conclusion") != "skipped"),
             "open_draft_prs": sum(1 for p in prs if DRAFT in names(p)),
             "open_free_issues": len(issues), **policy}
    if facts["active_runs"] >= policy["max_concurrent"]:
        return Decision("skip", f"{facts['active_runs']} draft run(s) queued or in progress "
                                f"(max_concurrent {policy['max_concurrent']})", None, facts)
    if facts["runs_today"] >= policy["max_daily_tasks"]:
        return Decision("skip", f"{facts['runs_today']} draft runs today "
                                f"(max_daily_tasks {policy['max_daily_tasks']})", None, facts)
    if facts["open_draft_prs"] >= policy["max_pending_draft_prs"]:
        return Decision("skip", f"{facts['open_draft_prs']} open {DRAFT} PRs "
                                f"(max_pending_draft_prs {policy['max_pending_draft_prs']}): review is the bottleneck",
                        None, facts)
    number, skipped = eligible(issues, prs)
    if number is None:
        return Decision("skip", f"no eligible {FREE} issue", None, facts, skipped)
    return Decision("start", f"oldest eligible {FREE} issue", number, facts, skipped)


# --- GitHub (all state is read from, and written to, GitHub) -------------------------------

def gh(*args: str, input_text: str | None = None) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True, input=input_text).stdout


def gh_runs(repo: str) -> list[dict]:
    return json.loads(gh("run", "list", "--repo", repo, "--workflow", WORKFLOW, "--limit", "100",
                         "--json", "databaseId,status,conclusion,createdAt,event"))


def gh_open_prs(repo: str) -> list[dict]:
    return json.loads(gh("pr", "list", "--repo", repo, "--state", "open", "--limit", "500",
                         "--json", "number,headRefName,body,labels"))


def gh_free_issues(repo: str) -> list[dict]:
    return json.loads(gh("issue", "list", "--repo", repo, "--label", FREE, "--state", "open", "--limit", "500",
                         "--json", "number,title,createdAt,labels"))


def gh_timeline(repo: str, number: int) -> list[dict]:
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{repo}/issues/{number}/timeline?per_page=100"))
    return [event for page in pages for event in (page if isinstance(page, list) else [page])]


def gh_remove_deferred(repo: str, number: int) -> None:
    gh("issue", "edit", str(number), "--repo", repo, "--remove-label", DEFERRED)


def gh_dispatch(repo: str, number: int) -> None:
    gh("workflow", "run", WORKFLOW, "--repo", repo, "-f", f"issue={number}", "-f", "open_pr=true")


def gh_comments(repo: str, number: int) -> list[str]:
    data = json.loads(gh("issue", "view", str(number), "--repo", repo, "--json", "comments"))
    return [c.get("body") or "" for c in data.get("comments", [])]


def gh_comment(repo: str, number: int, body: str) -> None:
    gh("issue", "comment", str(number), "--repo", repo, "--body-file", "-", input_text=body)


# --- commands ----------------------------------------------------------------------------------

def usage_today(repo: str, now: datetime.datetime) -> str:
    """Today's measured usage from the run metrics artifacts, best effort (units are measured,
    never guessed; `max_daily_tasks` is still the only hard cap)."""
    try:
        import free_agent_report as report
        rows = report.aggregate(report.collect(repo, 1, now))
        total = report.day_totals(rows, now.strftime("%Y-%m-%d"))
    except Exception as err:  # noqa: BLE001 - a missing artifact must never stop a tick
        return f"usage today: not available ({type(err).__name__})"
    if total is None:
        return "usage today: no metrics yet"
    show = report._show
    return (f"usage today (metrics): {len(total['runs'])} runs, {total['invocations']} invocations, "
            f"{total['requests']} LLM requests, tokens in {show(total['tokens_in'])} / out {show(total['tokens_out'])} / "
            f"reasoning {show(total['tokens_reasoning'])} / cache read {show(total['cache_read'])}, "
            f"{total['c429']} x 429")


def tick(repo: str, policy: dict, dry_run: bool, now: datetime.datetime | None = None) -> Decision:
    now = now or datetime.datetime.now(datetime.timezone.utc)
    lines = []
    issues = gh_free_issues(repo)
    for item in issues:
        if DEFERRED not in names(item):
            continue
        expired, why = deferred_expired(gh_timeline(repo, item["number"]), now)
        if expired:
            lines.append(f"#{item['number']}: {'would remove' if dry_run else 'removing'} {DEFERRED} ({why})")
            if not dry_run:
                gh_remove_deferred(repo, item["number"])
            item["labels"] = [x for x in item["labels"] if x["name"] != DEFERRED]
        else:
            lines.append(f"#{item['number']}: keeping {DEFERRED} ({why}, removed after {DEFER_HOURS} h)")
    decision = decide(policy, gh_runs(repo), now, gh_open_prs(repo), issues)
    lines.append("scheduler sees: " + ", ".join(f"{k}={v}" for k, v in decision.facts.items()))
    lines.append(usage_today(repo, now))
    for number, why in decision.considered:
        lines.append(f"#{number}: not started ({why})")
    if decision.action == "start":
        lines.append(f"{'WOULD START' if dry_run else 'starting'} {WORKFLOW} for #{decision.issue}: {decision.reason}")
        if not dry_run:
            gh_dispatch(repo, decision.issue)
    else:
        lines.append(f"nothing started: {decision.reason}")
    text = "\n".join(lines)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write("### Free-agent scheduler" + (" (dry run)" if dry_run else "") + "\n\n```\n" + text + "\n```\n")
    return decision


def guard(repo: str, issue: int, github_output: str | None) -> bool:
    """True when an open draft PR exists for the issue (and says so on the issue, once)."""
    pr = find_open_draft(gh_open_prs(repo), issue)
    if pr is not None:
        marker = f"<!-- free-agent-draft-open:#{pr['number']} -->"
        if not any(marker in body for body in gh_comments(repo, issue)):
            gh_comment(repo, issue, f"A free-model draft is already open for this issue: #{pr['number']}. "
                                    f"No new draft was started.\n\n{marker}\n")
        print(f"issue #{issue} already has an open draft: #{pr['number']}")
    else:
        print(f"issue #{issue} has no open draft")
    if github_output:
        with open(github_output, "a", encoding="utf-8") as fh:
            fh.write(f"exists={'true' if pr is not None else 'false'}\n")
    return pr is not None


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", DEFAULT_REPO))
    sub = ap.add_subparsers(dest="command", required=True)
    t = sub.add_parser("tick")
    t.add_argument("--dry-run", action="store_true", help="read GitHub, change nothing")
    t.add_argument("--policy", type=Path, default=POLICY_FILE)
    g = sub.add_parser("guard")
    g.add_argument("--issue", type=int, required=True)
    g.add_argument("--github-output")
    args = ap.parse_args(argv)
    if args.command == "tick":
        tick(args.repo, load_policy(args.policy), args.dry_run)
    else:
        guard(args.repo, args.issue, args.github_output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
