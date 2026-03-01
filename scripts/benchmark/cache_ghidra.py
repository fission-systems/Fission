#!/usr/bin/env python3
"""
cache_ghidra.py — Pre-cache Ghidra decompilation results for all benchmark binaries.

Runs PyGhidra ONCE per binary to decompile all functions, saving results as JSON
files in a cache directory. Subsequent benchmark runs read from cache instead of
launching Ghidra.

Usage:
    python3 cache_ghidra.py --suite suites/suite_all.yaml [-o benchmark_cache/]
    python3 cache_ghidra.py --suite suites/suite_macos_arm64.yaml --force

Cache structure:
    benchmark_cache/
      <binary_basename>/
        ghidra_0x<addr>.json    ← {name, address, code, asm, decomp_sec}
      cache_manifest.json       ← summary of cached results
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

try:
    import yaml as _yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False


def _project_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


def load_suite(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    if path.suffix in (".yaml", ".yml") and _YAML_AVAILABLE:
        return _yaml.safe_load(text)
    return json.loads(text)


def resolve_ghidra_install_dir() -> str:
    env_path = os.environ.get("GHIDRA_INSTALL_DIR")
    if env_path and Path(env_path).exists():
        return env_path
    root = _project_root()
    candidate = root / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
    if candidate.exists():
        return str(candidate)
    return env_path or str(candidate)


def cache_binary_pyghidra(
    binary_path: Path,
    addresses: list[dict],
    cache_dir: Path,
    timeout: int = 120,
) -> dict[str, Any]:
    """
    Decompile all functions in a binary using PyGhidra batch script.
    Returns stats: {total, success, failed, skipped}.
    """
    cache_dir.mkdir(parents=True, exist_ok=True)

    # Check which addresses are already cached
    to_decompile = []
    skipped = 0
    for func in addresses:
        addr_hex = func["address"]
        # Normalize address format for filename
        norm_addr = addr_hex.lower()
        cache_file = cache_dir / f"ghidra_{norm_addr}.json"
        if cache_file.exists():
            skipped += 1
        else:
            to_decompile.append(func)

    if not to_decompile:
        return {"total": len(addresses), "success": 0, "failed": 0, "skipped": skipped, "cached": True}

    # Write address file for batch script
    addr_file = cache_dir / "_addresses.txt"
    with open(addr_file, "w") as f:
        for func in to_decompile:
            f.write(f"{func['address']}  # {func.get('name', 'unknown')}\n")

    # Call pyghidra_decompile_batch.py
    batch_script = _project_root() / "scripts" / "ghidra" / "pyghidra_decompile_batch.py"

    if not batch_script.exists():
        print(f"  Error: {batch_script} not found", file=sys.stderr)
        return {"total": len(addresses), "success": 0, "failed": len(to_decompile), "skipped": skipped, "error": "batch script not found"}

    # Detect python with pyghidra
    python_cmd = sys.executable
    venv_python = _project_root() / ".venv" / "bin" / "python"
    if venv_python.exists():
        python_cmd = str(venv_python)

    cmd = [
        python_cmd,
        str(batch_script),
        str(binary_path),
        str(addr_file),
        str(cache_dir),
    ]

    print(f"  Running PyGhidra batch: {len(to_decompile)} functions...")
    start = time.perf_counter()
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout * len(to_decompile),  # scale timeout
            cwd=str(_project_root()),
        )
        elapsed = time.perf_counter() - start

        if result.returncode != 0:
            print(f"  PyGhidra stderr: {result.stderr[:500]}", file=sys.stderr)

        # Print batch output
        for line in result.stdout.splitlines():
            if line.strip():
                print(f"    {line}")

    except subprocess.TimeoutExpired:
        elapsed = time.perf_counter() - start
        print(f"  Timeout after {elapsed:.1f}s", file=sys.stderr)
    except FileNotFoundError as e:
        print(f"  Error launching PyGhidra: {e}", file=sys.stderr)
        return {"total": len(addresses), "success": 0, "failed": len(to_decompile), "skipped": skipped, "error": str(e)}

    # Count results
    success = 0
    failed = 0
    for func in to_decompile:
        norm_addr = func["address"].lower()
        cache_file = cache_dir / f"ghidra_{norm_addr}.json"
        if cache_file.exists():
            success += 1
        else:
            failed += 1

    # Clean up temp address file
    addr_file.unlink(missing_ok=True)

    return {
        "total": len(addresses),
        "success": success,
        "failed": failed,
        "skipped": skipped,
        "elapsed_sec": round(elapsed, 2),
    }


def cache_binary_fallback(
    binary_path: Path,
    addresses: list[dict],
    cache_dir: Path,
) -> dict[str, Any]:
    """
    Fallback: try importing pyghidra directly (if available in current env).
    If not available, create placeholder cache entries.
    """
    try:
        os.environ["GHIDRA_INSTALL_DIR"] = resolve_ghidra_install_dir()
        import pyghidra
        return _cache_direct(binary_path, addresses, cache_dir)
    except ImportError:
        print("  PyGhidra not available. Creating placeholder cache entries.", file=sys.stderr)
        return _create_placeholders(binary_path, addresses, cache_dir)


def _cache_direct(
    binary_path: Path,
    addresses: list[dict],
    cache_dir: Path,
) -> dict[str, Any]:
    """Direct PyGhidra decompilation (when running in a PyGhidra-capable env)."""
    import pyghidra

    cache_dir.mkdir(parents=True, exist_ok=True)
    skipped = 0
    success = 0
    failed = 0

    # Filter already cached
    to_process = []
    for func in addresses:
        norm_addr = func["address"].lower()
        if (cache_dir / f"ghidra_{norm_addr}.json").exists():
            skipped += 1
        else:
            to_process.append(func)

    if not to_process:
        return {"total": len(addresses), "success": 0, "failed": 0, "skipped": skipped, "cached": True}

    temp_dir = tempfile.mkdtemp(prefix="fission_bench_")
    try:
        with pyghidra.open_program(
            str(binary_path), analyze=True,
            project_location=temp_dir, project_name="fission_bench",
            nested_project_location=False,
        ) as flat_api:
            program = flat_api.getCurrentProgram()

            from ghidra.app.decompiler import DecompInterface
            from ghidra.util.task import ConsoleTaskMonitor

            decomp = DecompInterface()
            decomp.openProgram(program)
            monitor = ConsoleTaskMonitor()

            for func_info in to_process:
                addr_hex = func_info["address"]
                norm_addr = addr_hex.lower()
                try:
                    addr_int = int(addr_hex, 16)
                    ghidra_addr = program.getAddressFactory().getDefaultAddressSpace().getAddress(addr_int)

                    function = program.getFunctionManager().getFunctionAt(ghidra_addr)
                    if function is None:
                        function = program.getFunctionManager().getFunctionContaining(ghidra_addr)

                    if function is None:
                        flat_api.disassemble(ghidra_addr)
                        flat_api.createFunction(ghidra_addr, f"sub_{addr_int:x}")
                        function = program.getFunctionManager().getFunctionAt(ghidra_addr)

                    if function:
                        t0 = time.perf_counter()
                        results = decomp.decompileFunction(function, 30, monitor)
                        dt = time.perf_counter() - t0

                        if results.decompileCompleted():
                            ds = results.getDecompiledFunction()
                            code = ds.getC() if ds else "// Error: null decompilation"

                            res_obj = {
                                "name": function.getName(),
                                "address": addr_hex,
                                "code": code,
                                "decomp_sec": round(dt, 6),
                            }
                            with open(cache_dir / f"ghidra_{norm_addr}.json", "w") as f:
                                json.dump(res_obj, f, indent=2)
                            success += 1
                        else:
                            failed += 1
                    else:
                        failed += 1
                except Exception as e:
                    print(f"    Error at {addr_hex}: {e}", file=sys.stderr)
                    failed += 1

            decomp.dispose()
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

    return {"total": len(addresses), "success": success, "failed": failed, "skipped": skipped}


def _create_placeholders(
    binary_path: Path,
    addresses: list[dict],
    cache_dir: Path,
) -> dict[str, Any]:
    """Create placeholder JSON files when PyGhidra is not available."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    created = 0
    for func in addresses:
        norm_addr = func["address"].lower()
        cache_file = cache_dir / f"ghidra_{norm_addr}.json"
        if not cache_file.exists():
            res = {
                "name": func.get("name", "unknown"),
                "address": func["address"],
                "code": f"// Placeholder: PyGhidra not available\n// Binary: {binary_path.name}\n// Address: {func['address']}\n",
                "decomp_sec": 0.0,
                "placeholder": True,
            }
            with open(cache_file, "w") as f:
                json.dump(res, f, indent=2)
            created += 1
    return {"total": len(addresses), "success": 0, "failed": 0, "skipped": len(addresses) - created, "placeholders": created}


