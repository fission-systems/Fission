#!/usr/bin/env python3
"""File GitHub issues for CI defects that Sentry recorded. Dry-run by default.

Reads unresolved Sentry issues of project `fission` and files a GitHub issue
for each one whose latest event is a CI defect under the Sentry defect-event
contract: environment `ci` or `nightly`, category `confirmed_semantic_defect`
or `crash`, and every contract tag present and valid. Anything else is
ignored. Event content is untrusted (the project's DSN ships in fission_cli),
so the issue is built only from closed-set or pattern-validated tags and
Sentry's own numeric metadata; no event text is ever copied.

Before an issue is drafted, two facts are checked on GitHub, not taken from
the event: the reported Actions run exists in this repository, was triggered
by the event the environment implies, and tested the reported commit; and
that commit is on `main` (docs/ISSUE_PR_WORKFLOW.md: no issue for work in
progress, a probe names a `main` SHA).

Every issue carries a hidden `<!-- sentry-issue: <id> -->` marker. Issues
already carrying the marker, open or closed, are left alone: a closed one is
never reopened or commented on. At most `--run-cap` issues are filed per run
and at most 10 per UTC day, counting marked issues created that day.

Creating issues needs both `--create` and SENTRY_TRIAGE_ENABLED=true in the
environment; without them the script only reads, and writes the would-be
issues to `--summary` (Markdown) and `--out` (JSON). Standard library only.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Optional

ROOT = Path(__file__).resolve().parents[2]

SENTRY_API = "https://sentry.io/api/0"
GITHUB_API = "https://api.github.com"
DEFAULT_ORG = "fission-systems"
DEFAULT_PROJECT = "fission"
DEFAULT_REPO = "fission-systems/Fission"
BASE_BRANCH = "main"
ISSUE_AUTHOR = "github-actions[bot]"
DAILY_CAP_MAX = 10

# --- The contract (closed sets and patterns) --------------------------------

KINDS = frozenset(
    {"verify_diverged", "native_differential_mismatch", "fuzz_crash", "flaky_test", "test_failure"}
)
ARCHES = frozenset({"x86", "x86_64", "aarch64", "arm", "thumb", "riscv64", "mips", "ppc", "unknown"})
STAGES = frozenset(
    {
        "lift", "sleigh", "emulator", "normalize", "structuring", "render",
        "verify", "loader", "solver", "dir", "cli", "test", "unknown",
    }
)
CATEGORIES = frozenset({"confirmed_semantic_defect", "crash", "flaky", "infra"})
ENVIRONMENTS = frozenset({"ci", "nightly"})
COMPONENT_RE = re.compile(r"\A(?:fission-[a-z0-9-]+|xtask)\Z")
INVARIANT_RE = re.compile(r"\A[a-z0-9_.:-]{1,64}\Z")
COMMIT_RE = re.compile(r"\A[0-9a-f]{40}\Z")
REPRO_RE = re.compile(r"\A[a-z0-9_.:/-]{1,128}\Z")
RUN_URL_RE = re.compile(r"\Ahttps://github\.com/([A-Za-z0-9-]+/[A-Za-z0-9._-]+)/actions/runs/([0-9]{1,20})\Z")

# Triage policy on top of the contract.
ELIGIBLE_CATEGORIES = frozenset({"confirmed_semantic_defect", "crash"})
# A flaky test is not a defect (docs/ISSUE_PR_WORKFLOW.md), whatever its category.
INELIGIBLE_KINDS = frozenset({"flaky_test"})
# Which Actions trigger each environment stands for (the contract's definition).
RUN_EVENTS = {"ci": frozenset({"push", "merge_group", "workflow_dispatch"}), "nightly": frozenset({"schedule"})}

TAG_CHECKS: dict[str, Callable[[str], bool]] = {
    # Any well-formed environment passes validation; eligibility narrows it.
    "environment": lambda v: bool(re.match(r"\A[a-z0-9_-]{1,32}\Z", v)),
    "kind": lambda v: v in KINDS,
    "component": lambda v: bool(COMPONENT_RE.match(v)),
    "arch": lambda v: v in ARCHES,
    "stage": lambda v: v in STAGES,
    "invariant": lambda v: bool(INVARIANT_RE.match(v)),
    "category": lambda v: v in CATEGORIES,
    "commit": lambda v: bool(COMMIT_RE.match(v)),
    "repro_id": lambda v: bool(REPRO_RE.match(v)),
}
# Optional in the contract, required here: it is what lets triage check the
# event against GitHub instead of trusting it.
RUN_URL_TAG = "run_url"

SENTRY_ID_RE = re.compile(r"\A[0-9]{1,20}\Z")
SHORT_ID_RE = re.compile(r"\A[A-Z][A-Z0-9]{0,31}-[A-Z0-9]{1,12}\Z")
EVENT_ID_RE = re.compile(r"\A[0-9a-f]{32}\Z")
SEEN_RE = re.compile(r"\A(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.\d{1,9})?(?:Z|\+00:00)\Z")
SLUG_RE = re.compile(r"\A[a-z0-9][a-z0-9_-]{0,49}\Z")
STATS_PERIOD_RE = re.compile(r"\A[0-9]{1,3}[hd]\Z")
MARKER_RE = re.compile(r"<!-- sentry-issue: ([0-9]{1,20}) -->")
TITLE_TAG_RE = re.compile(r"\A\[sentry ([A-Z][A-Z0-9]{0,31}-[A-Z0-9]{1,12})\] ")

# --- Labels ------------------------------------------------------------------

# Rule (docs/ISSUE_PR_WORKFLOW.md, "Sentry triage"): every filed issue is a
# defect candidate, so `bug`; it has a probe reference but no recorded probe
# output or acceptance check, so `status:needs-repro`, never `confirmed`; both
# eligible categories are correctness or reliability defects, which is P1's
# definition, and automation never assigns P0 (a maintainer escalates).
FIXED_LABELS = ("bug", "status:needs-repro", "priority:P1")


def managed_labels(root: Path = ROOT) -> set[str]:
    return {item["name"] for item in json.loads((root / ".github/labels.json").read_text())}


def labeler_rules(text: str) -> dict[str, list[str]]:
    """label -> globs from .github/labeler.yml (the changed-files form used there)."""
    rules: dict[str, list[str]] = {}
    label = None
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line.startswith(" "):
            label = line.rstrip(":").strip()
            rules[label] = []
        elif label and line.strip().startswith("- ") and not line.strip().endswith(":"):
            rules[label].append(line.strip()[2:].strip().strip('"'))
    return rules


def _glob_regex(glob: str) -> re.Pattern:
    out, i = "", 0
    while i < len(glob):
        if glob.startswith("**", i):
            out, i = out + ".*", i + 2
        elif glob[i] == "*":
            out, i = out + "[^/]*", i + 1
        else:
            out, i = out + re.escape(glob[i]), i + 1
    return re.compile(out + r"\Z")


def area_labels(component: str, rules: dict[str, list[str]]) -> list[str]:
    """area:* labels for a crate, by the same path globs that label PRs."""
    probe = f"{component}/Cargo.toml" if component == "xtask" else f"crates/{component}/Cargo.toml"
    return sorted(
        label
        for label, globs in rules.items()
        if label.startswith("area:") and any(_glob_regex(g).match(probe) for g in globs)
    )


# --- HTTP --------------------------------------------------------------------

Transport = Callable[[str, str, dict, Optional[bytes]], tuple]


def urllib_transport(method: str, url: str, headers: dict, body: Optional[bytes]) -> tuple:
    request = urllib.request.Request(url, data=body, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:  # fixed https bases only
            return response.status, dict(response.headers.items()), response.read()
    except urllib.error.HTTPError as error:
        return error.code, dict((error.headers or {}).items()), error.read()


class ApiError(RuntimeError):
    pass


class Api:
    def __init__(self, base: str, headers: dict, transport: Transport, allow_write: bool):
        self.base = base
        self.headers = headers
        self.transport = transport
        self.allow_write = allow_write

    def call(self, method: str, path_or_url: str, params=None, body=None, ok=(200,)):
        if method != "GET" and not self.allow_write:
            raise ApiError(f"refusing {method} in read-only mode")
        url = path_or_url if path_or_url.startswith(self.base + "/") else self.base + path_or_url
        if not url.startswith(self.base + "/"):
            raise ApiError("refusing a URL outside the API base")
        if params:
            url += ("&" if "?" in url else "?") + urllib.parse.urlencode(params, doseq=True)
        data = json.dumps(body).encode() if body is not None else None
        headers = dict(self.headers)
        if data is not None:
            headers["Content-Type"] = "application/json"
        status, response_headers, raw = self.transport(method, url, headers, data)
        if status not in ok:
            raise ApiError(f"{method} {url.split('?')[0]} -> HTTP {status}")
        payload = json.loads(raw) if raw else None
        return status, response_headers, payload

    def next_page(self, headers: dict) -> Optional[str]:
        """Follow rel=next only within this API (the token must not leave it)."""
        link = next((v for k, v in headers.items() if k.lower() == "link"), "")
        for part in link.split(","):
            if 'rel="next"' not in part or 'results="false"' in part:
                continue
            match = re.search(r"<([^>]+)>", part)
            if match and match.group(1).startswith(self.base + "/"):
                return match.group(1)
        return None


class Sentry:
    def __init__(self, api: Api, org: str, project: str):
        self.api, self.org, self.project = api, org, project

    def candidate_issues(self, stats_period: str, max_pages: int = 5) -> list[dict]:
        _, _, project = self.api.call("GET", f"/projects/{self.org}/{self.project}/")
        project_id = str(project.get("id", "")) if isinstance(project, dict) else ""
        if not SENTRY_ID_RE.match(project_id):
            raise ApiError("Sentry project id is not numeric")
        # The query only narrows the listing; every event is validated again.
        # `environment:` goes in the query: the `environment` parameter 404s
        # until an environment has received its first event.
        params = {
            "project": project_id,
            "statsPeriod": stats_period,
            "limit": 100,
            "query": "is:unresolved environment:[ci,nightly] category:[confirmed_semantic_defect,crash]",
        }
        issues: list[dict] = []
        url: Optional[str] = f"/organizations/{self.org}/issues/"
        for _ in range(max_pages):
            _, headers, page = self.api.call("GET", url, params=params)
            issues.extend(item for item in (page or []) if isinstance(item, dict))
            url, params = self.api.next_page(headers), None
            if not url:
                break
        return issues

    def latest_event(self, issue_id: str) -> dict:
        _, _, event = self.api.call("GET", f"/organizations/{self.org}/issues/{issue_id}/events/latest/")
        return event if isinstance(event, dict) else {}

    def suspect_commit(self, event_id: str) -> Optional[str]:
        """The first suspect commit SHA, if Sentry has one. Only the SHA is used."""
        try:
            _, _, data = self.api.call(
                "GET", f"/projects/{self.org}/{self.project}/events/{event_id}/committers/", ok=(200, 404)
            )
        except ApiError:
            return None
        for committer in (data or {}).get("committers", []) if isinstance(data, dict) else []:
            for commit in committer.get("commits", []) if isinstance(committer, dict) else []:
                sha = commit.get("id") if isinstance(commit, dict) else None
                if isinstance(sha, str) and COMMIT_RE.match(sha):
                    return sha
        return None


class GitHub:
    def __init__(self, api: Api, repo: str, author: str = ISSUE_AUTHOR):
        self.api, self.repo, self.author = api, repo, author

    def tracked(self, max_pages: int = 20) -> dict[str, dict]:
        """Sentry id or short id -> issue, for every issue the bot filed, open or closed."""
        found: dict[str, dict] = {}
        url: Optional[str] = f"/repos/{self.repo}/issues"
        params = {"state": "all", "creator": self.author, "per_page": 100, "sort": "created", "direction": "desc"}
        for _ in range(max_pages):
            _, headers, page = self.api.call("GET", url, params=params)
            for issue in page or []:
                if not isinstance(issue, dict) or "pull_request" in issue:
                    continue
                entry = {
                    "number": issue.get("number"),
                    "state": issue.get("state"),
                    "created_at": issue.get("created_at") or "",
                }
                marker = MARKER_RE.search(issue.get("body") or "")
                if marker:
                    found[marker.group(1)] = entry
                title = TITLE_TAG_RE.match(issue.get("title") or "")
                if title:
                    found.setdefault(title.group(1), entry)
            url, params = self.api.next_page(headers), None
            if not url:
                break
        return found

    def labels(self) -> set[str]:
        names: set[str] = set()
        url: Optional[str] = f"/repos/{self.repo}/labels"
        params = {"per_page": 100}
        for _ in range(10):
            _, headers, page = self.api.call("GET", url, params=params)
            names.update(item["name"] for item in page or [] if isinstance(item, dict) and "name" in item)
            url, params = self.api.next_page(headers), None
            if not url:
                break
        return names

    def run(self, run_id: str) -> Optional[dict]:
        _, _, run = self.api.call("GET", f"/repos/{self.repo}/actions/runs/{run_id}", ok=(200, 404))
        return run if isinstance(run, dict) and run.get("id") is not None else None

    def on_base_branch(self, sha: str) -> bool:
        status, _, data = self.api.call(
            "GET", f"/repos/{self.repo}/compare/{BASE_BRANCH}...{sha}", ok=(200, 404, 422)
        )
        return status == 200 and isinstance(data, dict) and data.get("status") in ("identical", "behind")

    def create(self, title: str, body: str, labels: list[str]) -> int:
        _, _, issue = self.api.call(
            "POST", f"/repos/{self.repo}/issues", body={"title": title, "body": body, "labels": labels}, ok=(201,)
        )
        return int(issue["number"])


# --- Validation --------------------------------------------------------------


@dataclass(frozen=True)
class Defect:
    sentry_id: str
    short_id: str
    permalink: str
    events: int
    first_seen: str
    last_seen: str
    environment: str
    kind: str
    component: str
    arch: str
    stage: str
    invariant: str
    category: str
    commit: str
    repro_id: str
    run_url: str
    run_id: str
    suspect_commit: Optional[str] = None


class Rejected(Exception):
    """An issue or event that is not filed; the message is a fixed reason code."""


def _seen(value) -> str:
    match = SEEN_RE.match(value) if isinstance(value, str) else None
    if not match:
        raise Rejected("invalid_timestamp")
    return f"{match.group(1)}T{match.group(2)}Z"


def validate_issue(issue: dict, org: str, project: str) -> dict:
    """Sentry's own metadata for the issue; the permalink is rebuilt, not copied."""
    sentry_id = str(issue.get("id", ""))
    short_id = issue.get("shortId")
    if not SENTRY_ID_RE.match(sentry_id):
        raise Rejected("invalid_issue_id")
    if not isinstance(short_id, str) or not SHORT_ID_RE.match(short_id):
        raise Rejected("invalid_short_id")
    if (issue.get("project") or {}).get("slug") != project:
        raise Rejected("wrong_project")
    try:
        events = int(str(issue.get("count", "")))
    except ValueError:
        raise Rejected("invalid_count") from None
    if events < 0:
        raise Rejected("invalid_count")
    return {
        "sentry_id": sentry_id,
        "short_id": short_id,
        "permalink": f"https://{org}.sentry.io/issues/{sentry_id}/",
        "events": events,
        "first_seen": _seen(issue.get("firstSeen")),
        "last_seen": _seen(issue.get("lastSeen")),
    }


