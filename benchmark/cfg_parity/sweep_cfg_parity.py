#!/usr/bin/env python3
"""Sweep Ghidra BBM vs Fission instruction CFG across all functions in a binary."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

from compare_cfg import compare_snapshots


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent

DEFAULT_CORPUS = [
    "benchmark/binary/x86-64/window/small/binary/c/test_functions.exe",
    "benchmark/binary/x86-64/window/small/binary/c/bitops_and_control_flow.exe",
    "benchmark/binary/x86-64/window/small/binary/c/array_operations.exe",
    "benchmark/binary/x86-64/window/small/binary/c/math_operations.exe",
    "benchmark/binary/x86-64/window/small/binary/c/structs_and_pointers.exe",
    "benchmark/binary/x86-64/window/small/binary/c/instruction_matrix.exe",
    "benchmark/binary/x86-64/window/commercial_binary/binary/PuTTY_V0.83.exe",
]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def classify_gap(report: dict[str, Any]) -> list[str]:
    causes: list[str] = []
    if report.get("buckets") == ["full_match"]:
        return ["full_match"]

    missing_blocks = report.get("missing_blocks", [])
    extra_blocks = report.get("extra_blocks", [])
    missing_edges = report.get("missing_edges", [])
    extra_edges = report.get("extra_edges", [])

    if missing_blocks or extra_blocks:
        if extra_blocks and not missing_blocks:
            causes.append("extra_block")
        elif missing_blocks and not extra_blocks:
            causes.append("missing_block")
        else:
            causes.append("block_set_mismatch")

    if missing_edges or extra_edges:
        if extra_edges and not missing_edges:
            causes.append("extra_edge")
        elif missing_edges and not extra_edges:
            causes.append("missing_edge")
        else:
            causes.append("edge_set_mismatch")

    if report.get("missing_exit_blocks") or report.get("extra_exit_blocks"):
        causes.append("exit_set_mismatch")

    if not causes:
        causes.append("unknown_mismatch")
    return causes


def sweep_binary(
    *,
    binary: Path,
    ghidra_dir: Path | None,
    output_dir: Path,
    fission_release: bool,
    language: str,
    compiler: str,
    decompile_timeout_sec: int,
) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    ghidra_json = output_dir / "ghidra_all_functions.json"
    fission_json = output_dir / "fission_all_functions.json"
    inventory_json = output_dir / "gap_inventory.json"

    ghidra_cmd = [
        "python3",
        str(THIS_DIR / "ghidra_cfg.py"),
        "--binary",
        str(binary),
        "--all-functions",
        "--model",
        "ghidra_basic_block_model",
        "--language",
        language,
        "--compiler",
        compiler,
        "--decompile-timeout-sec",
        str(decompile_timeout_sec),
    ]
    if ghidra_dir is not None:
        ghidra_cmd += ["--ghidra-dir", str(ghidra_dir)]
    ghidra_cmd += ["--output", str(ghidra_json)]

    ghidra_proc = run(ghidra_cmd)
    if ghidra_proc.returncode != 0:
        raise SystemExit(
            f"ghidra sweep failed for {binary}\nSTDOUT:\n{ghidra_proc.stdout}\nSTDERR:\n{ghidra_proc.stderr}"
        )

    fission_cmd = ["cargo", "run", "-p", "fission-sleigh"]
    if fission_release:
        fission_cmd.append("--release")
    fission_cmd += [
        "--example",
        "cfg_probe_sweep",
        "--",
        "--binary",
        str(binary),
        "--model",
        "pcode_instruction_cfg",
    ]
    fission_proc = run(fission_cmd)
    print(fission_proc.stderr, file=sys.stderr)
    if fission_proc.returncode != 0:
        raise SystemExit(
            f"fission sweep failed for {binary}\nSTDOUT:\n{fission_proc.stdout}\nSTDERR:\n{fission_proc.stderr}"
        )
    fission_json.write_text(fission_proc.stdout)

    ghidra_payload = json.loads(ghidra_json.read_text())
    fission_payload = json.loads(fission_json.read_text())
    ghidra_functions = ghidra_payload.get("functions", {})
    fission_functions = fission_payload.get("functions", {})

    rows: list[dict[str, Any]] = []
    bucket_totals: Counter[str] = Counter()
    cause_totals: Counter[str] = Counter()
    by_cause: dict[str, list[dict[str, Any]]] = defaultdict(list)

    all_addrs = sorted(set(ghidra_functions.keys()) | set(fission_functions.keys()))
    for addr in all_addrs:
        gh_entry = ghidra_functions.get(addr)
        fi_entry = fission_functions.get(addr)
        if gh_entry is None:
            row = {
                "function_address": addr,
                "buckets": ["missing_ghidra_function"],
                "causes": ["missing_ghidra_function"],
            }
        elif fi_entry is None:
            row = {
                "function_address": addr,
                "function_name": gh_entry.get("function_name"),
                "buckets": ["missing_fission_function"],
                "causes": ["missing_fission_function"],
            }
        elif fi_entry.get("lift_error"):
            row = {
                "function_address": addr,
                "function_name": fi_entry.get("function_name"),
                "buckets": ["fission_lift_failed"],
                "causes": ["fission_lift_failed"],
                "lift_error": fi_entry.get("lift_error"),
            }
        else:
            report = compare_snapshots(
                {"snapshot": gh_entry["snapshot"]},
                {"snapshot": fi_entry["snapshot"]},
            )
            causes = classify_gap(report)
            row = {
                "function_address": addr,
                "function_name": fi_entry.get("function_name") or gh_entry.get("function_name"),
                "buckets": report.get("buckets", []),
                "causes": causes,
                "ghidra_block_count": report.get("ghidra_block_count"),
                "fission_block_count": report.get("fission_block_count"),
                "ghidra_edge_count": report.get("ghidra_edge_count"),
                "fission_edge_count": report.get("fission_edge_count"),
                "missing_blocks": report.get("missing_blocks", [])[:3],
                "extra_blocks": report.get("extra_blocks", [])[:3],
                "missing_edges": report.get("missing_edges", [])[:3],
                "extra_edges": report.get("extra_edges", [])[:3],
                "missing_exit_blocks": report.get("missing_exit_blocks", [])[:3],
                "extra_exit_blocks": report.get("extra_exit_blocks", [])[:3],
            }
        for bucket in row.get("buckets", []):
            bucket_totals[bucket] += 1
        for cause in row.get("causes", []):
            cause_totals[cause] += 1
            if cause != "full_match":
                by_cause[cause].append(row)
        rows.append(row)

    inventory = {
        "binary": str(binary),
        "function_count": len(all_addrs),
        "full_match_count": bucket_totals.get("full_match", 0),
        "mismatch_count": len(all_addrs) - bucket_totals.get("full_match", 0),
        "bucket_totals": dict(sorted(bucket_totals.items())),
        "cause_totals": dict(sorted(cause_totals.items())),
        "rows": rows,
        "examples_by_cause": {
            cause: examples[:5] for cause, examples in sorted(by_cause.items())
        },
    }
    inventory_json.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n")
    return inventory


def write_markdown_report(inventories: list[dict[str, Any]], output_path: Path) -> None:
    lines = [
        "# CFG Gap Inventory",
        "",
        "Ghidra `BasicBlockModel` vs Fission `pcode_instruction_cfg` sweep report.",
        "",
    ]
    total_functions = 0
    total_full = 0
    aggregate_causes: Counter[str] = Counter()
    for inventory in inventories:
        binary = inventory["binary"]
        total_functions += inventory["function_count"]
        total_full += inventory["full_match_count"]
        lines.append(f"## {Path(binary).name}")
        lines.append("")
        lines.append(f"- Binary: `{binary}`")
        lines.append(f"- Functions: {inventory['function_count']}")
        lines.append(f"- Full match: {inventory['full_match_count']}")
        lines.append(f"- Mismatch: {inventory['mismatch_count']}")
        lines.append(f"- Bucket totals: `{inventory['bucket_totals']}`")
        lines.append(f"- Cause totals: `{inventory['cause_totals']}`")
        lines.append("")
        for cause, examples in inventory.get("examples_by_cause", {}).items():
            aggregate_causes[cause] += len(examples)
            lines.append(f"### {cause}")
            lines.append("")
            for example in examples[:3]:
                lines.append(
                    f"- `{example.get('function_name', '?')}` @ {example['function_address']} "
                    f"buckets={example.get('buckets')}"
                )
            lines.append("")

    lines.insert(4, f"- Total functions swept: {total_functions}")
    lines.insert(5, f"- Total full match: {total_full}")
    lines.insert(6, f"- Total mismatch: {total_functions - total_full}")
    lines.insert(7, "")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, action="append", default=[])
    parser.add_argument("--corpus", choices=("default",), default="default")
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "benchmark/artifacts/cfg_parity/sweep")
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--fission-release", action="store_true")
    parser.add_argument("--decompile-timeout-sec", type=int, default=120)
    args = parser.parse_args()

    binaries = args.binary or [ROOT / rel for rel in DEFAULT_CORPUS]
    inventories = []
    for binary in binaries:
        binary = binary if binary.is_absolute() else ROOT / binary
        stem = binary.stem
        inventory = sweep_binary(
            binary=binary,
            ghidra_dir=args.ghidra_dir,
            output_dir=args.output_dir / stem,
            fission_release=args.fission_release,
            language=args.language,
            compiler=args.compiler,
            decompile_timeout_sec=args.decompile_timeout_sec,
        )
        inventories.append(inventory)
        print(json.dumps({"binary": str(binary), "inventory": inventory["cause_totals"]}, indent=2))

    write_markdown_report(
        inventories,
        args.output_dir / "gap_inventory.md",
    )
    print(json.dumps({"report": str(args.output_dir / "gap_inventory.md")}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
