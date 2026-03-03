#!/usr/bin/env python3
"""
speed_benchmark.py — Fission vs Ghidra decompiler speed benchmark.

Fair comparison: both tools initialize ONCE then batch-decompile all functions.
- Fission: runs `fission binary -A --benchmark --ghidra-compat` (single process)
- Ghidra:  reads pre-cached results from pyghidra_decompile_batch.py output,
           or runs live batch decompilation.

Each function's `decomp_sec` measures ONLY the decompilation call time
(no init, no post-processing), ensuring an apples-to-apples comparison.

Usage:
    # Compare using existing Ghidra cache
    python3 speed_benchmark.py --binary samples/windows/test_all.exe \\
        --ghidra-cache benchmark_cache/

    # Run Ghidra live (requires PyGhidra)
    python3 speed_benchmark.py --binary samples/windows/test_all.exe \\
        --ghidra-live

    # Fission only (no Ghidra comparison)
    python3 speed_benchmark.py --binary samples/windows/test_all.exe

    # Output JSON results
    python3 speed_benchmark.py --binary samples/windows/test_all.exe \\
        --ghidra-cache benchmark_cache/ -o results.json

    # Custom Fission profile
    python3 speed_benchmark.py --binary samples/windows/test_all.exe \\
        --profile speed
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


# ===========================================================================
# Path helpers
# ===========================================================================

def project_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


def detect_fission_cmd() -> list[str]:
    root = project_root()
    for rel in ("target/release/fission_cli", "target/debug/fission_cli"):
        p = root / rel
        if p.exists():
            return [str(p)]
    return ["cargo", "run", "--quiet", "--bin", "fission_cli", "--"]


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    libdecomp_dir = str(project_root() / "ghidra_decompiler" / "build")
    for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        existing = env.get(var, "")
        env[var] = f"{libdecomp_dir}:{existing}" if existing else libdecomp_dir
    return env


# ===========================================================================
# Fission batch execution
# ===========================================================================

def run_fission_batch(
    binary: str,
    profile: str = "balanced",
    timeout: int = 300,
) -> tuple[dict[str, Any] | None, float]:
    """
    Run Fission in batch benchmark mode (single process, all functions).
    Returns (parsed_json, wall_clock_sec).
    """
    cmd = detect_fission_cmd() + [
        binary,
        "-A",
        "--benchmark",
        "--ghidra-compat",
        "--profile", profile,
    ]

    print(f"[*] Running Fission batch: {' '.join(cmd)}")
    wall_start = time.perf_counter()

    try:
        result = subprocess.run(
            cmd,
            cwd=str(project_root()),
            env=build_env(),
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        wall_elapsed = time.perf_counter() - wall_start

        if result.returncode != 0:
            print(f"[!] Fission exited with code {result.returncode}")
            if result.stderr:
                # Print last 20 lines of stderr
                lines = result.stderr.strip().splitlines()
                for line in lines[-20:]:
                    print(f"    {line}")

        stdout = result.stdout.strip()
        if not stdout:
            print("[!] Fission produced no output")
            return None, wall_elapsed

        try:
            data = json.loads(stdout)
            return data, wall_elapsed
        except json.JSONDecodeError as e:
            print(f"[!] Failed to parse Fission JSON output: {e}")
            # Try to find JSON in output (skip non-JSON preamble)
            for i, ch in enumerate(stdout):
                if ch == '{':
                    try:
                        data = json.loads(stdout[i:])
                        return data, wall_elapsed
                    except json.JSONDecodeError:
                        pass
            return None, wall_elapsed

    except subprocess.TimeoutExpired:
        wall_elapsed = time.perf_counter() - wall_start
        print(f"[!] Fission timed out after {timeout}s")
        return None, wall_elapsed
    except FileNotFoundError:
        print("[!] fission_cli not found. Build with: cargo build --bin fission_cli")
        return None, 0.0


# ===========================================================================
# Ghidra cache reader
# ===========================================================================

def load_ghidra_cache(cache_dir: Path) -> dict[str, dict]:
    """
    Load all cached Ghidra results from a directory.
    Returns {normalized_address: {name, address, code, decomp_sec, ...}}
    """
    results = {}
    if not cache_dir.exists():
        print(f"[!] Ghidra cache directory not found: {cache_dir}")
        return results

    for f in sorted(cache_dir.glob("ghidra_0x*.json")):
        try:
            with open(f, "r", encoding="utf-8") as fp:
                data = json.load(fp)
            addr = data.get("address", "").lower()
            if addr:
                results[addr] = data
        except (json.JSONDecodeError, OSError) as e:
            print(f"  [!] Skipping {f.name}: {e}")

    return results


def run_ghidra_live(binary: str, addresses: list[str], output_dir: Path, timeout: int = 600) -> dict[str, dict]:
    """
    Run PyGhidra batch decompilation live.
    Returns {normalized_address: {name, address, code, decomp_sec, ...}}
    """
    script = project_root() / "scripts" / "ghidra" / "pyghidra_decompile_batch.py"
    if not script.exists():
        print(f"[!] PyGhidra batch script not found: {script}")
        return {}

    # Write address file
    addr_file = output_dir / "addresses.txt"
    with open(addr_file, "w") as f:
        for addr in addresses:
            f.write(f"{addr}\n")

    ghidra_out = output_dir / "ghidra_output"
    ghidra_out.mkdir(parents=True, exist_ok=True)

    cmd = [sys.executable, str(script), binary, str(addr_file), str(ghidra_out)]
    print(f"[*] Running Ghidra batch: {' '.join(cmd)}")

    try:
        subprocess.run(cmd, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        print(f"[!] Ghidra timed out after {timeout}s")
    except FileNotFoundError:
        print("[!] Python not found")

    return load_ghidra_cache(ghidra_out)


# ===========================================================================
# Statistics
# ===========================================================================

def compute_stats(times: list[float]) -> dict[str, float]:
    if not times:
        return {"count": 0}
    times_sorted = sorted(times)
    n = len(times_sorted)
    total = sum(times_sorted)
    mean = total / n
    median = times_sorted[n // 2] if n % 2 else (times_sorted[n // 2 - 1] + times_sorted[n // 2]) / 2
    p95_idx = min(int(n * 0.95), n - 1)
    return {
        "count": n,
        "total_sec": round(total, 6),
        "mean_ms": round(mean * 1000, 3),
        "median_ms": round(median * 1000, 3),
        "min_ms": round(times_sorted[0] * 1000, 3),
        "max_ms": round(times_sorted[-1] * 1000, 3),
        "p95_ms": round(times_sorted[p95_idx] * 1000, 3),
    }


# ===========================================================================
# Comparison & reporting
# ===========================================================================

def compare_results(
    fission_data: dict[str, Any],
    ghidra_cache: dict[str, dict],
) -> list[dict]:
    """Match Fission and Ghidra results by address and build comparison rows."""
    fission_funcs = fission_data.get("functions", [])
    rows = []

    for func in fission_funcs:
        addr = func.get("address", "").lower()
        name = func.get("name", "unknown")
        f_sec = func.get("decomp_sec")
        has_error = "error" in func

        ghidra = ghidra_cache.get(addr)
        g_sec = ghidra.get("decomp_sec") if ghidra else None

        row: dict[str, Any] = {
            "address": addr,
            "name": name,
            "fission_ms": round(f_sec * 1000, 3) if f_sec is not None else None,
            "fission_error": has_error,
        }

        if g_sec is not None:
            row["ghidra_ms"] = round(g_sec * 1000, 3)
            if f_sec is not None and g_sec > 0:
                row["speedup"] = round(g_sec / f_sec, 2) if f_sec > 0 else float("inf")
        else:
            row["ghidra_ms"] = None

        rows.append(row)

    return rows


def print_table(rows: list[dict], fission_meta: dict | None, ghidra_cache: dict | None):
    """Print a human-readable comparison table to stdout."""
    has_ghidra = any(r.get("ghidra_ms") is not None for r in rows)

    # Header
    print()
    print("=" * 90)
    print("  Fission vs Ghidra — Speed Benchmark Results")
    print("=" * 90)

    if fission_meta:
        print(f"  Profile:   {fission_meta.get('profile', 'N/A')}")
        print(f"  Functions: {fission_meta.get('function_count', 'N/A')}")
        print(f"  Init:      {fission_meta.get('init_sec', 0):.3f}s")
        print(f"  Total decompile: {fission_meta.get('total_decomp_sec', 0):.3f}s")
        print(f"  Wall clock:      {fission_meta.get('wall_clock_sec', 0):.3f}s")
    print()

    # Per-function table
    if has_ghidra:
        hdr = f"  {'Address':<18} {'Name':<28} {'Fission(ms)':>12} {'Ghidra(ms)':>12} {'Speedup':>8}"
    else:
        hdr = f"  {'Address':<18} {'Name':<28} {'Fission(ms)':>12}"
    print(hdr)
    print("  " + "-" * (len(hdr) - 2))

    for r in sorted(rows, key=lambda x: x.get("fission_ms") or 0, reverse=True):
        addr = r["address"]
        name = r["name"][:27]
        f_ms = f"{r['fission_ms']:.3f}" if r.get("fission_ms") is not None else "ERROR"
        if r.get("fission_error"):
            f_ms = "ERROR"

        if has_ghidra:
            g_ms = f"{r['ghidra_ms']:.3f}" if r.get("ghidra_ms") is not None else "N/A"
            speedup = f"{r['speedup']:.2f}x" if r.get("speedup") is not None else "N/A"
            print(f"  {addr:<18} {name:<28} {f_ms:>12} {g_ms:>12} {speedup:>8}")
        else:
            print(f"  {addr:<18} {name:<28} {f_ms:>12}")

    # Summary statistics
    print()
    print("  " + "=" * 60)
    print("  Summary Statistics")
    print("  " + "=" * 60)

    fission_times = [r["fission_ms"] / 1000 for r in rows if r.get("fission_ms") is not None and not r.get("fission_error")]
    f_stats = compute_stats(fission_times)
    print(f"  Fission:  {f_stats.get('count', 0)} functions, "
          f"total={f_stats.get('total_sec', 0):.3f}s, "
          f"mean={f_stats.get('mean_ms', 0):.3f}ms, "
          f"median={f_stats.get('median_ms', 0):.3f}ms, "
          f"p95={f_stats.get('p95_ms', 0):.3f}ms")

    if has_ghidra:
        ghidra_times = [r["ghidra_ms"] / 1000 for r in rows if r.get("ghidra_ms") is not None]
        g_stats = compute_stats(ghidra_times)
        print(f"  Ghidra:   {g_stats.get('count', 0)} functions, "
              f"total={g_stats.get('total_sec', 0):.3f}s, "
              f"mean={g_stats.get('mean_ms', 0):.3f}ms, "
              f"median={g_stats.get('median_ms', 0):.3f}ms, "
              f"p95={g_stats.get('p95_ms', 0):.3f}ms")

        # Overall speedup
        if f_stats.get("total_sec", 0) > 0 and g_stats.get("total_sec", 0) > 0:
            overall_speedup = g_stats["total_sec"] / f_stats["total_sec"]
            faster = "Fission" if overall_speedup > 1 else "Ghidra"
            ratio = overall_speedup if overall_speedup > 1 else 1 / overall_speedup
            print(f"\n  >>> {faster} is {ratio:.2f}x faster overall <<<")

    print()


# ===========================================================================
# Main
# ===========================================================================

def main():
    parser = argparse.ArgumentParser(
        description="Fission vs Ghidra decompiler speed benchmark (fair batch comparison)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--binary", "-b", required=True, help="Path to target binary")
    parser.add_argument("--ghidra-cache", "-g", type=Path, help="Ghidra cache directory (pre-cached results)")
    parser.add_argument("--ghidra-live", action="store_true", help="Run Ghidra live via PyGhidra (slow, requires setup)")
    parser.add_argument("--profile", default="balanced", choices=["balanced", "quality", "speed"],
                        help="Fission decompilation profile (default: balanced)")
    parser.add_argument("--timeout", type=int, default=300, help="Fission process timeout in seconds (default: 300)")
    parser.add_argument("--ghidra-timeout", type=int, default=600, help="Ghidra process timeout in seconds (default: 600)")
    parser.add_argument("-o", "--output", type=Path, help="Write JSON results to file")
    parser.add_argument("--csv", type=Path, help="Write CSV results to file")
    parser.add_argument("--fission-only", action="store_true", help="Skip Ghidra comparison entirely")
    args = parser.parse_args()

    binary = str(Path(args.binary).resolve())
    if not Path(binary).exists():
        print(f"Error: Binary not found: {binary}")
        sys.exit(1)

    print(f"Binary: {binary}")
    print(f"Profile: {args.profile}")
    print(f"Platform: {platform.platform()}")
    print()

    # --- Step 1: Run Fission ---
    fission_data, fission_wall = run_fission_batch(binary, args.profile, args.timeout)
    if fission_data is None:
        print("[!] Fission failed. Cannot continue benchmark.")
        sys.exit(1)

    fission_meta = fission_data.get("_meta", {})
    fission_funcs = fission_data.get("functions", [])
    print(f"[+] Fission: {len(fission_funcs)} functions decompiled in {fission_wall:.3f}s (wall clock)")

    # --- Step 2: Get Ghidra results ---
    ghidra_cache: dict[str, dict] = {}

    if not args.fission_only:
        if args.ghidra_cache:
            ghidra_cache = load_ghidra_cache(args.ghidra_cache)
            print(f"[+] Ghidra cache: {len(ghidra_cache)} functions loaded from {args.ghidra_cache}")
        elif args.ghidra_live:
            addresses = [f.get("address", "") for f in fission_funcs if f.get("address")]
            with tempfile.TemporaryDirectory(prefix="fission_bench_") as tmpdir:
                ghidra_cache = run_ghidra_live(binary, addresses, Path(tmpdir), args.ghidra_timeout)
            print(f"[+] Ghidra live: {len(ghidra_cache)} functions decompiled")
        else:
            print("[*] No Ghidra source specified. Showing Fission-only results.")
            print("    Use --ghidra-cache DIR or --ghidra-live for comparison.")

    # --- Step 3: Compare & Report ---
    rows = compare_results(fission_data, ghidra_cache)
    print_table(rows, fission_meta, ghidra_cache if ghidra_cache else None)

    # --- Step 4: Save results ---
    output_obj = {
        "benchmark": "speed",
        "binary": binary,
        "platform": platform.platform(),
        "fission_meta": fission_meta,
        "fission_wall_sec": round(fission_wall, 6),
        "ghidra_functions_matched": sum(1 for r in rows if r.get("ghidra_ms") is not None),
        "fission_stats": compute_stats([
            r["fission_ms"] / 1000 for r in rows
            if r.get("fission_ms") is not None and not r.get("fission_error")
        ]),
        "ghidra_stats": compute_stats([
            r["ghidra_ms"] / 1000 for r in rows if r.get("ghidra_ms") is not None
        ]) if ghidra_cache else None,
        "per_function": rows,
    }

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(output_obj, f, indent=2)
        print(f"[+] JSON results written to: {args.output}")

    if args.csv:
        args.csv.parent.mkdir(parents=True, exist_ok=True)
        with open(args.csv, "w", encoding="utf-8") as f:
            has_ghidra = any(r.get("ghidra_ms") is not None for r in rows)
            if has_ghidra:
                f.write("address,name,fission_ms,ghidra_ms,speedup\n")
                for r in rows:
                    f_ms = f"{r['fission_ms']:.3f}" if r.get("fission_ms") is not None else ""
                    g_ms = f"{r['ghidra_ms']:.3f}" if r.get("ghidra_ms") is not None else ""
                    sp = f"{r['speedup']:.2f}" if r.get("speedup") is not None else ""
                    f.write(f"{r['address']},{r['name']},{f_ms},{g_ms},{sp}\n")
            else:
                f.write("address,name,fission_ms\n")
                for r in rows:
                    f_ms = f"{r['fission_ms']:.3f}" if r.get("fission_ms") is not None else ""
                    f.write(f"{r['address']},{r['name']},{f_ms}\n")
        print(f"[+] CSV results written to: {args.csv}")


if __name__ == "__main__":
    main()
