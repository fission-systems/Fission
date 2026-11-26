#!/usr/bin/env python3
"""Draft a release's notes from the pull requests merged since the last tag.

Deterministic and stdlib only: no AI, no network except `gh api`, and the
same input always renders the same bytes. Two halves, so the renderer can be
tested on canned JSON:

  draft_notes.py collect --prev v0.3.0 --target <sha> --out prs.json
  draft_notes.py render  --data prs.json --version v0.4.0 --out docs/changelog/v0.4.0.md
  draft_notes.py draft   --version v0.4.0 [--prev TAG] [--target SHA] --out ...

What goes where:

* A PR belongs to the range when its merge commit is reachable from the
  target and not from the previous tag (`git rev-list prev..target`), so a
  stacked PR merged into another branch first is still found.
* Sections follow `.github/release.yml` (GitHub's own generated notes use
  the same labels; a test keeps the two in step). The first matching section
  wins, in the order below.
* A PR with no managed label is classified by its conventional title prefix:
  `fix` -> Correctness Fixes, `perf` -> Performance, `docs` -> Documentation,
  `test`/`ci` -> Testing & Infrastructure, `refactor` -> Internal,
  `feat` -> New Capabilities, and `type!:` -> Breaking Changes.
* Anything else goes under Other and is named in a "needs a label" line, so
  nothing is dropped silently. Bot authors and `skip-changelog` PRs are
  excluded and recorded in a trailing HTML comment.
* Numbers are quoted only from a PR's evidence sections, the template's
  `## Benchmark Impact` and `## Correctness Evidence` (any heading that starts
  with "Benchmark" or is "Correctness Evidence"), plus a top-level
  `## Measurement ...` / `## Measured ...` section, where many PRs put their
  numbers; verbatim and attributed. Nothing is computed, summed or compared
  across PRs.
* The full FreshBench sweep table is not in any PR, so it is a placeholder the
  maintainer fills. Known limitations come from open issues labelled
  `release:blocker`, or `bug` and `status:confirmed`.

Placeholders carry the marker `TODO(release)`; `release-tag.yml` refuses to
publish notes that still contain it. The summary, quality and known-limitation
blocks belong to the maintainer once drafted: `--previous-notes` (the draft on
the Release PR branch) keeps their text when the notes are regenerated.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import urllib.parse
from pathlib import Path

DEFAULT_REPO = "fission-systems/Fission"
SCHEMA = 1
TODO_MARKER = "TODO(release)"

# (key, heading, labels). Order is precedence and print order. Keep in step
# with .github/release.yml; test_draft_notes checks that it is.
SECTIONS: list[tuple[str, str, tuple[str, ...]]] = [
    ("breaking", "Breaking Changes", ("risk:breaking",)),
    ("correctness", "Correctness Fixes", ("bug", "risk:semantic")),
    ("reliability", "Reliability", ("phase:2-reliability",)),
    ("performance", "Performance", ("type:performance",)),
    ("capabilities", "New Capabilities", ("enhancement",)),
    ("internal", "Internal", ("type:refactor",)),
    ("testing", "Testing & Infrastructure", ("type:test", "area:ci")),
    ("documentation", "Documentation", ("documentation",)),
    ("other", "Other", ()),
]
SECTION_ORDER = {key: i for i, (key, _, _) in enumerate(SECTIONS)}

# Fallback for a PR that carries none of the labels above.
TITLE_PREFIX = {
    "fix": "correctness",
    "perf": "performance",
    "docs": "documentation",
    "test": "testing",
    "ci": "testing",
    "refactor": "internal",
    "feat": "capabilities",
}
PREFIX_RE = re.compile(r"^(?P<type>[A-Za-z]+)(?:\([^)]*\))?(?P<bang>!)?:\s")

SKIP_LABELS = frozenset({"skip-changelog"})
# pr_body() starts every Release PR description with this.
RELEASE_PR_MARKER = "<!-- release-pr: "

EVIDENCE_HEADING_RE = re.compile(
    r"^(?P<hashes>#{1,6})\s+(?P<title>(?:benchmark\b.*|correctness evidence\b.*?))\s*#*\s*$",
    re.IGNORECASE,
)
# A PR's own measurement section. Only at `##`, the PR body's top level: a
# "Measured" subsection of some other section is not an evidence section.
MEASUREMENT_HEADING_RE = re.compile(
    r"^##\s+(?P<title>measure(?:ments?|d)\b.*?)\s*#*\s*$",
    re.IGNORECASE,
)
HEADING_RE = re.compile(r"^(?P<hashes>#{1,6})\s+\S")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
# Lines of the PR template's measurement block that were never filled in.
PLACEHOLDER_RE = re.compile(r"<(?:before|after|config|functions)\b", re.IGNORECASE)
MAX_QUOTED_LINES = 20

ARCHIVES = [
    ("Linux x86-64", "fission-x86_64-unknown-linux-gnu.tar.gz"),
    ("Linux arm64", "fission-aarch64-unknown-linux-gnu.tar.gz"),
    ("macOS Apple silicon", "fission-aarch64-apple-darwin.tar.gz"),
    ("macOS Intel", "fission-x86_64-apple-darwin.tar.gz"),
    ("Windows x64", "fission-x86_64-pc-windows-msvc.zip"),
]

NUMBER_WORDS = {3: "three", 4: "four", 5: "five", 6: "six", 7: "seven"}

SEMVER_RE = re.compile(
    r"^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[\da-zA-Z-]+(\.[\da-zA-Z-]+)*)?$"
)


def normalize_tag(version: str) -> str:
    version = version.strip()
    if not SEMVER_RE.match(version):
        raise SystemExit(f"draft_notes: {version!r} is not a SemVer version")
    return version if version.startswith("v") else f"v{version}"


# ---------------------------------------------------------------- classify


def is_bot(pr: dict) -> bool:
    return bool(pr.get("author_is_bot")) or str(pr.get("author", "")).endswith("[bot]")


def exclusion(pr: dict) -> str | None:
    if is_bot(pr):
        return "bot author"
    if RELEASE_PR_MARKER in (pr.get("body") or ""):
        # A Release PR's body is an earlier release's notes; quoting its
        # "Measured evidence" again would repeat that release's numbers.
        return "release PR"
    skipped = sorted(SKIP_LABELS & set(pr.get("labels", [])))
    if skipped:
        return skipped[0]
    return None


def classify(pr: dict) -> tuple[str, str]:
    """Return (section key, how): how is "label", "prefix" or "unclassified"."""
    labels = set(pr.get("labels", []))
    for key, _, section_labels in SECTIONS:
        if labels.intersection(section_labels):
            return key, "label"
    m = PREFIX_RE.match(pr.get("title", ""))
    if m:
        if m.group("bang"):
            return "breaking", "prefix"
        key = TITLE_PREFIX.get(m.group("type").lower())
        if key:
            return key, "prefix"
    return "other", "unclassified"


# ---------------------------------------------------------------- evidence


def evidence_blocks(body: str) -> list[tuple[str, list[str]]]:
    """Quote the measured lines of each evidence section, verbatim.

    Only text under a heading that starts with "Benchmark" or is "Correctness
    Evidence", or a `##` heading that starts with "Measurement"/"Measured",
    counts, outside HTML comments. A prose line is quoted when it
    has a digit. A markdown table or a fenced block is quoted whole when any
    of its lines has a digit, so a header or a wrapped line stays with its
    numbers. Unfilled template placeholders (`<before> -> <after>`) are not
    quoted, and a block left with nothing but placeholders is dropped.
    """
    text = COMMENT_RE.sub("", (body or "").replace("\r\n", "\n"))
    blocks: list[tuple[str, list[str]]] = []
    current: tuple[str, int, list[str]] | None = None
    group: list[str] = []  # the table or fenced block being read
    in_fence = False

    def measured(line: str) -> bool:
        return bool(re.search(r"\d", line)) and not PLACEHOLDER_RE.search(line)

    def flush_group() -> None:
        if current is not None and any(measured(line) for line in group):
            kept = [line for line in group if not PLACEHOLDER_RE.search(line)]
            while kept and not kept[0].strip():
                kept.pop(0)
            while kept and not kept[-1].strip():
                kept.pop()
            if current[2]:
                current[2].append("")
            current[2].extend(kept)
        group.clear()

    def close() -> None:
        nonlocal current
        flush_group()
        if current is not None and current[2]:
            blocks.append((current[0], list(current[2])))
        current = None

    for raw in text.split("\n"):
        line = raw.rstrip()
        if FENCE_RE.match(line):
            if in_fence:
                flush_group()
            in_fence = not in_fence
            continue
        if in_fence:
            group.append(line)
            continue
        heading = HEADING_RE.match(line)
        if heading:
            flush_group()
            level = len(heading.group("hashes"))
            if current is not None and level <= current[1]:
                close()
            evidence = EVIDENCE_HEADING_RE.match(line) or MEASUREMENT_HEADING_RE.match(line)
            if evidence and current is None:
                current = (evidence.group("title").strip(), level, [])
            continue
        if line.strip().startswith("|"):
            group.append(line)
            continue
        flush_group()
        if current is not None and measured(line.strip()):
            current[2].append(line)
    close()
    return blocks


# ---------------------------------------------------------------- render


def installing(tag: str, repo: str) -> list[str]:
    rows = "\n".join(f"| {platform} | `{name}` |" for platform, name in ARCHIVES)
    return f"""## Installing

