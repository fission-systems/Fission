#!/usr/bin/env python3
"""Offline Ghidra vs Fission latency / timeout posture summary."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
FULL_BENCHMARK = ROOT / "benchmark" / "full_benchmark"
sys.path.insert(0, str(FULL_BENCHMARK))

from grand_finale_support.metrics import normalize_address  # noqa: E402


def percentile(sorted_vals: list[float], q: float) -> float:
    if not sorted_vals:
        return 0.0
    idx = min(max(int(round(q * (len(sorted_vals) - 1))), 0), len(sorted_vals) - 1)
    return sorted_vals[idx]


def load_oracle_rows(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    rows = payload.get("rows") or []
    if not isinstance(rows, list):
        raise ValueError("oracle JSON rows must be a list")
    return rows


def load_fission_map(path: Path) -> dict[str, dict[str, Any]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    out: dict[str, dict[str, Any]] = {}

    if isinstance(data.get("entries"), dict):
        for k, v in data["entries"].items():
            if isinstance(v, dict):
                out[normalize_address(str(k))] = v
        return out

    funcs = data.get("functions")
    if isinstance(funcs, list):
        for item in funcs:
            if not isinstance(item, dict):
                continue
            addr = item.get("address")
            if addr is None:
                continue
            out[normalize_address(str(addr))] = item
        return out

    pairwise = (
        data.get("pairwise", {})
        .get("pyghidra_vs_fission", {})
        .get("comparisons")
    )
    if isinstance(pairwise, list):
        for item in pairwise:
            if not isinstance(item, dict):
                continue
            addr = item.get("address") or item.get("seed_address")
            if addr is None:
                continue
            out[normalize_address(str(addr))] = item
        return out

    raise ValueError(
        "unrecognized fission JSON shape (expected entries{}, functions[], or pairwise comparisons)"
    )


def ghidra_slow_or_failed(oracle_cell: dict[str, Any], threshold: float) -> bool:
    g = oracle_cell.get("ghidra") or {}
    if not g.get("decompile_success", False):
        return True
    sec = float(g.get("decompile_sec") or 0.0)
    return sec > threshold


def fission_slow_or_failed(fcell: dict[str, Any], threshold: float) -> bool:
    if not bool(fcell.get("success", False)):
        return True
    sec = float(fcell.get("wall_sec") or fcell.get("decomp_sec") or 0.0)
    return sec > threshold


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--oracle", type=Path, required=True, help="export_oracle.py output JSON")
    p.add_argument("--fission", type=Path, required=True, help="Fission-side JSON with per-address facts")
    p.add_argument(
        "--thresholds-sec",
        type=str,
        default="1,5,30,180",
        help="comma-separated soft latency thresholds for bucket counts",
    )
    p.add_argument("--out", type=Path, required=True)
    return p.parse_args()


def main() -> int:
    args = parse_args()
    thresholds = [float(x.strip()) for x in args.thresholds_sec.split(",") if x.strip()]

    oracle_rows = load_oracle_rows(args.oracle)
    fission_map = load_fission_map(args.fission)

    ghidra_secs_ok: list[float] = []
    fission_secs_ok: list[float] = []

    joined = 0
    ghidra_ok_fission_fail = 0
    ghidra_fail_fission_ok = 0
    both_ok = 0
    both_fail = 0

    bucket_oracle: dict[float, int] = {t: 0 for t in thresholds}
    bucket_fission: dict[float, int] = {t: 0 for t in thresholds}
    bucket_both_slow: dict[float, int] = {t: 0 for t in thresholds}

    snapshot_rows = 0

    for row in oracle_rows:
        addr_raw = row.get("address")
        if addr_raw is None or str(addr_raw).strip() == "":
            snapshot_rows += 1
            continue
        key = normalize_address(str(addr_raw))
        gcell = row.get("ghidra") or {}
        fcell = fission_map.get(key)
        if fcell is None:
            continue
        joined += 1

        g_ok = bool(gcell.get("decompile_success", False))
        f_ok = bool(fcell.get("success", False))
        if g_ok and not f_ok:
            ghidra_ok_fission_fail += 1
        elif not g_ok and f_ok:
            ghidra_fail_fission_ok += 1
        elif g_ok and f_ok:
            both_ok += 1
            ghidra_secs_ok.append(float(gcell.get("decompile_sec") or 0.0))
            fission_secs_ok.append(float(fcell.get("wall_sec") or fcell.get("decomp_sec") or 0.0))
        else:
            both_fail += 1

        for t in thresholds:
            if ghidra_slow_or_failed({"ghidra": gcell}, t):
                bucket_oracle[t] += 1
            if fission_slow_or_failed(fcell, t):
                bucket_fission[t] += 1
            if ghidra_slow_or_failed({"ghidra": gcell}, t) and fission_slow_or_failed(fcell, t):
                bucket_both_slow[t] += 1

    def latency_summary(samples: list[float]) -> dict[str, float]:
        if not samples:
            return {"count": 0, "p50": 0.0, "p95": 0.0, "p99": 0.0}
        s = sorted(samples)
        return {
            "count": len(s),
            "p50": round(percentile(s, 0.50), 6),
            "p95": round(percentile(s, 0.95), 6),
            "p99": round(percentile(s, 0.99), 6),
            "mean": round(statistics.fmean(s), 6),
        }

    summary = {
        "_meta": {
            "tool": "summarize_timeouts",
            "oracle_path": str(args.oracle),
            "fission_path": str(args.fission),
            "thresholds_sec": thresholds,
        },
        "counts": {
            "oracle_rows_total": len(oracle_rows),
            "oracle_snapshot_rows_skipped_for_join": snapshot_rows,
            "joined_addresses": joined,
            "cross": {
                "ghidra_ok_fission_fail": ghidra_ok_fission_fail,
                "ghidra_fail_fission_ok": ghidra_fail_fission_ok,
                "both_ok": both_ok,
                "both_fail": both_fail,
            },
            "soft_timeout_buckets_oracle": {str(k): v for k, v in bucket_oracle.items()},
            "soft_timeout_buckets_fission": {str(k): v for k, v in bucket_fission.items()},
            "soft_timeout_buckets_both_slow": {str(k): v for k, v in bucket_both_slow.items()},
        },
        "latency_sec": {
            "ghidra_decompile_success_only": latency_summary(ghidra_secs_ok),
            "fission_wall_or_decomp_success_only": latency_summary(fission_secs_ok),
        },
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
