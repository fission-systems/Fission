#!/usr/bin/env python3
"""Mine mechanical documentation defects and file them as before -> after issues.

The free-model draft flow (`free_model_draft.py`, `.github/workflows/free-model-draft.yml`)
works best on a concrete replacement. This script finds those replacements
deterministically; no model takes part in finding them.

Detectors (each yields a finding only when the fix is determinable):
  home-path    an absolute home path (`/Users/<name>/...`, `/home/<name>/...`) in
               maintained Markdown. `.../fission-benchmark/...` becomes
               `~/fission-benchmark/...`; a path into the repo checkout becomes the
               repo-relative path (only when that target exists).
  broken-link  a relative Markdown link whose target is missing but whose basename
               exists exactly once elsewhere in the repo. Ambiguous or missing
               targets are skipped.

Findings are grouped per kind into issues of at most 12 files and 40 replacements.
Each body carries one hidden `<!-- free-task-item:<16 hex> -->` key per finding (a hash
of kind, file, before, after; no line number). Findings whose key already appears in any
issue labelled agent:free (open or closed) are dropped before grouping.

Default mode is a dry run. `--create` files the issues with `gh` (labels agent:free
and area:docs) and starts `free-model-draft.yml` for each, because issues created
with GITHUB_TOKEN do not trigger workflows.
"""

import argparse
import hashlib
import json
import os
import posixpath
import re
import subprocess
import sys
from collections import namedtuple

Finding = namedtuple("Finding", "kind file line before after")

MAX_FILES = 12
MAX_REPLACEMENTS = 40

# Excluded from every detector (a top-level `target*` dir is excluded too).
EXCLUDED_DIRS = ("docs/reports/", "docs/validation/", "docs/audits/", "docs/changelog/",
                 "vendor/", "benchmark/", ".claude/", ".git/", "workspace/")
# Historical records: home paths are fixed, links are left alone.
NO_LINK_FIX_DIRS = ("docs/proposals/", "docs/plans/")
# Never indexed as link targets.
INDEX_SKIP_DIRS = ("vendor/", "benchmark/", ".claude/", ".git/", "workspace/")

HOME_RE = re.compile(r"/(?:Users|home)/[^/\s`()\[\]<>\"']+(?:/[^\s`()\[\]<>\"']*)?")
LINK_RE = re.compile(r"(?<!!)\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
SCHEME_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")
FENCE_RE = re.compile(r"^\s*(```|~~~)")


def _top_is_target(path):
    return path.split("/", 1)[0].startswith("target")


def in_scope(path):
    """Maintained docs: root *.md, docs/**/*.md, crates/**/*.md."""
    if not path.endswith(".md") or _top_is_target(path):
        return False
    if any(path.startswith(d) for d in EXCLUDED_DIRS):
        return False
    return "/" not in path or path.startswith("docs/") or path.startswith("crates/")


def list_files(root):
    """Repo-relative file paths: git's tracked list when available, else a walk."""
    try:
        out = subprocess.run(["git", "ls-files", "-z"], cwd=root, check=True,
                             capture_output=True).stdout.decode("utf-8", "replace")
        files = [p for p in out.split("\0") if p and os.path.isfile(os.path.join(root, p))]
        if files:
            return sorted(files)
    except (OSError, subprocess.CalledProcessError):
        pass
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root).replace(os.sep, "/")
        dirnames[:] = [d for d in dirnames if d != ".git"]
        for name in filenames:
            files.append(name if rel_dir == "." else f"{rel_dir}/{name}")
    return sorted(files)


def read_lines(root, path):
    """(line, inside_fenced_block) pairs for a file; empty when unreadable."""
    try:
        with open(os.path.join(root, path), encoding="utf-8") as fh:
            text = fh.read()
    except (OSError, UnicodeDecodeError):
        return []
    fenced, result = False, []
    for line in text.splitlines():
        if FENCE_RE.match(line):
            fenced = not fenced
            result.append((line, True))
        else:
            result.append((line, fenced))
    return result


