#!/usr/bin/env python3
"""Helpers for the toolchain canary (.github/workflows/toolchain-canary.yml).

A new rustc or LLVM can fail to build the workspace, crash (an internal
compiler error, ICE), or compile it to a binary that decompiles differently.
Four subcommands, each printing Markdown for the job summary:

  ice LOG...          exit 1 if a log shows the compiler itself failing
  clippy-count LOG    count warnings and errors in clippy output (never fails)
  hash CLI OUT FIX... decompile fixtures with CLI twice, store per-function hashes
  compare A.json B.json
                      exit 1 if two hash files differ on a function both are
                      stable on

Only in-repo fixtures are decompiled; nothing is executed.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# What rustc and its LLVM print when the compiler, not the code, is at fault.
ICE_MARKERS = (
    "error: the compiler unexpectedly panicked",
    "internal compiler error",
    "error: rustc interrupted by SIG",
    "LLVM ERROR",
    "(signal: 11, SIGSEGV",
    "(signal: 6, SIGABRT",
)
# cargo's trailing "generated N warnings" and "aborting due to" lines repeat
# diagnostics that were already counted.
_DIAGNOSTIC = re.compile(r"^(warning|error)(\[[A-Za-z0-9_:]+\])?: ")
_SUMMARY_LINE = re.compile(r"generated \d+ warnings?|aborting due to|could not compile|warning emitted")


def find_ice(text: str) -> list[str]:
    """Lines of `text` that show a compiler crash, in order."""
    return [line.strip() for line in text.splitlines() if any(m in line for m in ICE_MARKERS)]


def count_diagnostics(text: str) -> dict[str, int]:
    counts = {"warning": 0, "error": 0}
    for line in text.splitlines():
        match = _DIAGNOSTIC.match(line)
        if match and not _SUMMARY_LINE.search(line):
            counts[match.group(1)] += 1
    return counts


def stable_hashes(first: dict[str, str], second: dict[str, str]) -> tuple[dict[str, str], list[str]]:
    """Hashes both runs agree on, and the addresses where they do not."""
    unstable = sorted(a for a in first.keys() | second.keys() if first.get(a) != second.get(a))
    return {a: h for a, h in first.items() if a not in unstable}, unstable


def diff_hashes(base: dict[str, dict[str, str]], head: dict[str, dict[str, str]]) -> list[str]:
    """`fixture address: how it differs`, for every function both sides are stable on."""
    problems = []
    for fixture in sorted(base.keys() | head.keys()):
        left, right = base.get(fixture), head.get(fixture)
        if left is None or right is None:
            problems.append(f"{fixture}: decompiled on only one side")
            continue
        for address in sorted(left.keys() | right.keys()):
            if address not in left or address not in right:
                problems.append(f"{fixture} {address}: present on only one side")
            elif left[address] != right[address]:
                problems.append(f"{fixture} {address}: output text differs")
    return problems


def cmd_ice(args: argparse.Namespace) -> int:
    hits = {}
    for log in args.logs:
        path = Path(log)
        if path.is_file():
            found = find_ice(path.read_text(errors="replace"))
            if found:
                hits[path.name] = found
    if not hits:
        print("No internal compiler error in the logs.")
        return 0
    print("### Internal compiler error (rustc bug, report upstream)\n")
    for name, lines in hits.items():
        print(f"- `{name}`: {lines[0]}")
    return 1


def cmd_clippy_count(args: argparse.Namespace) -> int:
    counts = count_diagnostics(Path(args.log).read_text(errors="replace"))
    print(f"Clippy on this toolchain: {counts['warning']} warnings, {counts['error']} errors (report-only).")
    return 0


def cmd_hash(args: argparse.Namespace) -> int:
    import decompiler_smoke  # CLI contract helpers; lazy so the other commands need no CLI

    result: dict[str, dict[str, str]] = {}
    unstable_total = 0
    for fixture in args.fixtures:
        path = Path(fixture)
        # One worker and two runs: a function that is not stable on this side
        # alone cannot say anything about the compiler.
        first = decompiler_smoke.decompile_codes(Path(args.cli), path, 1, timeout=args.timeout)
        second = decompiler_smoke.decompile_codes(Path(args.cli), path, 1, timeout=args.timeout)
        stable, unstable = stable_hashes(first, second)
        unstable_total += len(unstable)
        result[path.name] = stable
        print(f"{path.name}: {len(stable)} functions hashed, {len(unstable)} unstable across two runs")
    Path(args.out).write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
    print(f"\nTotal functions: {sum(len(v) for v in result.values())}, unstable and skipped: {unstable_total}")
    return 0


def cmd_compare(args: argparse.Namespace) -> int:
    base = json.loads(Path(args.base).read_text())
    head = json.loads(Path(args.head).read_text())
    problems = diff_hashes(base, head)
    total = sum(len(v) for v in base.values())
    if not problems:
        print(f"Stable and canary CLIs decompile {total} functions identically.")
        return 0
    print(f"### The canary toolchain changed decompiler output ({len(problems)} of {total} functions)\n")
    for line in problems[:50]:
        print(f"- {line}")
    if len(problems) > 50:
        print(f"- ... and {len(problems) - 50} more")
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("ice")
    p.add_argument("logs", nargs="+")
    p.set_defaults(func=cmd_ice)
    p = sub.add_parser("clippy-count")
    p.add_argument("log")
    p.set_defaults(func=cmd_clippy_count)
    p = sub.add_parser("hash")
    p.add_argument("cli")
    p.add_argument("out")
    p.add_argument("fixtures", nargs="+")
    p.add_argument("--timeout", type=int, default=300)
    p.set_defaults(func=cmd_hash)
    p = sub.add_parser("compare")
    p.add_argument("base")
    p.add_argument("head")
    p.set_defaults(func=cmd_compare)
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
