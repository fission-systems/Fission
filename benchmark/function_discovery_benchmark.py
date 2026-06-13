#!/usr/bin/env python3
"""Benchmark to compare Fission and Ghidra function discovery accuracy (Precision/Recall)."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Optional

ROOT = Path(__file__).resolve().parents[1]


def get_binary_files(directory: Path) -> list[Path]:
    """Scan directory recursively for executable binaries (PE/ELF/Mach-O)."""
    extensions = {".exe", ".dll", ".so", ".bin", "", ".elf"}
    binaries = []
    for root, _, files in os.walk(directory):
        for f in files:
            p = Path(root) / f
            if p.suffix.lower() in extensions:
                if p.is_file() and not p.is_symlink():
                    try:
                        # Quick signature verification to ignore text/data files
                        with p.open("rb") as fd:
                            head = fd.read(4)
                            # PE (MZ), ELF (\x7fELF), Mach-O magic numbers
                            if (
                                head.startswith(b"MZ")
                                or head.startswith(b"\x7fELF")
                                or head.startswith(b"\xca\xfe\xba\xbe")
                                or head.startswith(b"\xfe\xed\xfa\xcf")
                                or head.startswith(b"\xcf\xfa\xed\xfe")
                            ):
                                binaries.append(p)
                    except Exception:
                        pass
    return sorted(binaries)


def run_fission_list(binary_path: Path, profile: Optional[str]) -> set[int]:
    """Run fission-cli list command to get Fission discovered function entry points."""
    cmd = [
        "cargo",
        "run",
        "--release",
        "-p",
        "fission-cli",
        "--",
        "list",
        str(binary_path),
        "--json",
    ]
    if profile:
        cmd.extend(["--function-discovery-profile", profile])

    res = subprocess.run(cmd, capture_output=True, text=True, check=True)

    # Clean up stdout if cargo build outputs are mixed in
    idx = res.stdout.find("[")
    if idx != -1:
        stdout_json = res.stdout[idx:]
    else:
        stdout_json = res.stdout

    try:
        data = json.loads(stdout_json)
    except json.JSONDecodeError as e:
        print(f"Error parsing fission-cli JSON output: {e}", file=sys.stderr)
        print(f"CLI stdout: {res.stdout[:500]}...", file=sys.stderr)
        return set()

    # Collect function entry points that are not imports
    addrs = set()
    for item in data:
        if not item.get("is_import", False):
            addr_str = item.get("address")
            if addr_str:
                addrs.add(int(addr_str, 16))
    return addrs


def run_ghidra_analysis(binary_path: Path, ghidra_dir: Path) -> set[int]:
    """Run Ghidra headless analysis on the binary to get ground truth function entry points."""
    # Ensure pyghidra is imported dynamically after start() is called
    import pyghidra  # type: ignore

    ghidra_funcs = set()
    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        fm = program.getFunctionManager()
        for func in fm.getFunctions(True):
            ghidra_funcs.add(int(func.getEntryPoint().getOffset()))
    return ghidra_funcs


def calculate_metrics(fission_set: set[int], ghidra_set: set[int]) -> dict[str, Any]:
    """Calculate Precision, Recall, F1 Score and gather mismatched addresses."""
    tp = fission_set.intersection(ghidra_set)
    fp = fission_set.difference(ghidra_set)
    fn = ghidra_set.difference(fission_set)

    tp_len = len(tp)
    fp_len = len(fp)
    fn_len = len(fn)

    precision = tp_len / (tp_len + fp_len) if (tp_len + fp_len) > 0 else 0.0
    recall = tp_len / (tp_len + fn_len) if (tp_len + fn_len) > 0 else 0.0
    f1 = 2 * (precision * recall) / (precision + recall) if (precision + recall) > 0.0 else 0.0

    return {
        "precision": precision,
        "recall": recall,
        "f1_score": f1,
        "tp_count": tp_len,
        "fp_count": fp_len,
        "fn_count": fn_len,
        "tp_addresses": sorted([f"0x{addr:x}" for addr in tp]),
        "fp_addresses": sorted([f"0x{addr:x}" for addr in fp]),
        "fn_addresses": sorted([f"0x{addr:x}" for addr in fn]),
    }


def print_summary_table(results: list[dict[str, Any]]) -> None:
    """Print the final benchmark metrics using tabulate if available, falling back to clean text formatting."""
    headers = [
        "Binary",
        "Ghidra Funcs",
        "Fission Funcs",
        "TP",
        "FP",
        "FN",
        "Precision",
        "Recall",
        "F1 Score",
    ]
    rows = []
    for r in results:
        rows.append(
            [
                r["binary_name"],
                r["ghidra_total"],
                r["fission_total"],
                r["tp_count"],
                r["fp_count"],
                r["fn_count"],
                f"{r['precision'] * 100:.2f}%",
                f"{r['recall'] * 100:.2f}%",
                f"{r['f1_score'] * 100:.2f}%",
            ]
        )

    try:
        from tabulate import tabulate

        print("\n" + tabulate(rows, headers=headers, tablefmt="grid"))
    except ImportError:
        # Clean plain text fallback
        col_widths = [max(len(str(x)) for x in col) for col in zip(*([headers] + rows))]
        format_str = " | ".join(f"{{:<{w}}}" for w in col_widths)
        line = "-+-".join("-" * w for w in col_widths)
        print("\n" + format_str.format(*headers))
        print(line)
        for row in rows:
            print(format_str.format(*row))
        print()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Benchmark comparing Fission and Ghidra function discovery accuracy."
    )
    parser.add_argument(
        "--dir",
        type=Path,
        required=True,
        help="Directory to recursively scan for binaries.",
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=os.environ.get("GHIDRA_INSTALL_DIR"),
        help="Ghidra installation directory.",
    )
    parser.add_argument(
        "--profile",
        type=str,
        choices=["conservative", "balanced", "aggressive"],
        default="balanced",
        help="Fission function discovery profile.",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "benchmark" / "artifacts" / "function_discovery_benchmark" / "report.json",
        help="Path to save the detailed JSON benchmark report.",
    )
    args = parser.parse_args()

    if not args.ghidra_dir:
        print(
            "Error: Ghidra directory must be specified via --ghidra-dir or GHIDRA_INSTALL_DIR env variable.",
            file=sys.stderr,
        )
        return 1

    if not args.ghidra_dir.is_dir():
        print(f"Error: Ghidra directory not found: {args.ghidra_dir}", file=sys.stderr)
        return 1

    binaries = get_binary_files(args.dir)
    if not binaries:
        print(f"No valid binaries found in directory: {args.dir}", file=sys.stderr)
        return 0

    print(f"Found {len(binaries)} binary files to benchmark.")

    # Start PyGhidra JVM
    import pyghidra  # type: ignore

    print(f"Initializing PyGhidra with: {args.ghidra_dir}")
    pyghidra.start(install_dir=args.ghidra_dir)

    results = []
    wall_start = time.perf_counter()

    for bin_path in binaries:
        print(f"\nAnalyzing binary: {bin_path.name}")
        try:
            print("  Running Ghidra Analysis...")
            ghidra_set = run_ghidra_analysis(bin_path, args.ghidra_dir)

            print("  Running Fission Analysis...")
            fission_set = run_fission_list(bin_path, args.profile)

            print("  Comparing results...")
            metrics = calculate_metrics(fission_set, ghidra_set)
            metrics.update(
                {
                    "binary_name": bin_path.name,
                    "binary_path": str(bin_path.resolve().relative_to(ROOT)),
                    "ghidra_total": len(ghidra_set),
                    "fission_total": len(fission_set),
                }
            )
            results.append(metrics)

            print(
                f"  Done. Ghidra: {len(ghidra_set)} | Fission: {len(fission_set)} | F1: {metrics['f1_score'] * 100:.2f}%"
            )
        except Exception as e:
            print(f"  Failed to benchmark {bin_path.name}: {e}", file=sys.stderr)

    if not results:
        print("No binaries were successfully benchmarked.")
        return 1

    # Print summary table
    print_summary_table(results)

    # Save detailed JSON report
    report = {
        "_meta": {
            "tool": "function_discovery_benchmark",
            "scan_dir": str(args.dir.resolve().relative_to(ROOT) if args.dir.resolve().parts[:len(ROOT.parts)] == ROOT.parts else args.dir.resolve()),
            "ghidra_install_dir": str(args.ghidra_dir),
            "fission_profile": args.profile,
            "wall_clock_sec": round(time.perf_counter() - wall_start, 6),
        },
        "results": results,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"Detailed JSON report saved to: {args.out}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