def rel_link(from_file, target):
    return posixpath.relpath(target, posixpath.dirname(from_file) or ".")


def home_fix(token, prefix, path, existing):
    """The portable replacement for one absolute home path, or None."""
    marker = "/fission-benchmark"
    idx = token.find(marker)
    if idx != -1 and token[idx + len(marker):idx + len(marker) + 1] in ("", "/"):
        return "~" + token[idx:]
    m = re.match(r"/(?:Users|home)/[^/]+/Fission/(.+)$", token)
    if not m or ".claude/" in token:
        return None
    rel = m.group(1).rstrip("/")
    if rel not in existing and not any(p.startswith(rel + "/") for p in existing):
        return None
    return rel_link(path, rel) if prefix.endswith("](") else rel


def detect_home_paths(root, files):
    findings = []
    existing = set(files)
    for path in files:
        if not in_scope(path):
            continue
        for number, (line, _fenced) in enumerate(read_lines(root, path), 1):
            for match in HOME_RE.finditer(line):
                token = match.group(0).rstrip(".,;:!?")
                after = home_fix(token, line[:match.start()], path, existing)
                if after and after != token:
                    findings.append(Finding("home-path", path, number, token, after))
    return findings


def _scope_dir(path):
    """`crates/<name>/` for crate sources, else the top-level directory ("" at the root)."""
    parts = path.split("/")
    if parts[0] == "crates" and len(parts) > 2:
        return "crates/" + parts[1]
    return parts[0] if len(parts) > 1 else ""


def plausible_target(source, candidate):
    """A same-named file is trusted only if it is Markdown or lives next to the source."""
    return candidate.endswith(".md") or _scope_dir(source) == _scope_dir(candidate)


def link_fix(raw, path, existing, by_name):
    """The corrected link target, or None when it is fine or not determinable."""
    if raw.startswith(("#", "<", "/")) or SCHEME_RE.match(raw):
        return None
    target, sep, frag = raw.partition("#")
    target = target.partition("?")[0]
    if not target or target.endswith("/"):
        return None
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(path), target))
    if resolved in existing or any(p.startswith(resolved + "/") for p in existing):
        return None
    candidates = by_name.get(posixpath.basename(target), [])
    if len(candidates) != 1 or not plausible_target(path, candidates[0]):
        return None
    return rel_link(path, candidates[0]) + (sep + frag if sep else "")


def detect_broken_links(root, files):
    findings = []
    existing = set(files)
    by_name = {}
    for p in files:
        if _top_is_target(p) or any(p.startswith(d) for d in INDEX_SKIP_DIRS):
            continue
        by_name.setdefault(posixpath.basename(p), []).append(p)
    for path in files:
        if not in_scope(path) or any(path.startswith(d) for d in NO_LINK_FIX_DIRS):
            continue
        for number, (line, fenced) in enumerate(read_lines(root, path), 1):
            if fenced:
                continue
            for match in LINK_RE.finditer(line):
                after = link_fix(match.group(1), path, existing, by_name)
                if after:
                    findings.append(Finding("broken-link", path, number, match.group(1), after))
    return findings


def mine(root):
    files = list_files(root)
    found = detect_home_paths(root, files) + detect_broken_links(root, files)
    return sorted(set(found), key=lambda f: (f.kind, f.file, f.line, f.before))


def group_findings(findings):
    """Per kind, groups of at most MAX_FILES files and MAX_REPLACEMENTS findings."""
    groups = []
    for kind in sorted({f.kind for f in findings}):
        current, names = [], set()
        for f in sorted(x for x in findings if x.kind == kind):
            if current and (len(current) >= MAX_REPLACEMENTS
                            or (f.file not in names and len(names) >= MAX_FILES)):
                groups.append(current)
                current, names = [], set()
            current.append(f)
            names.add(f.file)
        if current:
            groups.append(current)
    return groups


