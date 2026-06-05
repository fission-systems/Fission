#!/usr/bin/env python3
"""Compare Ghidra and Fission address-keyed CFG snapshots."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_snapshot(payload: dict[str, Any]) -> dict[str, Any]:
    if "snapshot" in payload:
        return payload["snapshot"]
    return payload


def edge_set(snapshot: dict[str, Any]) -> set[tuple[int, int]]:
    edges: set[tuple[int, int]] = set()
    for edge in snapshot.get("edges", []):
        edges.add((int(edge["from"]), int(edge["to"])))
    return edges


def compare_snapshots(ghidra: dict[str, Any], fission: dict[str, Any]) -> dict[str, Any]:
    g = load_snapshot(ghidra)
    f = load_snapshot(fission)

    g_blocks = {int(value) for value in g.get("block_starts", [])}
    f_blocks = {int(value) for value in f.get("block_starts", [])}
    g_edges = edge_set(g)
    f_edges = edge_set(f)
    g_exits = {int(value) for value in g.get("exit_blocks", [])}
    f_exits = {int(value) for value in f.get("exit_blocks", [])}

    missing_blocks = sorted(g_blocks - f_blocks)
    extra_blocks = sorted(f_blocks - g_blocks)
    missing_edges = sorted(g_edges - f_edges)
    extra_edges = sorted(f_edges - g_edges)
    missing_exit_blocks = sorted(g_exits - f_exits)
    extra_exit_blocks = sorted(f_exits - g_exits)

    buckets: list[str] = []
    if (
        not missing_blocks
        and not extra_blocks
        and not missing_edges
        and not extra_edges
        and not missing_exit_blocks
        and not extra_exit_blocks
    ):
        buckets.append("full_match")
    else:
        if missing_blocks or extra_blocks:
            buckets.append("block_set_mismatch")
        if missing_edges or extra_edges:
            buckets.append("edge_set_mismatch")
        if missing_exit_blocks or extra_exit_blocks:
            buckets.append("exit_set_mismatch")

    return {
        "ghidra_model": g.get("model"),
        "fission_model": f.get("model"),
        "ghidra_function_address": g.get("function_address"),
        "fission_function_address": f.get("function_address"),
        "ghidra_block_count": len(g_blocks),
        "fission_block_count": len(f_blocks),
        "ghidra_edge_count": len(g_edges),
        "fission_edge_count": len(f_edges),
        "buckets": buckets,
        "missing_blocks": [hex(value) for value in missing_blocks],
        "extra_blocks": [hex(value) for value in extra_blocks],
        "missing_edges": [{"from": hex(src), "to": hex(dst)} for src, dst in missing_edges],
        "extra_edges": [{"from": hex(src), "to": hex(dst)} for src, dst in extra_edges],
        "missing_exit_blocks": [hex(value) for value in missing_exit_blocks],
        "extra_exit_blocks": [hex(value) for value in extra_exit_blocks],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ghidra", required=True, type=Path)
    parser.add_argument("--fission", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    ghidra = json.loads(args.ghidra.read_text())
    fission = json.loads(args.fission.read_text())
    report = compare_snapshots(ghidra, fission)
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)
    return 0 if report["buckets"] == ["full_match"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