def validate_tags(event: dict, sentry_id: str, repo: str) -> dict:
    """Contract tags of the event, each validated; raises Rejected otherwise."""
    if str(event.get("groupID", "")) != sentry_id:
        raise Rejected("event_not_in_issue")
    raw = event.get("tags")
    if not isinstance(raw, list):
        raise Rejected("no_tags")
    tags: dict[str, str] = {}
    for item in raw:
        if not isinstance(item, dict) or not isinstance(item.get("key"), str):
            raise Rejected("malformed_tag")
        key, value = item["key"], item.get("value")
        if key not in TAG_CHECKS and key != RUN_URL_TAG:
            continue  # SDK tags (os, level, release...) are neither required nor copied.
        if key in tags:
            raise Rejected(f"duplicate_tag:{key}")
        if not isinstance(value, str):
            raise Rejected(f"invalid_tag:{key}")
        tags[key] = value
    for key, check in TAG_CHECKS.items():
        if key not in tags:
            raise Rejected(f"missing_tag:{key}")
        if not check(tags[key]):
            raise Rejected(f"invalid_tag:{key}")
    run = RUN_URL_RE.match(tags.get(RUN_URL_TAG, ""))
    if not run:
        raise Rejected(f"missing_tag:{RUN_URL_TAG}" if RUN_URL_TAG not in tags else f"invalid_tag:{RUN_URL_TAG}")
    if run.group(1) != repo:
        raise Rejected("run_in_other_repo")
    tags["run_id"] = run.group(2)
    return tags


