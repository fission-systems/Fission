#!/usr/bin/env python3
"""Fission Performance and Speed Benchmark Script.

Measures the execution time and throughput of all Fission CLI subcommands
(binary loading, listing, disassembly, lifting, structuring, and decompilation)
across a target corpus of binaries.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FISSION = REPO_ROOT / "target/release/fission_cli"
DEFAULT_OUTPUT = REPO_ROOT / "benchmark/artifacts/speed_benchmark_results.json"
DEFAULT_BINARY_DIR = REPO_ROOT / "benchmark/binary/x86-64/window/small/binary/c"


def extract_json(stdout: str) -> Any:
    stripped = stdout.strip()
    starts = [idx for idx in (stripped.find("{"), stripped.find("[")) if idx >= 0]
    if not starts:
        raise ValueError("Command output contains no JSON payload")
    return json.loads(stripped[min(starts) :])


def run_cmd(args: list[str], parse_as_json: bool = False) -> tuple[float, str, Any | None, bool]:
    """Runs a command, returns (elapsed_sec, stdout, json_payload_or_none, success)."""
    start = time.perf_counter()
    proc = subprocess.run(args, capture_output=True, text=True)
    elapsed = time.perf_counter() - start
    success = proc.returncode == 0
    
    json_data = None
    if success and parse_as_json:
        try:
            json_data = extract_json(proc.stdout)
        except Exception:
            pass
            
    return elapsed, proc.stdout, json_data, success


def calculate_stats(times: list[float]) -> dict[str, float]:
    """Calculates min, max, mean, stddev in milliseconds."""
    if not times:
        return {"min_ms": 0.0, "max_ms": 0.0, "mean_ms": 0.0, "stddev_ms": 0.0}
        
    times_ms = [t * 1000.0 for t in times]
    mean = sum(times_ms) / len(times_ms)
    
    if len(times_ms) > 1:
        variance = sum((t - mean) ** 2 for t in times_ms) / (len(times_ms) - 1)
        stddev = math.sqrt(variance)
    else:
        stddev = 0.0
        
    return {
        "min_ms": min(times_ms),
        "max_ms": max(times_ms),
        "mean_ms": mean,
        "stddev_ms": stddev,
    }


def find_default_binaries() -> list[Path]:
    """Finds default small C executables."""
    if not DEFAULT_BINARY_DIR.exists():
        return []
    binaries = []
    for file in DEFAULT_BINARY_DIR.iterdir():
        if file.is_file() and file.name.endswith(".exe") and not file.name.endswith("_ghidra"):
            binaries.append(file)
    return sorted(binaries)


def count_lines(code: str) -> int:
    return len([line for line in code.splitlines() if line.strip()])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        action="append",
        type=Path,
        help="Specify one or more binary paths to benchmark. If omitted, default small C binaries are used.",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=DEFAULT_FISSION,
        help="Path to the fission_cli executable.",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="Number of timed runs per benchmark (after 1 warm-up run).",
    )
    parser.add_argument(
        "--limit-functions",
        type=int,
        default=3,
        help="Maximum number of non-import/non-thunk functions to benchmark per binary.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="Path to save the JSON results report.",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Print verbose logging.",
    )

    args = parser.parse_args()
    fission_bin = args.fission_bin.resolve()

    if not fission_bin.exists():
        print(f"[-] Fission binary not found at: {fission_bin}")
        print("Please run 'cargo build -p fission-cli --release' first.")
        return 1

    binaries = args.binary if args.binary else find_default_binaries()
    if not binaries:
        print("[-] No benchmark binaries found. Please specify via --binary.")
        return 1

    print(f"[*] Starting Fission Speed Benchmark (iterations={args.iterations}, limit-functions={args.limit_functions})")
    print(f"[*] Fission CLI: {fission_bin}")
    print(f"[*] Target Binaries: {[b.name for b in binaries]}")
    print("=" * 80)

    report_results = []
    
    for binary_path in binaries:
        binary_path = binary_path.resolve()
        if not binary_path.exists():
            print(f"[-] Binary does not exist: {binary_path}")
            continue

        binary_size = binary_path.stat().st_size
        binary_size_mb = binary_size / (1024 * 1024)
        print(f"[*] Benchmarking: {binary_path.name} ({binary_size_mb:.2f} MB)")

        # Discovered functions list to use for function-level benchmarks
        print("    [+] Discovering functions...")
        _, _, funcs_json, success = run_cmd([str(fission_bin), "list", str(binary_path), "--json"], parse_as_json=True)
        if not success or not isinstance(funcs_json, list):
            print(f"    [-] Failed to list functions for {binary_path.name}. Skipping function-level tests.")
            funcs_json = []

        # Filter functions for benchmark candidates
        candidates = []
        for f in funcs_json:
            if not f.get("is_import") and not f.get("is_thunk_like") and f.get("size", 0) > 0:
                candidates.append(f)
        
        # Sort candidates by size descending and select top
        candidates.sort(key=lambda x: x.get("size", 0), reverse=True)
        target_funcs = candidates[: args.limit_functions]
        
        print(f"    [+] Selected {len(target_funcs)} functions for function-specific benchmarks:")
        for f in target_funcs:
            print(f"        - {f.get('name')} @ {f.get('address')} (size: {f.get('size')} bytes)")

        # Define the set of binary-level benchmarks
        binary_bench_configs = [
            ("info", [str(fission_bin), "info", str(binary_path), "--json"], True),
            ("sections", [str(fission_bin), "info", str(binary_path), "--sections", "--json"], True),
            ("imports", [str(fission_bin), "info", str(binary_path), "--imports", "--json"], True),
            ("exports", [str(fission_bin), "info", str(binary_path), "--exports", "--json"], True),
            ("strings", [str(fission_bin), "strings", str(binary_path), "--json"], True),
            ("list", [str(fission_bin), "list", str(binary_path), "--json"], True),
            ("xrefs", [str(fission_bin), "xrefs", str(binary_path), "--json"], True),
            ("callgraph", [str(fission_bin), "callgraph", str(binary_path), "--json"], True),
        ]

        # Execute binary-level benchmarks
        for name, cmd, parse_json in binary_bench_configs:
            print(f"    [+] Benchmarking binary feature: {name}")
            # Warm-up run
            run_cmd(cmd)
            
            times = []
            payload = None
            for i in range(args.iterations):
                elapsed, stdout, json_data, success = run_cmd(cmd, parse_as_json=parse_json)
                if not success:
                    print(f"        [-] Run {i+1} failed.")
                    continue
                times.append(elapsed)
                if json_data:
                    payload = json_data

            if not times:
                continue

            stats = calculate_stats(times)
            mean_sec = stats["mean_ms"] / 1000.0

            # Calculate throughput based on feature name
            throughput_str = "N/A"
            if name in ["info", "sections", "imports", "exports", "strings"]:
                throughput_str = f"{binary_size_mb / mean_sec:.2f} MB/s"
            elif name == "list" and isinstance(payload, list):
                throughput_str = f"{len(payload) / mean_sec:.2f} funcs/s"
            elif name in ["xrefs", "callgraph"] and isinstance(payload, list):
                throughput_str = f"{len(payload) / mean_sec:.2f} items/s"

            report_results.append({
                "binary": binary_path.name,
                "feature": name,
                "scope": "binary",
                "target": "all",
                "stats": stats,
                "throughput": throughput_str,
                "raw_times_sec": times,
            })

        # Define function-level benchmarks
        if target_funcs:
            func_features = ["disasm", "raw-pcode", "pcode-stages", "nir-stats", "pcode-topology", "decomp"]
            for f_feat in func_features:
                print(f"    [+] Benchmarking function feature: {f_feat}")
                
                # We aggregate times and sizes across all target functions
                feat_times = []
                total_bytes_processed = 0
                total_lines_decompiled = 0
                decomp_internal_secs = []

                for func in target_funcs:
                    addr = func["address"]
                    name = func["name"]
                    size = func["size"]
                    total_bytes_processed += size

                    # Build cmd
                    if f_feat == "disasm":
                        cmd = [str(fission_bin), "disasm", str(binary_path), "--addr", addr, "--function", "--json"]
                    elif f_feat == "raw-pcode":
                        cmd = [str(fission_bin), "raw-pcode", str(binary_path), "--addr", addr, "--json"]
                    elif f_feat == "pcode-stages":
                        cmd = [str(fission_bin), "pcode-stages", str(binary_path), "--addr", addr, "--json"]
                    elif f_feat == "nir-stats":
                        cmd = [str(fission_bin), "nir-stats", str(binary_path), "--addr", addr, "--json"]
                    elif f_feat == "pcode-topology":
                        cmd = [str(fission_bin), "pcode-topology", str(binary_path), "--addr", addr, "--json"]
                    elif f_feat == "decomp":
                        cmd = [str(fission_bin), "decomp", str(binary_path), "--addr", addr, "--json", "--benchmark"]

                    # Warm-up run
                    run_cmd(cmd)

                    # Timed runs
                    for i in range(args.iterations):
                        elapsed, stdout, json_data, success = run_cmd(cmd, parse_as_json=True)
                        if not success:
                            continue
                        feat_times.append(elapsed)

                        # Additional decomp metrics
                        if f_feat == "decomp" and isinstance(json_data, dict):
                            # extract internal timing
                            meta = json_data.get("_meta", {})
                            total_decomp_sec = meta.get("total_decomp_sec")
                            if isinstance(total_decomp_sec, (int, float)):
                                decomp_internal_secs.append(total_decomp_sec)
                            
                            # extract decompiled code lines
                            funcs_list = json_data.get("functions", [])
                            if funcs_list:
                                code = funcs_list[0].get("code", "")
                                total_lines_decompiled += count_lines(code)

                if not feat_times:
                    continue

                stats = calculate_stats(feat_times)
                # Adjust throughput calculations for multiple functions aggregated
                mean_sec_per_run = stats["mean_ms"] / 1000.0
                
                throughput_str = "N/A"
                if f_feat in ["disasm", "raw-pcode", "pcode-stages", "nir-stats", "pcode-topology"]:
                    # Throughput is average bytes processed per second per run
                    # each run executes the subcommand on one function at a time, but times are collected across all functions.
                    # Average time of a single function execution is mean_sec_per_run
                    # Average function size is total_bytes_processed / len(target_funcs)
                    avg_size = total_bytes_processed / len(target_funcs)
                    throughput_str = f"{(avg_size / 1024.0) / mean_sec_per_run:.2f} KB/s"
                elif f_feat == "decomp":
                    # Lines decompiled per second (aggregate lines divided by total elapsed time per iteration cycle)
                    avg_lines = total_lines_decompiled / args.iterations / len(target_funcs)
                    throughput_str = f"{avg_lines / mean_sec_per_run:.2f} lines/s"
                    
                    if decomp_internal_secs:
                        avg_internal_ms = (sum(decomp_internal_secs) / len(decomp_internal_secs)) * 1000.0
                        stats["internal_decomp_mean_ms"] = avg_internal_ms

                report_results.append({
                    "binary": binary_path.name,
                    "feature": f_feat,
                    "scope": "function",
                    "target": f"{len(target_funcs)} functions (avg size {total_bytes_processed // len(target_funcs)} B)",
                    "stats": stats,
                    "throughput": throughput_str,
                    "raw_times_sec": feat_times,
                })

        print("-" * 80)

    # Output Markdown table
    print("\n" + "=" * 32 + " BENCHMARK SUMMARY " + "=" * 32)
    print("| Binary | Scope | Feature | Latency (Mean ± Stddev ms) | Range [Min - Max ms] | Throughput |")
    print("| --- | --- | --- | --- | --- | --- |")
    for r in report_results:
        s = r["stats"]
        latency_str = f"{s['mean_ms']:.2f} ± {s['stddev_ms']:.2f}"
        range_str = f"[{s['min_ms']:.2f} - {s['max_ms']:.2f}]"
        
        # Include internal decomp time if available
        if "internal_decomp_mean_ms" in s:
            latency_str += f" (internal: {s['internal_decomp_mean_ms']:.2f} ms)"

        print(f"| {r['binary']} | {r['scope']} | {r['feature']} | {latency_str} | {range_str} | {r['throughput']} |")
    print("=" * 83)

    # Save detailed JSON report
    args.output.parent.mkdir(parents=True, exist_ok=True)
    report_envelope = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "iterations": args.iterations,
        "limit_functions": args.limit_functions,
        "fission_cli_version": "0.1.0",
        "results": report_results,
    }
    
    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(report_envelope, f, indent=2, sort_keys=True)
    print(f"\n[+] Detailed benchmark report saved to: {args.output}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
