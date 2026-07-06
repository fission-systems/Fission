#!/usr/bin/env python3
"""Flag metric movement that may indicate Goodhart-style metric gaming."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


CORRECTNESS_KEYS = ("correctness_score", "semantic_score", "semantic", "correctness")
READABILITY_KEYS = ("readability_proxy_score", "generic_naming_ratio", "gnr", "type_specificity")


@dataclass(frozen=True)
class MetricPoint:
    source: str
    correctness: float | None
    readability: float | None


@dataclass(frozen=True)
class Flag:
    before: str
    after: str
    correctness_delta: float | None
    readability_delta: float | None
    reason: str


def numeric_value(data: dict[str, Any], keys: tuple[str, ...]) -> float | None:
    for key in keys:
        value = data.get(key)
        if isinstance(value, (int, float)):
            return float(value)
    return None


def walk_dicts(data: Any) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if isinstance(data, dict):
        rows.append(data)
        for value in data.values():
            rows.extend(walk_dicts(value))
    elif isinstance(data, list):
        for value in data:
            rows.extend(walk_dicts(value))
    return rows


def extract_point(path: Path) -> MetricPoint | None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None
    correctness_values = []
    readability_values = []
    for row in walk_dicts(data):
        correctness = numeric_value(row, CORRECTNESS_KEYS)
        readability = numeric_value(row, READABILITY_KEYS)
        if correctness is not None:
            correctness_values.append(correctness)
        if readability is not None:
            readability_values.append(readability)
    if not correctness_values and not readability_values:
        return None
    correctness = sum(correctness_values) / len(correctness_values) if correctness_values else None
    readability = sum(readability_values) / len(readability_values) if readability_values else None
    return MetricPoint(path.as_posix(), correctness, readability)


def scan_history(history: Path, readability_jump: float) -> tuple[list[MetricPoint], list[Flag]]:
    paths = [history] if history.is_file() else sorted(history.rglob("*.json"))
    points = [point for path in paths if (point := extract_point(path)) is not None]
    flags: list[Flag] = []
    for before, after in zip(points, points[1:]):
        if before.readability is None or after.readability is None:
            continue
        readability_delta = after.readability - before.readability
        correctness_delta = None
        if before.correctness is not None and after.correctness is not None:
            correctness_delta = after.correctness - before.correctness
        if readability_delta >= readability_jump and (correctness_delta is None or correctness_delta <= 0):
            flags.append(
                Flag(
                    before.source,
                    after.source,
                    correctness_delta,
                    readability_delta,
                    "Readability/proxy metric jumped while correctness did not improve.",
                )
            )
    return points, flags


def render_markdown(history: Path, points: list[MetricPoint], flags: list[Flag]) -> str:
    lines = [
        "# Metric Gaming Scan",
        "",
        f"- History: `{history}`",
        f"- Metric points: `{len(points)}`",
        f"- Flags: `{len(flags)}`",
        "",
    ]
    if flags:
        lines += ["| Before | After | Correctness delta | Readability delta | Reason |", "|---|---|---|---|---|"]
        for flag in flags:
            lines.append(
                f"| `{flag.before}` | `{flag.after}` | `{flag.correctness_delta}` | `{flag.readability_delta}` | {flag.reason} |"
            )
    else:
        lines.append("No Goodhart-style metric jumps found in the supplied history.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", type=Path, required=True)
    parser.add_argument("--readability-jump", type=float, default=0.15)
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fail-on-flag", action="store_true")
    args = parser.parse_args()

    if not args.history.exists():
        print(f"metric_gaming_scan: history path not found: {args.history}", file=sys.stderr)
        return 0
    points, flags = scan_history(args.history, args.readability_jump)
    if args.format == "json":
        rendered = json.dumps(
            {
                "history": str(args.history),
                "points": [point.__dict__ for point in points],
                "flags": [flag.__dict__ for flag in flags],
            },
            indent=2,
            sort_keys=True,
        )
    else:
        rendered = render_markdown(args.history, points, flags)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 1 if args.fail_on_flag and flags else 0


if __name__ == "__main__":
    sys.exit(main())
