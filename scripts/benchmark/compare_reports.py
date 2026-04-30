#!/usr/bin/env python3
"""Compare benchmark/loader reports and emit JSON plus Markdown deltas."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_json(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"report is not a JSON object: {path}")
    return payload


def get_path(payload: dict[str, Any], dotted: str) -> Any:
    cur: Any = payload
    for part in dotted.split("."):
        if not isinstance(cur, dict):
            return None
        cur = cur.get(part)
    return cur


def numeric_delta(before: Any, after: Any) -> dict[str, Any]:
    if isinstance(before, (int, float)) and isinstance(after, (int, float)):
        return {"before": before, "after": after, "delta": after - before}
    return {"before": before, "after": after, "delta": None}


def counter_delta(before: Any, after: Any) -> dict[str, dict[str, Any]]:
    lhs = before if isinstance(before, dict) else {}
    rhs = after if isinstance(after, dict) else {}
    keys = sorted(set(lhs) | set(rhs))
    return {key: numeric_delta(lhs.get(key, 0), rhs.get(key, 0)) for key in keys}


def infer_kind(payload: dict[str, Any]) -> str:
    if "loaded" in payload and "load_failed" in payload:
        return "loader_smoke"
    if "bucket_totals" in payload or "average_similarity_score" in payload:
        return "raw_pcode"
    if "summary" in payload and "binaries" in payload:
        return "full_benchmark"
    if "steps" in payload:
        return "suite"
    return "generic"


def compare_reports(baseline: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    kind = infer_kind(current)
    common_metrics = [
        "row_count",
        "loaded",
        "load_failed",
        "full_match",
        "average_similarity_score",
        "average_parity_ratio",
        "compat_emitter_used",
        "fake_placeholder_op",
        "invalid_pcode_shape",
        "function_count_total",
    ]
    deltas = {
        key: numeric_delta(get_path(baseline, key), get_path(current, key))
        for key in common_metrics
        if get_path(baseline, key) is not None or get_path(current, key) is not None
    }
    counters = {
        "status_counts": counter_delta(baseline.get("status_counts"), current.get("status_counts")),
        "failure_bucket_counts": counter_delta(baseline.get("failure_bucket_counts"), current.get("failure_bucket_counts")),
        "bucket_totals": counter_delta(baseline.get("bucket_totals"), current.get("bucket_totals")),
        "format_counts": counter_delta(baseline.get("format_counts"), current.get("format_counts")),
        "arch_counts": counter_delta(baseline.get("arch_counts"), current.get("arch_counts")),
    }
    return {"kind": kind, "metric_deltas": deltas, "counter_deltas": counters}


def render_markdown(report: dict[str, Any], baseline: Path, current: Path) -> str:
    lines = [
        "# Benchmark Report Diff",
        "",
        f"- Baseline: `{baseline}`",
        f"- Current: `{current}`",
        f"- Kind: `{report['kind']}`",
        "",
        "## Metrics",
        "",
        "| Metric | Before | After | Delta |",
        "|---|---:|---:|---:|",
    ]
    for key, row in sorted(report["metric_deltas"].items()):
        lines.append(f"| `{key}` | {row['before']} | {row['after']} | {row['delta']} |")
    lines.extend(["", "## Counters", ""])
    for counter_name, rows in sorted(report["counter_deltas"].items()):
        nonzero = {key: row for key, row in rows.items() if row["before"] or row["after"]}
        if not nonzero:
            continue
        lines.extend([f"### `{counter_name}`", "", "| Bucket | Before | After | Delta |", "|---|---:|---:|---:|"])
        for key, row in sorted(nonzero.items()):
            lines.append(f"| `{key}` | {row['before']} | {row['after']} | {row['delta']} |")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--current", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    baseline = load_json(args.baseline)
    current = load_json(args.current)
    report = compare_reports(baseline, current)
    json_path = args.output_dir / "report_diff.json"
    md_path = args.output_dir / "report_diff.md"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    md_path.write_text(render_markdown(report, args.baseline, args.current), encoding="utf-8")
    print(json.dumps({"json": str(json_path), "markdown": str(md_path)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
