#!/usr/bin/env python3
"""Detect benchmark identity leaks in AI prompt surfaces."""

from __future__ import annotations

import argparse
import json
import re
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

PROMPT_SURFACES = ("docs/templates",)

OPTIONAL_PROPOSAL_SURFACES = ("docs/proposals",)
OPTIONAL_AGENT_SURFACES = ("AGENTS.md",)
OPTIONAL_GITHUB_SURFACES = (
    ".github/prompts",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/pull_request_template.md",
)


@dataclass(frozen=True)
class Leak:
    severity: str
    kind: str
    path: str
    line: int
    token: str


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return path.read_text(encoding="utf-8", errors="replace")


def iter_files(root: Path, include_proposals: bool, include_agents: bool, include_github: bool) -> Iterable[Path]:
    surfaces = list(PROMPT_SURFACES)
    if include_proposals:
        surfaces.extend(OPTIONAL_PROPOSAL_SURFACES)
    if include_agents:
        surfaces.extend(OPTIONAL_AGENT_SURFACES)
    if include_github:
        surfaces.extend(OPTIONAL_GITHUB_SURFACES)
    for rel in surfaces:
        base = root / rel
        if base.is_file():
            yield base
            continue
        if not base.exists():
            continue
        for path in sorted(base.rglob("*")):
            if path.is_file() and path.suffix in {".md", ".txt", ".yml", ".yaml"}:
                yield path


def scan_file(root: Path, path: Path) -> list[Leak]:
    rel = path.relative_to(root).as_posix()
    text = read_text(path)
    function_pattern = re.compile(r"\b(" + "|".join(map(re.escape, BENCHMARK_FUNCTIONS)) + r")\b")
    address_pattern = re.compile(r"\b0x14[0-9a-fA-F]{6,}\b")
    path_pattern = re.compile(
        r"(?:benchmark/binary/|source_semantic_benchmark|fission-benchmark|corpus/(?:dev|holdout))"
    )
    compiler_tuple_pattern = re.compile(r"\b(?:gcc|clang|msvc|mingw)[-_]?(?:m32|x86_64|x64)?[_-]O[0-3s]\b", re.I)
    leaks: list[Leak] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        if "ai_prompt_leak_allow" in line:
            continue
        if "BENCHMARK_FUNCTIONS" in line:
            continue
        for pattern, kind in (
            (function_pattern, "benchmark_function_name"),
            (address_pattern, "benchmark_like_address"),
            (path_pattern, "corpus_path"),
            (compiler_tuple_pattern, "compiler_tuple"),
        ):
            for match in pattern.finditer(line):
                leaks.append(Leak("high", kind, rel, lineno, match.group(0)))
    return leaks


def render_markdown(root: Path, leaks: list[Leak]) -> str:
    lines = [
        "# AI Prompt Leak Scan",
        "",
        f"- Repo: `{root}`",
        f"- Leaks: `{len(leaks)}`",
        "",
    ]
    if leaks:
        lines += ["| Severity | Kind | Location | Token |", "|---|---|---|---|"]
        for leak in leaks:
            lines.append(f"| {leak.severity} | `{leak.kind}` | `{leak.path}:{leak.line}` | `{leak.token}` |")
    else:
        lines.append("No benchmark identity leaks found in prompt surfaces.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--include-proposals", action="store_true")
    parser.add_argument("--include-agents", action="store_true")
    parser.add_argument("--include-github", action="store_true")
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()

    root = args.root.resolve()
    leaks: list[Leak] = []
    for path in iter_files(root, args.include_proposals, args.include_agents, args.include_github):
        leaks.extend(scan_file(root, path))

    if args.format == "json":
        rendered = json.dumps({"root": str(root), "leaks": [asdict(leak) for leak in leaks]}, indent=2, sort_keys=True)
    else:
        rendered = render_markdown(root, leaks)

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")

    return 0 if args.report_only or not leaks else 1


if __name__ == "__main__":
    sys.exit(main())
