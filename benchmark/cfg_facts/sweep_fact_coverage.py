#!/usr/bin/env python3
"""Sweep all loader-known functions and compare Ghidra vs Fission fact coverage."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any

from compare_facts import compare_facts


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def run(cmd: list[str]) -> dict[str, Any]:
    proc = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise SystemExit(
            f"command failed: {' '.join(cmd)}\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )
    return json.loads(proc.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "benchmark/artifacts/cfg_facts/sweep",
    )
    parser.add_argument("--fission-release", action="store_true")
    args = parser.parse_args()

    binary = args.binary if args.binary.is_absolute() else ROOT / args.binary
    args.output_dir.mkdir(parents=True, exist_ok=True)

    ghidra_json = args.output_dir / "ghidra_facts_all.json"
    fission_json = args.output_dir / "fission_facts_all.json"

    run(
        [
            "python3",
            str(THIS_DIR / "ghidra_facts.py"),
            "--binary",
            str(binary),
            "--all-functions",
            "--language",
            args.language,
            "--compiler",
            args.compiler,
            *(["--ghidra-dir", str(args.ghidra_dir)] if args.ghidra_dir is not None else []),
            "--output",
            str(ghidra_json),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "fission_facts.py"),
            "--binary",
            str(binary),
            "--all-functions",
            *(["--release"] if args.fission_release else []),
            "--output",
            str(fission_json),
        ]
    )

    ghidra = json.loads(ghidra_json.read_text())
    fission = json.loads(fission_json.read_text())

    rows = []
    for key, g_entry in sorted(ghidra.get("functions", {}).items()):
        f_entry = fission.get("functions", {}).get(key)
        if f_entry is None:
            continue
        report = compare_facts(
            {"facts": g_entry["facts"]},
            {"facts": f_entry["facts"]},
        )
        rows.append(
            {
                "function_address": key,
                "function_name": g_entry.get("function_name"),
                "label_recall": report.get("label_recall"),
                "flow_edge_recall": report.get("flow_edge_recall"),
                "flow_edge_precision": report.get("flow_edge_precision"),
                "noreturn_match": report.get("noreturn_match"),
            }
        )

    aggregate = {
        "binary": str(binary),
        "function_count": len(rows),
        "rows": rows,
    }
    aggregate_path = args.output_dir / "sweep_fact_coverage.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"report": str(aggregate_path), "function_count": len(rows)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