def check_eligible(tags: dict) -> None:
    if tags["environment"] not in ENVIRONMENTS:
        raise Rejected("ineligible_environment")
    if tags["category"] not in ELIGIBLE_CATEGORIES:
        raise Rejected("ineligible_category")
    if tags["kind"] in INELIGIBLE_KINDS:
        raise Rejected("ineligible_kind")


def check_run(run: Optional[dict], tags: dict, repo: str) -> None:
    """The event's run must be a real run of this repo that tested its commit."""
    if run is None:
        raise Rejected("run_not_found")
    if (run.get("repository") or {}).get("full_name") != repo:
        raise Rejected("run_in_other_repo")
    if run.get("head_sha") != tags["commit"]:
        raise Rejected("run_commit_mismatch")
    if run.get("event") not in RUN_EVENTS[tags["environment"]]:
        raise Rejected("run_event_mismatch")


# --- Rendering ---------------------------------------------------------------


def render_title(defect: Defect) -> str:
    return f"[sentry {defect.short_id}] {defect.kind} in {defect.component}/{defect.stage}: {defect.invariant}"


def render_body(defect: Defect, labels: list[str]) -> str:
    """Only validated values are interpolated; all other text is fixed here."""
    suspect = f"`{defect.suspect_commit}`" if defect.suspect_commit else "none reported by Sentry"
    rows = [
        ("Kind", defect.kind),
        ("Component", defect.component),
        ("Stage", defect.stage),
        ("Arch", defect.arch),
        ("Invariant", defect.invariant),
        ("Category", defect.category),
        ("Environment", defect.environment),
        ("Commit", defect.commit),
        ("Repro", defect.repro_id),
    ]
    table = "\n".join(f"| {name} | `{value}` |" for name, value in rows)
    label_list = ", ".join(f"`{label}`" for label in labels) or "none"
    return f"""<!-- sentry-issue: {defect.sentry_id} -->
Filed by `scripts/ci/sentry_triage.py` from a Sentry CI defect event. Every value
below is a validated contract tag or Sentry metadata; no event text is copied.

## Defect

| Field | Value |
| --- | --- |
{table}

## Sentry

- Issue: [{defect.short_id}]({defect.permalink})
- Events: {defect.events}; first seen {defect.first_seen}; last seen {defect.last_seen}
- Run: {defect.run_url}
- Suspect commit: {suspect}

## Probe

Check out `{defect.commit}` (on `{BASE_BRANCH}`) and rerun the check that reported
this, against `{defect.repro_id}`. Not yet recorded: add the command and its output
on a named `{BASE_BRANCH}` SHA.

## Acceptance

Not yet written: the expected behaviour as a test in `{defect.component}`, failing
on `{defect.commit}` and passing after the fix.

## Triage

Labels by rule ({label_list}): see "Sentry triage" in `docs/ISSUE_PR_WORKFLOW.md`.
This stays `status:needs-repro` until the probe output and the acceptance check are
recorded on a named `{BASE_BRANCH}` SHA. A closed issue is never reopened by the triage.
"""