```bash
# Homebrew (macOS, Linux)
brew install fission-systems/tap/fission-cli

# Install script (macOS, Linux): installs into ~/.local and checks the SHA-256
curl -fsSL https://raw.githubusercontent.com/{repo}/main/install.sh | sh
```

**Cargo** builds from source. The processor specifications and signatures come
from this release:

```bash
cargo install --locked --git https://github.com/{repo} --tag {tag} fission-cli
mkdir -p ~/.cargo/share/fission
curl -fsSL https://github.com/{repo}/releases/download/{tag}/fission-resources.tar.gz \\
  | tar -xz -C ~/.cargo/share/fission
```

**Archives** are attached to the release for {NUMBER_WORDS.get(len(ARCHIVES), len(ARCHIVES))} targets:

| Platform | Archive |
| --- | --- |
{rows}

Unpack one anywhere and run `bin/fission_cli`; it finds `share/fission/` next
to itself. `SHA256SUMS` covers every asset.
""".split("\n")


def fence_for(lines: list[str]) -> str:
    return "````" if any("```" in line for line in lines) else "```"


BLOCK_RE = re.compile(
    r"<!-- maintainer:(?P<name>[a-z-]+) -->\n(?P<body>.*?)<!-- /maintainer:(?P=name) -->",
    re.DOTALL,
)


def maintainer_blocks(text: str | None) -> dict[str, str]:
    """The hand-edited blocks of an earlier draft, by name."""
    return {m.group("name"): m.group("body") for m in BLOCK_RE.finditer(text or "")}


def block(name: str, default: list[str], kept: dict[str, str]) -> list[str]:
    """A block the maintainer owns once drafted: a re-run keeps its text."""
    body = kept[name].rstrip("\n").split("\n") if name in kept else default
    return [f"<!-- maintainer:{name} -->", *body, f"<!-- /maintainer:{name} -->"]


def render(data: dict, version: str, previous: str | None = None) -> str:
    """Render the notes. `previous` is an earlier draft whose maintainer
    blocks (summary, quality, known limitations) are kept verbatim."""
    tag = normalize_tag(version)
    kept = maintainer_blocks(previous)
    repo = data.get("repo", DEFAULT_REPO)
    prev = data["previous_tag"]
    prs = sorted(data.get("pull_requests", []), key=lambda p: p["number"])

    included: list[tuple[str, dict]] = []
    excluded: list[tuple[int, str]] = []
    unlabelled: list[int] = []
    for pr in prs:
        reason = exclusion(pr)
        if reason:
            excluded.append((pr["number"], reason))
            continue
        key, how = classify(pr)
        if how == "unclassified":
            unlabelled.append(pr["number"])
        included.append((key, pr))
    included.sort(key=lambda kp: (SECTION_ORDER[kp[0]], kp[1]["number"]))

    out: list[str] = [f"# Fission {tag}", ""]
    out.append(
        f"<!-- release-candidate: {data['target_sha']} previous: {prev} "
        "generated-by: scripts/release/draft_notes.py -->"
    )
    out.append("")
    out.extend(
        block(
            "summary",
            ["<!-- Optional, for a major release only: a short hand-written summary. -->"],
            kept,
        )
    )
    out.append("")
    noun = "pull request" if len(prs) == 1 else "pull requests"
    out.append(f"{len(prs)} {noun} merged since {prev}.")
    out.append("")
    out.extend(installing(tag, repo))

    orphans = data.get("commits_without_pr", [])
    for key, heading, _ in SECTIONS:
        entries = [pr for k, pr in included if k == key]
        if not entries and not (key == "other" and orphans):
            continue
        out.append(f"## {heading}")
        out.append("")
        for pr in entries:
            out.append(f"- {pr['title'].strip()} (#{pr['number']})")
        if key == "other":
            for commit in orphans:
                out.append(f"- {commit['subject'].strip()} ({commit['sha'][:9]}, no pull request)")
        out.append("")
        if key == "other" and unlabelled:
            refs = ", ".join(f"#{n}" for n in unlabelled)
            out.append(
                f"Needs a label (no managed label and no conventional title prefix): {refs}."
            )
            out.append("")

    quoted = [
        (pr, blocks) for _, pr in included if (blocks := evidence_blocks(pr.get("body", "")))
    ]
    if quoted:
        out.append("## Measured evidence")
        out.append("")
        out.append(
            "Quoted verbatim from each PR's Benchmark, Measurement or Correctness Evidence "
            "section. "
            "Each number was measured by that PR, under its own load; nothing here is "
            "added up or compared across PRs. Benchmark scores measure similarity to the "
            "source, not correctness (ADR 0019)."
        )
        out.append("")
        for pr, blocks in quoted:
            for title, lines in blocks:
                out.append(f"From #{pr['number']} ({title}):")
                out.append("")
                shown = lines[:MAX_QUOTED_LINES]
                fence = fence_for(shown)
                out.append(f"{fence}text")
                out.extend(shown)
                out.append(fence)
                if len(lines) > len(shown):
                    out.append(f"({len(lines) - len(shown)} more lines in #{pr['number']})")
                out.append("")

    out.append("## Quality (full FreshBench sweep)")
    out.append("")
    quality = [
        f"> **{TODO_MARKER}:** fill this from full FreshBench M1 runs of "
        f"{prev} and this candidate (`freshbench sweep run`, then `sweep diff`; "
        "see docs/BENCHMARK_FRESHBENCH.md): GED perfect count next to distance, "
        "types perfect, decompile CPU time and failures. No PR carries this "
        "measurement. If no sweep was run, write \"Not measured for this release.\"",
        "",
        "GED (structure) measures how close the output's control flow is to the "
        "source's. It does not measure correctness (ADR 0019).",
    ]
    out.extend(block("quality", quality, kept))
    out.append("")

    out.append("## Known limitations")
    out.append("")
    issues = sorted(data.get("open_issues", []), key=lambda i: i["number"])
    if issues:
        limitations = [
            "Open issues labelled `release:blocker`, or `bug` and `status:confirmed`, "
            "when these notes were drafted:",
            "",
            *(f"- {issue['title'].strip()} (#{issue['number']})" for issue in issues),
        ]
    else:
        limitations = [
            f"> **{TODO_MARKER}:** no open issue is labelled `release:blocker`, or `bug` "
            "and `status:confirmed`. List the known limitations, or write \"None known.\""
        ]
    out.extend(block("limitations", limitations, kept))
    out.append("")

    out.append(f"[Compare with {prev}](https://github.com/{repo}/compare/{prev}...{tag})")
    if excluded:
        listed = ", ".join(f"#{n} ({why})" for n, why in excluded)
        out.append("")
        out.append(f"<!-- excluded: {listed} -->")
    return "\n".join(out).rstrip("\n") + "\n"


def candidate_record(data: dict, version: str) -> str:
    """`.github/release-candidate.json`: what release-tag.yml checks the tag against."""
    tag = normalize_tag(version)
    record = {
        "schema": SCHEMA,
        "tag": tag,
        "version": tag[1:],
        "candidate_sha": data["target_sha"],
        "previous_tag": data["previous_tag"],
        "notes": f"docs/changelog/{tag}.md",
    }
    return json.dumps(record, indent=2, sort_keys=True) + "\n"


def pr_body(data: dict, version: str, notes: str) -> str:
    """The Release PR's description: the pinned candidate, the two steps, the notes."""
    tag = normalize_tag(version)
    sha = data["target_sha"]
    return "\n".join(
        [
            f"{RELEASE_PR_MARKER}{tag} -->",
            f"Release **{tag}**. Candidate `{sha}`, notes drafted from the PRs merged in "
            f"`{data['previous_tag']}..{sha[:12]}` by `scripts/release/draft_notes.py`.",
            "",
            "This PR only bumps the workspace version, records the candidate in "
            "`.github/release-candidate.json` and adds the notes. It never tags or publishes.",
            "",
            "- [ ] Every `TODO(release)` block in "
            f"`docs/changelog/{tag}.md` is filled (release-tag refuses notes that still have one).",
            "- [ ] Required checks ran on this PR. A PR opened by the workflow's own token "
            "does not start CI: close and reopen it once (docs/CI_RELEASE_GATES.md).",
            "",
            "After merging, dispatch **CI Heavy Validation** on the merge commit (or wait for "
            f"the nightly), then **Release Tag (CI green)** with `ref` = that commit. The tag "
            f"defaults to `{tag}` from `.github/release-candidate.json`, and release-tag refuses "
            "if the merge commit "
            "contains anything but this PR on top of the candidate.",
            "",
            "---",
            "",
            notes.rstrip("\n"),
            "",
        ]
    )


