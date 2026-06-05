#!/usr/bin/env python3
"""Compare Ghidra and Fission CFG input fact slices."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_function_facts(payload: dict[str, Any]) -> dict[str, Any]:
    if "facts" in payload:
        return payload["facts"]
    if "snapshot" in payload and "facts" in payload["snapshot"]:
        return payload["snapshot"]["facts"]
    return payload


def edge_set(edges: list[Any]) -> set[tuple[int, int]]:
    result: set[tuple[int, int]] = set()
    for edge in edges:
        if isinstance(edge, dict):
            result.add((int(edge["from"]), int(edge["to"])))
        elif isinstance(edge, (list, tuple)) and len(edge) == 2:
            result.add((int(edge[0]), int(edge[1])))
    return result


def compare_facts(ghidra: dict[str, Any], fission: dict[str, Any]) -> dict[str, Any]:
    g = load_function_facts(ghidra)
    f = load_function_facts(fission)

    g_labels = {int(value) for value in g.get("labels", [])}
    f_labels = {int(value) for value in f.get("labels", [])}
    g_edges = edge_set(g.get("flow_edges", []))
    f_edges = edge_set(f.get("flow_edges", []))

    g_noreturn = bool(g.get("has_no_return", False))
    f_noreturn_sites = {int(value) for value in f.get("noreturn_callsites", [])}
    f_noreturn = bool(f.get("has_no_return", False)) or bool(f_noreturn_sites)

    label_intersection = g_labels & f_labels
    edge_intersection = g_edges & f_edges

    label_recall = len(label_intersection) / len(g_labels) if g_labels else 1.0
    flow_edge_recall = len(edge_intersection) / len(g_edges) if g_edges else 1.0
    flow_edge_precision = len(edge_intersection) / len(f_edges) if f_edges else 1.0
    noreturn_match = g_noreturn == f_noreturn

    missing_labels = sorted(g_labels - f_labels)
    missing_edges = sorted(g_edges - f_edges)
    extra_edges = sorted(f_edges - g_edges)

    return {
        "ghidra_function_address": g.get("function_address"),
        "fission_function_address": f.get("function_address"),
        "ghidra_label_count": len(g_labels),
        "fission_label_count": len(f_labels),
        "ghidra_flow_edge_count": len(g_edges),
        "fission_flow_edge_count": len(f_edges),
        "label_recall": label_recall,
        "flow_edge_recall": flow_edge_recall,
        "flow_edge_precision": flow_edge_precision,
        "noreturn_match": noreturn_match,
        "ghidra_has_no_return": g_noreturn,
        "fission_has_no_return": f_noreturn,
        "missing_labels": [hex(value) for value in missing_labels],
        "missing_flow_edges": [
            {"from": hex(src), "to": hex(dst)} for src, dst in missing_edges
        ],
        "extra_flow_edges": [{"from": hex(src), "to": hex(dst)} for src, dst in extra_edges],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ghidra", required=True, type=Path)
    parser.add_argument("--fission", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    ghidra = json.loads(args.ghidra.read_text())
    fission = json.loads(args.fission.read_text())
    report = compare_facts(ghidra, fission)
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)

    ok = report["label_recall"] >= 1.0 and report["flow_edge_recall"] >= 1.0
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
