#!/usr/bin/env python3
"""
Identify functions that trigger timeouts or hang-like behavior.

This script helps find the specific function responsible for a long timeout,
such as a long `fission_cli decomp putty.exe --all --limit 20` run.
It runs each function individually with `decomp --addr <addr>` and checks whether
the decompilation finishes within the given time budget.

Usage:
  python benchmark/full_benchmark/find_timeout_culprit.py samples/windows/x64/putty.exe
  python benchmark/full_benchmark/find_timeout_culprit.py putty.exe --limit 20 --timeout 120
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

def find_repo_root() -> Path:
    current = Path(__file__).resolve()
    for parent in [current.parent, *current.parents]:
        if (parent / "Cargo.toml").exists() and (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not locate repository root from script path")


ROOT_DIR = find_repo_root()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Find which function(s) cause timeout/hang in Fission decompilation.",
    )
    parser.add_argument("binary", type=Path, help="Path to the target binary")
    parser.add_argument(
        "--limit",
        type=int,
        default=20,
        help="Number of functions to test (first N by address order)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=120,
        help="Per-function timeout in seconds",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=None,
        help="Path to fission_cli binary",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Print timing for each function",
    )
    return parser.parse_args()


def resolve_fission_bin(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value.expanduser().resolve())

    candidates.extend(
        [
            (ROOT_DIR / "target" / "release" / "fission_cli").resolve(),
            (ROOT_DIR / "target" / "debug" / "fission_cli").resolve(),
        ]
    )

    for candidate in candidates:
        if candidate.is_file():
            return candidate

    raise FileNotFoundError(
        "fission_cli not found. Build with: cargo build --release -p fission-cli --features native_decomp"
    )


def make_fission_env(fission_bin: Path) -> dict[str, str]:
    """Build env with library paths for fission_cli (libdecomp.dylib etc)."""
    env = os.environ.copy()
    bin_dir = str(fission_bin.parent)
    add_library_search_path(env, "DYLD_LIBRARY_PATH", bin_dir)
    add_library_search_path(env, "LD_LIBRARY_PATH", bin_dir)
    # libdecomp may be in ghidra_decompiler/build if not copied to target/
    ghidra_build = str(ROOT_DIR / "ghidra_decompiler" / "build")
    add_library_search_path(env, "DYLD_LIBRARY_PATH", ghidra_build)
    add_library_search_path(env, "LD_LIBRARY_PATH", ghidra_build)
    return env


def get_function_addresses(binary: Path, fission_bin: Path, limit: int) -> list[dict]:
    """Get first N function addresses via `fission_cli list <binary> --json`."""
    cmd = [str(fission_bin), "list", str(binary), "--json"]
    env = make_fission_env(fission_bin)
    result = subprocess.run(
        cmd,
        cwd=ROOT_DIR,
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        print(result.stderr, file=sys.stderr)
        raise RuntimeError(f"fission_cli list failed: {result.returncode}")

    funcs = json.loads(result.stdout)
    if not isinstance(funcs, list):
        raise RuntimeError("Expected JSON array from fission_cli list --json")

    return funcs[:limit]


def add_library_search_path(env: dict[str, str], key: str, value: str) -> None:
    current = env.get(key, "")
    env[key] = value if not current else f"{value}{os.pathsep}{current}"


def test_single_function(
    fission_bin: Path,
    binary: Path,
    addr: str,
    name: str,
    timeout_sec: int,
    verbose: bool,
) -> tuple[bool, float]:
    """
    Decompile a single function. Returns (success, elapsed_sec).
    success=False if timeout or non-zero exit.
    """
    import time

    with tempfile.NamedTemporaryFile(suffix=".json", delete=True) as tmp:
        cmd = [
            str(fission_bin),
            "decomp",
            str(binary),
            "--addr",
            addr,
            "--benchmark",
            "--ghidra-compat",
            "--output",
            tmp.name,
        ]
        env = make_fission_env(fission_bin)

        start = time.perf_counter()
        try:
            result = subprocess.run(
                cmd,
                cwd=ROOT_DIR,
                env=env,
                capture_output=True,
                text=True,
                timeout=timeout_sec,
            )
            elapsed = time.perf_counter() - start
            ok = result.returncode == 0
            if verbose:
                status = "OK" if ok else "FAIL"
                print(f"  [{status}] {addr} {name}: {elapsed:.1f}s")
            return ok, elapsed
        except subprocess.TimeoutExpired:
            elapsed = time.perf_counter() - start
            if verbose:
                print(f"  [TIMEOUT] {addr} {name}: >{timeout_sec}s")
            return False, elapsed


def main() -> int:
    args = parse_args()
    binary = args.binary.expanduser().resolve()
    if not binary.is_file():
        print(f"Error: binary not found: {binary}", file=sys.stderr)
        return 1

    fission_bin = resolve_fission_bin(args.fission_bin)
    print(f"[*] Binary: {binary}")
    print(f"[*] Fission: {fission_bin}")
    print(f"[*] Limit: {args.limit} functions, per-function timeout: {args.timeout}s")
    print()

    funcs = get_function_addresses(binary, fission_bin, args.limit)
    print(f"[*] Testing {len(funcs)} functions...")
    print()

    timed_out: list[dict] = []
    failed: list[dict] = []
    slow: list[tuple[dict, float]] = []

    for i, f in enumerate(funcs):
        addr = f.get("address", "")
        name = f.get("name", "?")
        ok, elapsed = test_single_function(
            fission_bin,
            binary,
            addr,
            name,
            args.timeout,
            args.verbose,
        )
        rec = {"address": addr, "name": name, "index": i + 1}
        if not ok:
            if elapsed >= args.timeout - 1:
                timed_out.append(rec)
            else:
                failed.append(rec)
        elif elapsed > 60:
            slow.append((rec, elapsed))

    print()
    print("=" * 60)
    print("RESULTS")
    print("=" * 60)

    if timed_out:
        print(f"\n*** TIMEOUT (>={args.timeout}s) — possible infinite loop or extreme bottleneck ***")
        for r in timed_out:
            print(f"  #{r['index']} {r['address']}  {r['name']}")
        print("\nSuggested profiling commands:")
        if timed_out:
            first = timed_out[0]
            print(f"  cargo flamegraph --bin fission_cli -- decomp {binary} --addr {first['address']}")
            print(f"  # Or on macOS: xcrun xctrace record --template 'Time Profiler' -- \\")
            print(f"  #   ./target/release/fission_cli decomp {binary} --addr {first['address']}")

    if failed and not all(r in timed_out for r in failed):
        print(f"\nFailed (exit != 0):")
        for r in failed:
            print(f"  #{r['index']} {r['address']}  {r['name']}")

    if slow:
        print(f"\nSlow (>60s, but not a timeout):")
        for r, sec in sorted(slow, key=lambda x: -x[1])[:5]:
            print(f"  #{r['index']} {r['address']}  {r['name']}: {sec:.1f}s")

    if not timed_out and not failed:
        print("\nAll tested functions completed within the time limit.")
        print("  -> The timeout may instead come from parallel-execution interactions or initialization overhead.")

    return 0 if not timed_out else 1


if __name__ == "__main__":
    sys.exit(main())