def item_key(finding):
    """Identity of one defect: independent of its line number and of grouping."""
    payload = json.dumps([finding.kind, finding.file, finding.before, finding.after],
                         ensure_ascii=True)
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


def fingerprint(group):
    payload = json.dumps(sorted(list(f) for f in group), ensure_ascii=True)
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


ITEM_MARKER_RE = re.compile(r"<!-- free-task-item:([0-9a-f]{16}) -->")

TITLES = {"home-path": "docs: replace absolute home paths with portable paths",
          "broken-link": "docs: fix broken relative Markdown links"}
INTROS = {"home-path": "Replace each absolute home-directory path below with the given portable path.",
          "broken-link": "Each link target below does not exist, but the file exists exactly once "
                         "elsewhere in the repo. Replace the link target and keep the link text."}


def render_title(group):
    n, files = len(group), len({f.file for f in group})
    return f"{TITLES[group[0].kind]} ({n} in {files} file{'s' if files != 1 else ''})"


def render_body(group):
    lines = [INTROS[group[0].kind], "",
             "Edit only the listed lines in these Markdown files; change nothing else.", ""]
    for f in sorted(group):
        lines.append(f"- `{f.file}:{f.line}`: `{f.before}` → `{f.after}`")
    lines += ["", "Found by `scripts/ci/free_task_miner.py`.", ""]
    lines += [f"<!-- free-task-item:{item_key(f)} -->" for f in sorted(group)]
    lines.append(f"<!-- free-task:{fingerprint(group)} -->")
    return "\n".join(lines) + "\n"


def known_markers(issues):
    found = set()
    for item in issues:
        found.update(ITEM_MARKER_RE.findall(item.get("body") or ""))
    return found


def plan(findings, markers, max_issues):
    """Drop already-filed findings, then group the rest and cap the issue count."""
    fresh = [f for f in findings if item_key(f) not in markers]
    return group_findings(fresh)[:max_issues]


def gh_existing_issues():
    out = subprocess.run(["gh", "issue", "list", "--label", "agent:free", "--state", "all",
                          "--limit", "1000", "--json", "number,body"],
                         check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def gh_create(title, body):
    out = subprocess.run(["gh", "issue", "create", "--title", title, "--body-file", "-",
                          "--label", "agent:free", "--label", "area:docs"],
                         input=body, check=True, capture_output=True, text=True).stdout.strip()
    number = out.rstrip("/").rsplit("/", 1)[-1]
    if not number.isdigit():
        raise RuntimeError(f"unexpected gh output: {out}")
    return number


def gh_start_draft(number):
    subprocess.run(["gh", "workflow", "run", "free-model-draft.yml", "-f", f"issue={number}"],
                   check=True)


def main(argv=None):
    ap = argparse.ArgumentParser(description="File mechanical docs fixes as before -> after issues.")
    ap.add_argument("--root", default=".")
    ap.add_argument("--max-issues", type=int, default=3)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="print what would be filed (default)")
    mode.add_argument("--create", action="store_true", help="file the issues with gh")
    args = ap.parse_args(argv)

    findings = mine(args.root)
    groups = group_findings(findings)
    print(f"{len(findings)} findings in {len(groups)} groups")
    try:
        markers = known_markers(gh_existing_issues())
    except (OSError, subprocess.CalledProcessError):
        if args.create:
            raise
        markers = set()
        print("note: gh unavailable; not checking for existing issues", file=sys.stderr)
    chosen = plan(findings, markers, args.max_issues)
    filed = sum(1 for f in findings if item_key(f) in markers)
    print(f"{filed} findings already filed; {len(chosen)} issues to file (max {args.max_issues})")
    for group in chosen:
        title, body = render_title(group), render_body(group)
        if args.create:
            number = gh_create(title, body)
            print(f"created #{number}: {title}")
            gh_start_draft(number)
        else:
            print(f"--- would file: {title}\n{body}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
