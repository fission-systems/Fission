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
  missing-docs a public Rust item without a `///` comment, from the `missing_docs`
               warnings of one crate. The fix needs writing, but it is a comment only.
  rustdoc      a rustdoc lint warning of that crate (`rustdoc::broken_intra_doc_links`,
               `rustdoc::invalid_html_tags`, `rustdoc::bare_urls`, ...).

The two Rust detectors run on ONE crate per run, picked from DOC_CRATES by day of
year, so a run stays bounded. Their issues carry the hidden marker
`<!-- free-task-kind:comment-only -->`; free_model_draft.py then refuses any edit that
adds or removes a Rust line that is not a comment, so the model cannot change logic.

Findings are grouped per kind into issues (MAX_GROUP: 12 files and 40 replacements for
the Markdown kinds, 8 files and 15 items for the Rust kinds). Each body carries one
hidden `<!-- free-task-item:<16 hex> -->` key per finding (a hash of kind, file, before,
after; no line number). Findings whose key already appears in any issue labelled
agent:free (open or closed) are dropped before grouping. `--max-issues` caps the total
across kinds; kinds take turns, so one cannot starve another.

Findings with an exact before -> after (`home-path`, `broken-link`) need no model: `--fix`
applies one group of them to the working tree, each replacement only if `before` occurs
exactly once on the recorded file:line, and writes the PR title and body to `--out`. The
workflow (free-task-miner.yml), not a model, checks the result with
`free_model_draft.py check` and opens a draft PR labelled `agent:deterministic` on a branch
`agent-fix/<fingerprint>`. Findings that need writing (`missing-docs`, `rustdoc`) become
issues for the model. A finding already carried by any agent:free issue (open or closed) or
by an open agent:deterministic PR is not filed or fixed again.