# ---------------------------------------------------------------- collect


def run(cmd: list[str], cwd: Path | None = None) -> str:
    return subprocess.run(cmd, cwd=cwd, check=True, capture_output=True, text=True).stdout


def gh_api(path: str) -> object:
    return json.loads(run(["gh", "api", path]))


def git(git_dir: Path, *args: str) -> str:
    return run(["git", *args], cwd=git_dir).strip()


def previous_tag(git_dir: Path, target: str, version: str) -> str:
    tag = normalize_tag(version)
    cmd = ["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", "--exclude", tag]
    if "-" not in tag:
        # A final release follows the previous final release, not its rcs.
        cmd += ["--exclude", "v*-*"]
    return git(git_dir, *cmd, target)


def parse_time(value: str) -> dt.datetime:
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def collect(repo: str, prev: str, target: str, git_dir: Path) -> dict:
    prev_sha = git(git_dir, "rev-parse", f"{prev}^{{commit}}")
    target_sha = git(git_dir, "rev-parse", f"{target}^{{commit}}")
    in_range = set(git(git_dir, "rev-list", f"{prev_sha}..{target_sha}").split())
    # Every PR merged after the previous tag's commit was updated after it too.
    # Two days of margin absorb clock skew between git and GitHub.
    cutoff = parse_time(git(git_dir, "log", "-1", "--format=%cI", prev_sha)) - dt.timedelta(days=2)

    prs: dict[int, dict] = {}
    page = 1
    while True:
        items = gh_api(
            f"repos/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100&page={page}"
        )
        if not items:
            break
        for pr in items:
            if pr.get("merged_at") and pr.get("merge_commit_sha") in in_range:
                user = pr.get("user") or {}
                prs[pr["number"]] = {
                    "number": pr["number"],
                    "title": pr["title"],
                    "labels": sorted(label["name"] for label in pr.get("labels", [])),
                    "author": user.get("login", ""),
                    "author_is_bot": user.get("type") == "Bot",
                    "merge_commit_sha": pr["merge_commit_sha"],
                    "body": (pr.get("body") or "").replace("\r\n", "\n"),
                }
        if parse_time(items[-1]["updated_at"]) < cutoff:
            break
        page += 1

    merged = {pr["merge_commit_sha"] for pr in prs.values()}
    first_parent = git(git_dir, "rev-list", "--first-parent", f"{prev_sha}..{target_sha}").split()
    # Oldest first. A direct push to main has no PR; it is listed, not dropped.
    orphans = [
        {"sha": sha, "subject": git(git_dir, "log", "-1", "--format=%s", sha)}
        for sha in reversed(first_parent)
        if sha not in merged
    ]

    issues: dict[int, dict] = {}
    for labels in ("release:blocker", "bug,status:confirmed"):
        page = 1
        while True:
            query = urllib.parse.quote(labels, safe=",")
            items = gh_api(
                f"repos/{repo}/issues?state=open&labels={query}&per_page=100&page={page}"
            )
            if not items:
                break
            for issue in items:
                if "pull_request" in issue:
                    continue
                issues[issue["number"]] = {
                    "number": issue["number"],
                    "title": issue["title"],
                    "labels": sorted(label["name"] for label in issue.get("labels", [])),
                }
            page += 1

    return {
        "schema": SCHEMA,
        "repo": repo,
        "previous_tag": prev,
        "target_sha": target_sha,
        "pull_requests": [prs[n] for n in sorted(prs)],
        "commits_without_pr": orphans,
        "open_issues": [issues[n] for n in sorted(issues)],
    }


