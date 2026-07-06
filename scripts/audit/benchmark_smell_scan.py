#!/usr/bin/env python3
"""Scan for benchmark-specific overfit smells in Fission code and docs."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable


BENCHMARK_FUNCTIONS = (
    "accumulate_pairs",
    "checksum",
    "classify_range",
    "clamp",
    "count_bits",
    "crc32",
    "rc4_crypt",
    "rc4_init",
    "saturating_add",
    "signum",
)

COMMON_CODE_IDENTIFIERS = {
    "clamp",
    "saturating_add",
}

CORPUS_TOKENS = (
    "benchmark/binary/",
    "source_semantic_benchmark",
    "fission-benchmark",
    "corpus/dev",
    "corpus/holdout",
    "canonical_rows.json",
    "smoke_corpus.json",
    "release_corpus.json",
    "parity_corpus.json",
)

FAST_SCAN_ROOTS = (
    "crates/fission-pcode/src/nir",
    "crates/fission-decompiler/src",
    "crates/fission-automation/src",
)

FULL_SCAN_ROOTS = (
    "crates",
    "scripts",
    "docs/templates",
    "docs/proposals",
    "AGENTS.md",
)

EXCLUDE_DIRS = {
    ".git",
    "target",
    "vendor",
    "__pycache__",
    "docs/changelog",
}

TEXT_EXTENSIONS = {
    ".rs",
    ".py",
    ".sh",
    ".md",
    ".toml",
    ".yml",
    ".yaml",
    ".json",
}


@dataclass(frozen=True)
class Finding:
    severity: str
    kind: str
    path: str
    line: int
    token: str
    detail: str


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return path.read_text(encoding="utf-8", errors="replace")


def iter_scan_files(root: Path, mode: str) -> Iterable[Path]:
    roots = FAST_SCAN_ROOTS if mode == "fast" else FULL_SCAN_ROOTS
    for rel in roots:
        base = root / rel
        if base.is_file():
            yield base
            continue
        if not base.exists():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file() or path.suffix not in TEXT_EXTENSIONS:
                continue
            rel_path = path.relative_to(root).as_posix()
            if rel_path.startswith("scripts/audit/"):
                continue
            if path.suffix == ".rs" and ("test" in path.stem or "/tests/" in rel_path):
                continue
            if any(part in EXCLUDE_DIRS for part in path.relative_to(root).parts):
                continue
            if rel_path.endswith(".pyc"):
                continue
            yield path


def strip_line_comment(line: str, suffix: str) -> str:
    if suffix == ".rs":
        stripped = line.strip()
        if stripped.startswith("//!") or stripped.startswith("///"):
            return ""
        if "//" in line:
            code, _, _ = line.partition("//")
            return code
    if suffix in {".py", ".sh"} and "#" in line:
        code, _, _ = line.partition("#")
        return code
    return line


def quoted_or_comment_context(raw_line: str, token: str, suffix: str) -> bool:
    token_index = raw_line.find(token)
    if token_index < 0:
        return False
    before = raw_line[:token_index]
    if suffix == ".rs" and "//" in before:
        return True
    if suffix in {".py", ".sh"} and "#" in before:
        return True
    return before.count('"') % 2 == 1 or before.count("'") % 2 == 1 or before.count("`") % 2 == 1


def rust_lines_without_test_modules(text: str) -> Iterable[tuple[int, str]]:
    pending_cfg_test = False
    in_test_mod = False
    depth = 0
    for lineno, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if in_test_mod:
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                in_test_mod = False
            continue
        if stripped.startswith("#[cfg(test)]"):
            pending_cfg_test = True
            continue
        if pending_cfg_test and re.match(r"(pub\s+)?mod\s+tests\s*\{", stripped):
            in_test_mod = True
            depth = line.count("{") - line.count("}")
            pending_cfg_test = False
            continue
        if pending_cfg_test and stripped and not stripped.startswith("#"):
            pending_cfg_test = False
        yield lineno, line


def scan_file(root: Path, path: Path, mode: str) -> list[Finding]:
    rel = path.relative_to(root).as_posix()
    text = read_text(path)
    lines = rust_lines_without_test_modules(text) if path.suffix == ".rs" else enumerate(text.splitlines(), start=1)
    findings: list[Finding] = []
    function_pattern = re.compile(r"\b(" + "|".join(map(re.escape, BENCHMARK_FUNCTIONS)) + r")\b")
    address_pattern = re.compile(r"\b0x14[0-9a-fA-F]{6,}\b")
    threshold_pattern = re.compile(r"\b(?:if|while)\s*\([^)]*(?:[<>]=?|==)\s*(\d{1,4})[^)]*\)")

    for lineno, raw_line in lines:
        line = strip_line_comment(raw_line, path.suffix)
        if not line.strip():
            continue
        if "benchmark_smell_allow" in raw_line:
            continue
        seen_tokens: set[tuple[str, str]] = set()
        for match in function_pattern.finditer(line):
            token = match.group(1)
            if token in COMMON_CODE_IDENTIFIERS and not quoted_or_comment_context(raw_line, token, path.suffix):
                continue
            key = ("benchmark_function_name", token)
            if key in seen_tokens:
                continue
            seen_tokens.add(key)
            findings.append(
                Finding(
                    "high",
                    "benchmark_function_name",
                    rel,
                    lineno,
                    token,
                    "Benchmark corpus function name appears in non-test code.",
                )
            )
        for match in address_pattern.finditer(line):
            key = ("benchmark_like_address", match.group(0))
            if key in seen_tokens:
                continue
            seen_tokens.add(key)
            findings.append(
                Finding(
                    "high",
                    "benchmark_like_address",
                    rel,
                    lineno,
                    match.group(0),
                    "PE-style benchmark address appears in scanned source.",
                )
            )
        for token in CORPUS_TOKENS:
            if token in line:
                key = ("corpus_path_or_manifest", token)
                if key in seen_tokens:
                    continue
                seen_tokens.add(key)
                findings.append(
                    Finding(
                        "high" if mode == "fast" else "medium",
                        "corpus_path_or_manifest",
                        rel,
                        lineno,
                        token,
                        "Benchmark corpus path or manifest token appears in scanned source.",
                    )
                )
        for match in threshold_pattern.finditer(line):
            if "MAX_" in line or "const " in line or "static " in line:
                continue
            findings.append(
                Finding(
                    "warning",
                    "unexplained_numeric_threshold",
                    rel,
                    lineno,
                    match.group(1),
                    "Numeric threshold in a branch may need invariant documentation.",
                )
            )
    return findings


def recent_heuristic_commits(root: Path, months: int) -> list[dict[str, str]]:
    try:
        proc = subprocess.run(
            [
                "git",
                "log",
                f"--since={months}.months",
                "--regexp-ignore-case",
                "--grep=heuristic",
                "--grep=threshold",
                "--grep=workaround",
                "--grep=overfit",
                "--format=%H%x09%cs%x09%s",
            ],
            cwd=root,
            text=True,
            capture_output=True,
            check=False,
        )
    except OSError:
        return []
    rows = []
    for line in proc.stdout.splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3:
            rows.append({"commit": parts[0], "date": parts[1], "subject": parts[2]})
    return rows


def render_markdown(findings: list[Finding], commits: list[dict[str, str]], root: Path) -> str:
    lines = [
        "# Benchmark Smell Scan",
        "",
        f"- Repo: `{root}`",
        f"- Findings: `{len(findings)}`",
        "",
        "## Findings",
        "",
    ]
    if findings:
        lines += ["| Severity | Kind | Location | Token | Detail |", "|---|---|---|---|---|"]
        for finding in findings:
            location = f"{finding.path}:{finding.line}"
            token = finding.token.replace("|", "\\|")
            lines.append(
                f"| {finding.severity} | `{finding.kind}` | `{location}` | `{token}` | {finding.detail} |"
            )
    else:
        lines.append("No benchmark identity smells found in the selected scan scope.")

    lines += ["", "## Recent Heuristic-Labeled Commits", ""]
    if commits:
        lines += ["| Date | Commit | Subject |", "|---|---|---|"]
        for row in commits:
            lines.append(f"| {row['date']} | `{row['commit'][:12]}` | {row['subject']} |")
    else:
        lines.append("No recent heuristic/threshold/workaround commits matched the audit query.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--mode", choices=["fast", "full"], default="fast")
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--recent-months", type=int, default=6)
    parser.add_argument("--fail-on", choices=["never", "high"], default="never")
    args = parser.parse_args()

    root = args.root.resolve()
    findings: list[Finding] = []
    for path in iter_scan_files(root, args.mode):
        findings.extend(scan_file(root, path, args.mode))
    commits = recent_heuristic_commits(root, args.recent_months)

    if args.format == "json":
        rendered = json.dumps(
            {
                "root": str(root),
                "mode": args.mode,
                "findings": [asdict(finding) for finding in findings],
                "recent_heuristic_commits": commits,
            },
            indent=2,
            sort_keys=True,
        )
    elif args.format == "markdown":
        rendered = render_markdown(findings, commits, root)
    else:
        rendered = render_markdown(findings, commits, root)

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")

    if args.fail_on == "high" and any(f.severity == "high" for f in findings):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