# --- Triage ------------------------------------------------------------------


@dataclass
class Decision:
    sentry_id: str
    action: str  # would_create | created | tracked | capped | rejected
    reason: str = ""
    github_issue: Optional[int] = None
    title: str = ""
    labels: tuple = ()
    body: str = ""


def utc_day(stamp: str) -> str:
    return stamp[:10] if isinstance(stamp, str) else ""


def triage(
    sentry: Sentry,
    github: GitHub,
    *,
    create: bool,
    now: datetime,
    run_cap: int,
    daily_cap: int,
    stats_period: str = "14d",
    root: Path = ROOT,
) -> list[Decision]:
    daily_cap = min(daily_cap, DAILY_CAP_MAX)
    issues = sentry.candidate_issues(stats_period)
    tracked = github.tracked()
    today = now.astimezone(timezone.utc).strftime("%Y-%m-%d")
    created_today = len(
        {entry["number"] for entry in tracked.values() if utc_day(entry["created_at"]) == today}
    )
    rules = labeler_rules((root / ".github/labeler.yml").read_text())
    allowed_labels = managed_labels(root) & github.labels()
    decisions: list[Decision] = []
    new = 0
    ordered = sorted(issues, key=lambda i: (str(i.get("firstSeen", "")), str(i.get("id", ""))))
    for issue in ordered:
        raw_id = str(issue.get("id", ""))
        sentry_id = raw_id if SENTRY_ID_RE.match(raw_id) else "invalid"
        try:
            meta = validate_issue(issue, sentry.org, sentry.project)
            existing = tracked.get(meta["sentry_id"]) or tracked.get(meta["short_id"])
            if existing:
                decisions.append(
                    Decision(sentry_id, "tracked", existing["state"] or "", existing["number"])
                )
                continue
            event = sentry.latest_event(meta["sentry_id"])
            tags = validate_tags(event, meta["sentry_id"], github.repo)
            check_eligible(tags)
            check_run(github.run(tags["run_id"]), tags, github.repo)
            if not github.on_base_branch(tags["commit"]):
                raise Rejected("commit_not_on_main")
        except Rejected as reason:
            decisions.append(Decision(sentry_id, "rejected", str(reason)))
            continue
        if new >= run_cap or created_today + new >= daily_cap:
            decisions.append(Decision(sentry_id, "capped", "daily_cap" if new < run_cap else "run_cap"))
            continue
        event_id = event.get("eventID")
        suspect = sentry.suspect_commit(event_id) if isinstance(event_id, str) and EVENT_ID_RE.match(event_id) else None
        defect = Defect(
            **meta,
            **{key: tags[key] for key in TAG_CHECKS},
            run_url=tags[RUN_URL_TAG],
            run_id=tags["run_id"],
            suspect_commit=suspect,
        )
        wanted = [*FIXED_LABELS, *area_labels(defect.component, rules)]
        labels = [label for label in wanted if label in allowed_labels]
        title, body = render_title(defect), render_body(defect, labels)
        number = github.create(title, body, labels) if create else None
        if number is not None:
            tracked[defect.sentry_id] = {"number": number, "state": "open", "created_at": now.isoformat()}
        decisions.append(
            Decision(sentry_id, "created" if create else "would_create", "", number, title, tuple(labels), body)
        )
        new += 1
    return decisions