Default mode is a dry run. `--create` files the model-kind issues with `gh` (labels
agent:free and area:docs). It does not start a draft: free-agent-scheduler.yml picks the
issues up, one task per tick.
"""

import argparse
import datetime
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
# kind -> (files, items) per issue, when it differs: the model has to write each
# Rust item's text, so these groups are smaller.
MAX_GROUP = {"missing-docs": (8, 15), "rustdoc": (8, 15)}
COMMENT_ONLY_MARKER = "<!-- free-task-kind:comment-only -->"
DETERMINISTIC_LABEL = "agent:deterministic"
# `--fix` opens nothing while this many deterministic PRs wait for review.
MAX_OPEN_DETERMINISTIC = 3

# Crates (directory names under crates/; the package name is the same) whose Rust
# docs are mined, one per run: DOC_CRATES[day of year % len]. Small, with few
# workspace dependencies, so a cold build stays cheap. Counts of missing_docs
# warnings when chosen (2026-11): analysis-protocol 34, ttd 30, cspec 43, plugin 4,
# project 74, evidence 128, signatures 189, pcode-ir 221, core 268. Left out on
# purpose: crates with their own AGENTS.md (solver, analysis-db) and the
# decompiler core. Append more once these are documented.
DOC_CRATES = ("fission-analysis-protocol", "fission-ttd", "fission-cspec", "fission-plugin",
              "fission-project", "fission-evidence", "fission-signatures", "fission-pcode-ir",
              "fission-core")
CARGO_TIMEOUT = 1500  # seconds, per cargo command

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


def pick_doc_crate(day_of_year):
    """The crate whose Rust docs this run mines: one per day, rotating through DOC_CRATES."""
    return DOC_CRATES[day_of_year % len(DOC_CRATES)]


def _clip(text, limit=120):
    """One source line, whitespace collapsed and cut to `limit` characters."""
    text = " ".join(text.split())
    return text if len(text) <= limit else text[:limit - 1] + "…"


def code_span(text):
    """Inline Markdown code; double backticks when the text holds a backtick itself."""
    return f"`` {text} ``" if "`" in text else f"`{text}`"


def parse_cargo_messages(output, crate, kind):
    """Findings of `kind` in cargo's `--message-format=json` output, for `crate` only.

    A compiler message counts only when its target's `src_path` and its primary span
    are both under `crates/<crate>/` (a path dependency's warnings, or an expansion
    into another file, are ignored). `missing-docs` keeps `missing_docs` warnings as
    (item header line, "" or "#n" for the n-th identical header in a file);
    `rustdoc` keeps `rustdoc::*` lints as (message, source line).
    """
    prefix = f"crates/{crate}/"
    found = []
    for raw in output.splitlines():
        try:
            msg = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(msg, dict) or msg.get("reason") != "compiler-message":
            continue
        src = str((msg.get("target") or {}).get("src_path", "")).replace(os.sep, "/")
        if f"/{prefix}" not in f"/{src}":
            continue
        diag = msg.get("message") or {}
        code = (diag.get("code") or {}).get("code") or ""
        wanted = code == "missing_docs" if kind == "missing-docs" else code.startswith("rustdoc::")
        if not wanted:
            continue
        spans = [s for s in diag.get("spans", []) if s.get("is_primary")]
        if not spans or not str(spans[0].get("file_name", "")).startswith(prefix):
            continue
        span = spans[0]
        text = _clip(((span.get("text") or [{}])[0]).get("text", ""))
        if kind == "missing-docs":
            found.append(Finding(kind, span["file_name"], span["line_start"], text, ""))
        else:
            found.append(Finding(kind, span["file_name"], span["line_start"],
                                 _clip(diag.get("message", ""), 200), text))
    found.sort(key=lambda f: (f.file, f.line, f.before, f.after))
    # Identical headers in one file (two `pub fn new() -> Self`) must keep distinct keys.
    seen, result = {}, []
    for f in found:
        if f.kind == "missing-docs":
            n = seen[(f.file, f.before)] = seen.get((f.file, f.before), 0) + 1
            f = f._replace(after=f"#{n}" if n > 1 else "")
        result.append(f)
    return list(dict.fromkeys(result))


def run_cargo(root, args, crate, kind):
    """Run one cargo command and parse it; a failed build is reported, never fatal."""
    # Build scripts of dependencies run here: they get no GitHub token.
    env = {k: v for k, v in os.environ.items() if k not in ("GH_TOKEN", "GITHUB_TOKEN")}
    try:
        proc = subprocess.run(["cargo", *args], cwd=root, capture_output=True, text=True, env=env,
                              timeout=CARGO_TIMEOUT, errors="replace")
    except (OSError, subprocess.TimeoutExpired) as err:
        print(f"::warning::{kind} detector skipped for {crate}: {err}", file=sys.stderr)
        return []
    found = parse_cargo_messages(proc.stdout, crate, kind)
    if proc.returncode != 0 and not found:
        print(f"::warning::cargo failed for {crate} ({kind}): {proc.stderr.strip()[-300:]}",
              file=sys.stderr)
    return found


def detect_missing_docs(root, crate):
    # `cargo rustc ... -- -W missing_docs` passes the lint to this crate's own
    # compilation only. RUSTFLAGS would change the flags of every dependency and
    # rebuild the whole tree; this rebuilds one crate and reuses cached dependencies.
    # Only the library target: binaries have no public API to document.
    return run_cargo(root, ["rustc", "--locked", "-p", crate, "--lib", "--message-format=json",
                            "--", "-W", "missing_docs"], crate, "missing-docs")


def detect_rustdoc_lints(root, crate):
    return run_cargo(root, ["doc", "--locked", "--no-deps", "-p", crate,
                            "--message-format=json"], crate, "rustdoc")


def mine(root, doc_crate=None):
    files = list_files(root)
    found = detect_home_paths(root, files) + detect_broken_links(root, files)
    if doc_crate:
        found += detect_missing_docs(root, doc_crate) + detect_rustdoc_lints(root, doc_crate)
    return sorted(set(found), key=lambda f: (f.kind, f.file, f.line, f.before))


def group_findings(findings):
    """Per kind, groups of at most MAX_GROUP[kind] (files, findings); default 12 and 40."""
    groups = []
    for kind in sorted({f.kind for f in findings}):
        max_files, max_items = MAX_GROUP.get(kind, (MAX_FILES, MAX_REPLACEMENTS))
        current, names = [], set()
        for f in sorted(x for x in findings if x.kind == kind):
            if current and (len(current) >= max_items
                            or (f.file not in names and len(names) >= max_files)):
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
          "broken-link": "docs: fix broken relative Markdown links",
          "missing-docs": "docs({crate}): document public items",
          "rustdoc": "docs({crate}): fix rustdoc warnings"}
INTROS = {"home-path": "Replace each absolute home-directory path below with the given portable path.",
          "broken-link": "Each link target below does not exist, but the file exists exactly once "
                         "elsewhere in the repo. Replace the link target and keep the link text.",
          "missing-docs": "Each public item below has no doc comment (`missing_docs` warning). Add "
                          "a `///` doc comment to each one, in the file and above the item at the "
                          "line given.",
          "rustdoc": "Each doc comment below makes `cargo doc` print a rustdoc warning. Fix only "
                     "those doc comments so the warning goes away."}
_RULES_HEAD = ["Rules (these edits are checked mechanically: any added or removed Rust line that is "
               "not a comment makes the whole draft be refused):",
               "- Change comments only. Do not change, add, remove, move or reformat any code, "
               "attribute, import or string."]
_RULES_TAIL = ["- Do not add code blocks or examples to doc comments (they would run as doctests).",
               "- Touch only the listed items."]
RUST_RULES = {
    "missing-docs": _RULES_HEAD + [
        "- Describe only what the code visibly does, in one or two plain sentences. Do not "
        "invent guarantees, invariants, units, panics, performance or history. If you "
        "cannot tell what an item is for from its code, write the shortest accurate "
        "description or leave it."] + _RULES_TAIL,
    "rustdoc": _RULES_HEAD + [
        "- Change the smallest part of the comment: fix the link or the tag, keep the rest of "
        "the wording. For an unresolved link, point it at the item's real path if you can see "
        "it in the code, otherwise turn it into plain code text without brackets. Escape or "
        "wrap in backticks a stray `<tag>` or bare URL."] + _RULES_TAIL,
}


def crate_of(group):
    parts = group[0].file.split("/")
    return parts[1] if parts[0] == "crates" and len(parts) > 2 else "workspace"


def render_title(group):
    n, files = len(group), len({f.file for f in group})
    title = TITLES[group[0].kind].format(crate=crate_of(group))
    return f"{title} ({n} in {files} file{'s' if files != 1 else ''})"


def render_item(f):
    if f.kind == "missing-docs":
        return f"- `{f.file}:{f.line}`: {code_span(f.before)}"
    if f.kind == "rustdoc":
        return f"- `{f.file}:{f.line}`: {f.before}: {code_span(f.after)}"
    return f"- `{f.file}:{f.line}`: `{f.before}` → `{f.after}`"


def render_body(group):
    kind = group[0].kind
    if kind in MAX_GROUP:
        lines = [INTROS[kind], ""] + RUST_RULES[kind] + [""]
    else:
        lines = [INTROS[kind], "",
                 "Edit only the listed lines in these Markdown files; change nothing else.", ""]
    for f in sorted(group):
        lines.append(render_item(f))
    lines += ["", "Found by `scripts/ci/free_task_miner.py`.", ""]
    if kind in MAX_GROUP:
        lines += [COMMENT_ONLY_MARKER]
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
    # Take turns between kinds, so that one kind's many groups cannot use the whole cap.
    by_kind = {}
    for group in group_findings(fresh):
        by_kind.setdefault(group[0].kind, []).append(group)
    chosen = []
    while by_kind and len(chosen) < max_issues:
        for kind in sorted(by_kind):
            if len(chosen) < max_issues:
                chosen.append(by_kind[kind].pop(0))
            if not by_kind[kind]:
                del by_kind[kind]
    return chosen


def gh_existing_issues():
    out = subprocess.run(["gh", "issue", "list", "--label", "agent:free", "--state", "all",
                          "--limit", "1000", "--json", "number,body"],
                         check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def gh_open_deterministic_prs():
    """Open `agent:deterministic` PRs: their findings are as good as filed."""
    out = subprocess.run(["gh", "pr", "list", "--label", DETERMINISTIC_LABEL, "--state", "open",
                          "--limit", "200", "--json", "number,body"],
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


def apply_fixes(root, findings):
    """Apply deterministic replacements. One is applied only if `before` occurs exactly once
    in the line at the recorded file:line (the current text, after earlier replacements);
    anything else is skipped with a reason. Returns (applied, skipped) where skipped holds
    (finding, reason). Line endings are kept."""
    applied, skipped = [], []
    by_file = {}
    for f in findings:
        by_file.setdefault(f.file, []).append(f)
    for path in sorted(by_file):
        full = os.path.join(root, path)
        try:
            with open(full, encoding="utf-8", newline="") as fh:
                lines = fh.read().splitlines(keepends=True)
        except (OSError, UnicodeDecodeError) as err:
            skipped.extend((f, f"cannot read the file ({type(err).__name__})") for f in by_file[path])
            continue
        changed = False
        for f in sorted(by_file[path]):
            if not 1 <= f.line <= len(lines):
                skipped.append((f, f"line {f.line} is past the end of the file"))
                continue
            text = lines[f.line - 1]
            count = text.count(f.before)
            if count != 1:
                skipped.append((f, "`before` not found on that line" if count == 0
                                else f"`before` occurs {count} times on that line"))
                continue
            lines[f.line - 1] = text.replace(f.before, f.after, 1)
            applied.append(f)
            changed = True
        if changed:
            with open(full, "w", encoding="utf-8", newline="") as fh:
                fh.write("".join(lines))
    return applied, skipped


def render_fix_body(applied, skipped):
    """The body of an `agent:deterministic` PR: what was replaced, with the per-finding dedup keys."""
    lines = ["Exact replacements found and applied by `scripts/ci/free_task_miner.py --fix`. "
             "**No model took part.** Each one was applied only where the text before it was found "
             "exactly once on the recorded line.", ""]
    lines += [render_item(f) for f in sorted(applied)]
    if skipped:
        lines += ["", "Skipped (the line no longer matches):"]
        lines += [f"- `{f.file}:{f.line}`: {why}" for f, why in skipped]
    lines += [""] + [f"<!-- free-task-item:{item_key(f)} -->" for f in sorted(applied)]
    lines.append(f"<!-- free-task:{fingerprint(applied)} -->")
    return "\n".join(lines) + "\n"


def write_fix_output(applied, fp):
    """The step outputs the workflow reads: `applied` always, `branch` when something changed."""
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as fh:
            fh.write(f"applied={applied}\n")
            if fp:
                fh.write(f"branch=agent-fix/{fp}\n")


def run_fix(args, markers, open_prs):
    """`--fix`: apply one group of deterministic findings to the working tree."""
    if open_prs >= MAX_OPEN_DETERMINISTIC:
        print(f"{open_prs} open {DETERMINISTIC_LABEL} PRs (max {MAX_OPEN_DETERMINISTIC}): nothing to fix now")
        write_fix_output(0, None)
        return 0
    files = list_files(args.root)
    found = detect_home_paths(args.root, files) + detect_broken_links(args.root, files)
    chosen = plan(sorted(set(found)), markers, 1)
    print(f"{len(found)} deterministic findings; {sum(1 for f in found if item_key(f) in markers)} already filed")
    applied, skipped = ([], [])
    if chosen:
        applied, skipped = apply_fixes(args.root, chosen[0])
    for f in applied:
        print(f"fixed {f.file}:{f.line}: {f.before} -> {f.after}")
    for f, why in skipped:
        print(f"skipped {f.file}:{f.line}: {why}")
    if args.out:
        os.makedirs(args.out, exist_ok=True)
        if applied:
            title = render_title(applied)
            with open(os.path.join(args.out, "title.txt"), "w", encoding="utf-8") as fh:
                fh.write(title + "\n")
            with open(os.path.join(args.out, "body.md"), "w", encoding="utf-8") as fh:
                fh.write(render_fix_body(applied, skipped))
    write_fix_output(len(applied), fingerprint(applied) if applied else None)
    print(f"{len(applied)} replacements applied, {len(skipped)} skipped")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="File mechanical docs fixes as before -> after issues.")
    ap.add_argument("--root", default=".")
    ap.add_argument("--max-issues", type=int, default=3, help="total across all kinds")
    ap.add_argument("--doc-crate", help="crate to mine for Rust docs (default: by day of year)")
    ap.add_argument("--no-rust-docs", action="store_true", help="skip the cargo-based detectors")
    ap.add_argument("--out", help="with --fix: directory for title.txt and body.md of the PR")
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="print what would be filed (default)")
    mode.add_argument("--create", action="store_true", help="file the issues (model kinds) with gh")
    mode.add_argument("--fix", action="store_true",
                      help="apply the exact home-path / broken-link replacements in the working tree")
    args = ap.parse_args(argv)

    if args.fix:
        det_prs = gh_open_deterministic_prs()
        markers = known_markers(gh_existing_issues()) | known_markers(det_prs)
        open_prs = len(det_prs)
        return run_fix(args, markers, open_prs)

    doc_crate = None
    if not args.no_rust_docs:
        doc_crate = args.doc_crate or pick_doc_crate(datetime.date.today().timetuple().tm_yday)
        print(f"Rust docs crate: {doc_crate}")
    findings = mine(args.root, doc_crate)
    print(f"{len(findings)} findings: "
          + ", ".join(f"{k} {sum(1 for f in findings if f.kind == k)}"
                      for k in sorted({f.kind for f in findings})))
    try:
        markers = known_markers(gh_existing_issues()) | known_markers(gh_open_deterministic_prs())
    except (OSError, subprocess.CalledProcessError):
        if args.create:
            raise
        markers = set()
        print("note: gh unavailable; not checking for existing issues", file=sys.stderr)
    # Findings with an exact before -> after are fixed by `--fix` (a PR, no model); only the
    # kinds that need writing become issues for a model.
    model_findings = [f for f in findings if f.kind in MAX_GROUP]
    chosen = plan(model_findings, markers, args.max_issues)
    filed = sum(1 for f in model_findings if item_key(f) in markers)
    print(f"{filed} findings already filed; {len(chosen)} issues to file (max {args.max_issues})")
    for group in chosen:
        title, body = render_title(group), render_body(group)
        if args.create:
            number = gh_create(title, body)
            print(f"created #{number}: {title}")
        else:
            print(f"--- would file: {title}\n{body}")
    if not args.create:
        fixable = [f for f in findings if f.kind not in MAX_GROUP and item_key(f) not in markers]
        print(f"{len(fixable)} deterministic findings the `--fix` mode would fix")
    return 0


if __name__ == "__main__":
    sys.exit(main())
