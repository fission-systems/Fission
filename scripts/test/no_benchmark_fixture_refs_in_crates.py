#!/usr/bin/env python3
"""Fail if crate unit-test sources reference benchmark corpus paths or manifests."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


FORBIDDEN = (
    "benchmark/binary/",
    "samples/windows/",
    "test_functions.exe",
    "canonical_rows.json",
    "smoke_corpus.json",
    "release_corpus.json",
    "parity_corpus.json",
)


def strip_line_comment(line: str) -> str | None:
    stripped = line.strip()
    if stripped.startswith("//!") or stripped.startswith("///"):
        return None
    if "//" in line:
        code, _, _rest = line.partition("//")
        return None if code.strip() == "" else code.rstrip("\n")
    return line.rstrip("\n")


def scan_file(path: Path) -> list[tuple[str, int]]:
    hits: list[tuple[str, int]] = []
    try:
        body = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise SystemExit(f"cannot read {path}: {exc}") from exc
    for lineno, line in enumerate(body.splitlines(), start=1):
        kept = strip_line_comment(line)
        if kept is None or not kept.strip():
            continue
        for needle in FORBIDDEN:
            if needle in kept:
                hits.append((needle, lineno))
                break
    return hits


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root",
    )
    args = parser.parse_args()
    root: Path = args.root
    crates_src = root / "crates"
    if not crates_src.is_dir():
        print(f"error: missing {crates_src}", file=sys.stderr)
        sys.exit(2)

    failures: list[str] = []
    for path in sorted(crates_src.glob("**/src/**/*.rs")):
        if not path.is_file():
            continue
        for needle, lineno in scan_file(path):
            rel = path.relative_to(root)
            failures.append(f"{rel}:{lineno}: forbidden substring {needle!r}")

    if failures:
        print(
            "crate sources must not reference benchmark fixtures or corpus manifests:\n",
            file=sys.stderr,
        )
        for line in failures:
            print(line, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