def dump(data: dict) -> str:
    return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def write(path: str | None, text: str) -> None:
    if path in (None, "-"):
        sys.stdout.write(text)
    else:
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(text, encoding="utf-8")


def read_optional(path: str | None) -> str | None:
    if path and Path(path).is_file():
        return Path(path).read_text(encoding="utf-8")
    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    def common_collect(p: argparse.ArgumentParser) -> None:
        p.add_argument("--repo", default=DEFAULT_REPO)
        p.add_argument("--target", default="HEAD", help="target commit (default HEAD)")
        p.add_argument("--git-dir", default=".", help="repository checkout")

    p_collect = sub.add_parser("collect", help="fetch merged PRs and open issues as JSON")
    common_collect(p_collect)
    p_collect.add_argument("--prev", required=True, help="previous release tag")
    p_collect.add_argument("--out", default="-")

    p_render = sub.add_parser("render", help="render notes from collected JSON")
    p_render.add_argument("--data", required=True)
    p_render.add_argument("--version", required=True)
    p_render.add_argument("--out", default="-")
    p_render.add_argument("--previous-notes")

    p_draft = sub.add_parser("draft", help="collect, then render")
    common_collect(p_draft)
    p_draft.add_argument("--version", required=True)
    p_draft.add_argument("--prev", help="previous tag (default: nearest older v* tag)")
    p_draft.add_argument("--out", default="-")
    p_draft.add_argument("--data-out", help="also save the collected JSON here")
    p_draft.add_argument(
        "--previous-notes", help="an earlier draft whose maintainer blocks are kept"
    )
    p_draft.add_argument("--pr-body-out", help="also write the Release PR description")
    p_draft.add_argument("--candidate-out", help="also write the candidate record JSON")

    args = parser.parse_args(argv)
    if args.cmd == "render":
        data = json.loads(Path(args.data).read_text(encoding="utf-8"))
        write(args.out, render(data, args.version, read_optional(args.previous_notes)))
        return 0

    git_dir = Path(args.git_dir)
    if args.cmd == "collect":
        write(args.out, dump(collect(args.repo, args.prev, args.target, git_dir)))
        return 0

    prev = args.prev or previous_tag(git_dir, args.target, args.version)
    data = collect(args.repo, prev, args.target, git_dir)
    if args.data_out:
        write(args.data_out, dump(data))
    notes = render(data, args.version, read_optional(args.previous_notes))
    write(args.out, notes)
    if args.pr_body_out:
        write(args.pr_body_out, pr_body(data, args.version, notes))
    if args.candidate_out:
        write(args.candidate_out, candidate_record(data, args.version))
    return 0


if __name__ == "__main__":
    sys.exit(main())