# ── CLI ──────────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(description="Cache Ghidra decompilation results")
    parser.add_argument("--suite", required=True, help="Suite YAML/JSON file")
    parser.add_argument("-o", "--output", default=None, help="Cache output directory")
    parser.add_argument("--force", action="store_true", help="Re-cache even if files exist")
    parser.add_argument("--timeout", type=int, default=120, help="Per-function timeout (sec)")
    parser.add_argument(
        "--method", choices=["batch", "direct", "auto"], default="auto",
        help="Caching method: batch (via batch script), direct (import pyghidra), auto",
    )
    args = parser.parse_args()

    root = _project_root()
    suite_path = Path(args.suite)
    if not suite_path.is_absolute():
        suite_path = root / suite_path
    if not suite_path.exists():
        # Try relative to benchmark dir
        suite_path = root / "scripts" / "benchmark" / "suites" / args.suite
    if not suite_path.exists():
        print(f"Error: Suite file not found: {args.suite}")
        return 1

    suite = load_suite(suite_path)
    cache_root = Path(args.output) if args.output else root / "benchmark_cache"

    print(f"Suite: {suite.get('name', 'unnamed')}")
    print(f"Cache: {cache_root}")
    print(f"Binaries: {len(suite.get('binaries', []))}")
    print()

    total_stats = {"total": 0, "success": 0, "failed": 0, "skipped": 0}
    binary_results = []

    for binary_def in suite.get("binaries", []):
        bin_path = root / binary_def["path"]
        if not bin_path.exists():
            print(f"  SKIP {binary_def['path']}: file not found")
            continue

        binary_name = bin_path.stem
        bin_cache_dir = cache_root / binary_name
        functions = binary_def.get("functions", [])

        if not functions:
            print(f"  SKIP {binary_name}: no functions defined")
            continue

        if args.force:
            # Remove existing cache
            if bin_cache_dir.exists():
                shutil.rmtree(bin_cache_dir)

        print(f"  [{binary_name}] {len(functions)} functions → {bin_cache_dir}")

        # Choose method
        if args.method == "batch":
            stats = cache_binary_pyghidra(bin_path, functions, bin_cache_dir, args.timeout)
        elif args.method == "direct":
            stats = cache_binary_fallback(bin_path, functions, bin_cache_dir)
        else:  # auto
            # Try batch script first
            batch_script = root / "scripts" / "ghidra" / "pyghidra_decompile_batch.py"
            if batch_script.exists():
                stats = cache_binary_pyghidra(bin_path, functions, bin_cache_dir, args.timeout)
            else:
                stats = cache_binary_fallback(bin_path, functions, bin_cache_dir)

        # Update totals
        for k in ("total", "success", "failed", "skipped"):
            total_stats[k] += stats.get(k, 0)

        cached_marker = " (already cached)" if stats.get("cached") else ""
        print(f"    → ok={stats.get('success',0)} fail={stats.get('failed',0)} skip={stats.get('skipped',0)}{cached_marker}")

        binary_results.append({
            "binary": str(binary_def["path"]),
            "cache_dir": str(bin_cache_dir),
            "stats": stats,
        })

    # Write manifest
    manifest = {
        "suite": suite.get("name", "unnamed"),
        "cache_dir": str(cache_root),
        "stats": total_stats,
        "binaries": binary_results,
    }
    manifest_path = cache_root / "cache_manifest.json"
    cache_root.mkdir(parents=True, exist_ok=True)
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"\n{'='*50}")
    print(f"Cache complete: {total_stats['success']} cached, "
          f"{total_stats['skipped']} skipped, {total_stats['failed']} failed")
    print(f"Manifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