def summary_markdown(decisions: list[Decision], create: bool, scanned_note: str = "") -> str:
    counts: dict[str, int] = {}
    for d in decisions:
        counts[d.action] = counts.get(d.action, 0) + 1
    lines = [
        "# Sentry triage",
        "",
        f"Mode: **{'create' if create else 'dry run'}**. Candidates: {len(decisions)}"
        + "".join(f"; {action}: {n}" for action, n in sorted(counts.items())),
    ]
    if scanned_note:
        lines += ["", scanned_note]
    if decisions:
        lines += ["", "| Sentry issue | Action | Reason | GitHub issue |", "| --- | --- | --- | --- |"]
        for d in decisions:
            lines.append(
                f"| {d.sentry_id} | {d.action} | {d.reason or '-'} | {('#' + str(d.github_issue)) if d.github_issue else '-'} |"
            )
    for d in decisions:
        if d.body:
            lines += [
                "",
                f"<details><summary>{d.title}</summary>",
                "",
                "Labels: " + (", ".join(d.labels) or "none"),
                "",
                "~~~~markdown",
                d.body.rstrip("\n"),
                "~~~~",
                "",
                "</details>",
            ]
    return "\n".join(lines) + "\n"


def main(argv: Optional[list[str]] = None, transport: Transport = urllib_transport, env=None) -> int:
    env = os.environ if env is None else env
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--create", action="store_true", help="file issues (also needs SENTRY_TRIAGE_ENABLED=true)")
    parser.add_argument("--org", default=DEFAULT_ORG)
    parser.add_argument("--project", default=DEFAULT_PROJECT)
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument("--run-cap", type=int, default=5, help="new issues per run (default 5)")
    parser.add_argument("--daily-cap", type=int, default=DAILY_CAP_MAX, help="new issues per UTC day, at most 10")
    parser.add_argument("--stats-period", default="14d", help="Sentry lookback, e.g. 14d or 24h")
    parser.add_argument("--summary", type=Path, help="append a Markdown summary here")
    parser.add_argument("--out", type=Path, help="write the decisions as JSON here")
    args = parser.parse_args(argv)

    if not (SLUG_RE.match(args.org) and SLUG_RE.match(args.project)):
        parser.error("--org/--project must be Sentry slugs")
    if not re.match(r"\A[A-Za-z0-9-]+/[A-Za-z0-9._-]+\Z", args.repo):
        parser.error("--repo must be owner/name")
    if not STATS_PERIOD_RE.match(args.stats_period):
        parser.error("--stats-period must look like 14d or 24h")
    if not 0 <= args.daily_cap <= DAILY_CAP_MAX or args.run_cap < 0:
        parser.error(f"--daily-cap must be 0..{DAILY_CAP_MAX} and --run-cap non-negative")
    create = args.create
    if create and env.get("SENTRY_TRIAGE_ENABLED") != "true":
        print("error: --create needs SENTRY_TRIAGE_ENABLED=true; nothing was filed", file=sys.stderr)
        return 2

    sentry_token = env.get("SENTRY_AUTH_TOKEN", "")
    github_token = env.get("GITHUB_TOKEN") or env.get("GH_TOKEN") or ""
    if not sentry_token:
        note = "SENTRY_AUTH_TOKEN is not set; Sentry was not queried."
        print(note)
        if args.summary:
            with args.summary.open("a") as handle:
                handle.write(summary_markdown([], create, note))
        return 0
    if not github_token:
        print("error: GITHUB_TOKEN (or GH_TOKEN) is required", file=sys.stderr)
        return 2

    sentry = Sentry(
        Api(SENTRY_API, {"Authorization": f"Bearer {sentry_token}", "Accept": "application/json"}, transport, False),
        args.org,
        args.project,
    )
    github = GitHub(
        Api(
            GITHUB_API,
            {
                "Authorization": f"Bearer {github_token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "fission-sentry-triage",
            },
            transport,
            allow_write=create,
        ),
        args.repo,
    )
    try:
        decisions = triage(
            sentry,
            github,
            create=create,
            now=datetime.now(timezone.utc),
            run_cap=args.run_cap,
            daily_cap=args.daily_cap,
            stats_period=args.stats_period,
        )
    except ApiError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    report = summary_markdown(decisions, create)
    print(report)
    if args.summary:
        with args.summary.open("a") as handle:
            handle.write(report)
    if args.out:
        args.out.write_text(
            json.dumps({"mode": "create" if create else "dry-run", "decisions": [asdict(d) for d in decisions]}, indent=2)
            + "\n"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
