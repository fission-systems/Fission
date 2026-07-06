#!/usr/bin/env python3
"""Plan or run a six-month regression replay audit for bug-fix commits."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class CandidateCommit:
    commit: str
    date: str
    subject: str


def candidate_commits(root: Path, since: str) -> list[CandidateCommit]:
    proc = subprocess.run(
        [
            "git",
            "log",
            f"--since={since}",
            "--regexp-ignore-case",
            "--grep=fix",
            "--grep=bug",
            "--grep=regression",
            "--grep=semantic",
            "--format=%H%x09%cs%x09%s",
        ],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    rows = []
    for line in proc.stdout.splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3:
            rows.append(CandidateCommit(parts[0], parts[1], parts[2]))
    return rows


def render_markdown(root: Path, since: str, commits: list[CandidateCommit]) -> str:
    lines = [
        "# Regression Replay Plan",
        "",
        f"- Repo: `{root}`",
        f"- Since: `{since}`",
        f"- Candidate commits: `{len(commits)}`",
        "",
        "## Method",
        "",
        "For each candidate commit, run the selected benchmark subset at the parent",
        "commit and at the candidate commit. Count newly passing rows, newly failing",
        "rows, and net semantic delta. Keep artifacts under `docs/audits/` or an",
        "external run directory; do not update dashboards from this audit.",
        "",
        "## Candidate Commits",
        "",
    ]
    if commits:
        lines += ["| Date | Commit | Subject |", "|---|---|---|"]
        for commit in commits:
            lines.append(f"| {commit.date} | `{commit.commit[:12]}` | {commit.subject} |")
    else:
        lines.append("No candidate commits matched the fix/bug/regression/semantic query.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--since", default="6.months")
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--execute",
        action="store_true",
        help="Reserved for a future benchmark runner integration. Current implementation is plan-only.",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    commits = candidate_commits(root, args.since)
    if args.execute:
        print("regression_replay: --execute is reserved until a stable replay runner is wired.", file=sys.stderr)
        return 2
    if args.format == "json":
        rendered = json.dumps({"root": str(root), "since": args.since, "commits": [asdict(c) for c in commits]}, indent=2, sort_keys=True)
    else:
        rendered = render_markdown(root, args.since, commits)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
