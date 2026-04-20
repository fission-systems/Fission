#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


def find_repo_root() -> Path:
    current = Path(__file__).resolve()
    for parent in [current.parent, *current.parents]:
        if (parent / "Cargo.toml").exists() and (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not locate repository root from script path")


ROOT_DIR = find_repo_root()
BENCH_SCRIPT = ROOT_DIR / "benchmark" / "full_benchmark" / "full_decomp_benchmark.py"
DEFAULT_GHIDRA_DIR = ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
DEFAULT_BINARY = ROOT_DIR / "samples" / "windows" / "x64" / "test_control_flow_x64_O0.exe"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "debug" / "fission_cli"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run full_decomp_benchmark regression validation for limit 2/20 and "
            "check artifact existence, schema keys, and deterministic ordering."
        )
    )
    parser.add_argument("binary", type=Path, nargs="?", default=DEFAULT_BINARY)
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION_BIN)
    parser.add_argument("--ghidra-dir", type=Path, default=DEFAULT_GHIDRA_DIR)
    parser.add_argument(
        "--limits",
        default="2,20",
        help="Comma-separated limits to validate (default: 2,20)",
    )
    parser.add_argument(
        "--repeat",
        type=int,
        default=2,
        help="Run count per limit for determinism checks (default: 2)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=600,
        help="Timeout seconds passed to full benchmark script (default: 600)",
    )
    parser.add_argument(
        "--output-root",
        type=Path,
        default=None,
        help="Optional explicit output root for regression artifacts",
    )
    return parser.parse_args()


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"
    text = str(value).strip()
    if not text:
        return "0x0"
    if text.lower().startswith("0x"):
        return f"0x{int(text, 16):x}"
    return f"0x{int(text, 16):x}"


def must_file(path: Path) -> None:
    if not path.is_file():
        raise RuntimeError(f"missing artifact: {path}")


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def is_non_decreasing_hex(addresses: list[str]) -> bool:
    vals = [int(addr, 16) for addr in addresses]
    return vals == sorted(vals)


def key_signature(items: list[dict[str, Any]], address_key: str = "address") -> dict[str, list[str]]:
    sig: dict[str, list[str]] = {}
    for item in items:
        addr = canonical_address(item.get(address_key, "0x0"))
        sig[addr] = sorted(item.keys())
    return sig


def validate_summary_schema(summary: dict[str, Any]) -> None:
    required_top = {"summary", "pairwise", "engines"}
    missing_top = sorted(required_top - set(summary.keys()))
    if missing_top:
        raise RuntimeError(f"benchmark_summary.json missing keys: {missing_top}")

    required_summary = {"engines", "quality", "speed", "resources", "samples"}
    missing_summary = sorted(required_summary - set(summary["summary"].keys()))
    if missing_summary:
        raise RuntimeError(f"summary section missing keys: {missing_summary}")

    pair = summary["pairwise"].get("pyghidra_vs_fission")
    if not isinstance(pair, dict):
        raise RuntimeError("pairwise.pyghidra_vs_fission is missing")
    if "comparisons" not in pair:
        raise RuntimeError("pairwise.pyghidra_vs_fission.comparisons is missing")


def run_one(
    binary: Path,
    fission_bin: Path,
    ghidra_dir: Path,
    limit: int,
    run_idx: int,
    timeout: int,
    output_root: Path,
) -> dict[str, Any]:
    output_dir = output_root / f"limit{limit}-run{run_idx}"
    output_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        sys.executable,
        str(BENCH_SCRIPT),
        str(binary),
        "--fission-bin",
        str(fission_bin),
        "--ghidra-dir",
        str(ghidra_dir),
        "--limit",
        str(limit),
        "--timeout",
        str(timeout),
        "--output-dir",
        str(output_dir),
    ]

    print(f"[*] run limit={limit} idx={run_idx}: {' '.join(cmd)}")
    completed = subprocess.run(
        cmd,
        cwd=ROOT_DIR,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"benchmark run failed (limit={limit}, run={run_idx}, code={completed.returncode})\n"
            f"stdout:\n{completed.stdout[-4000:]}\n"
            f"stderr:\n{completed.stderr[-4000:]}"
        )

    fission_path = output_dir / "fission_full.json"
    ghidra_path = output_dir / "ghidra_full.json"
    summary_path = output_dir / "benchmark_summary.json"
    summary_md_path = output_dir / "benchmark_summary.md"

    for path in [fission_path, ghidra_path, summary_path, summary_md_path]:
        must_file(path)

    fission_payload = load_json(fission_path)
    ghidra_payload = load_json(ghidra_path)
    summary_payload = load_json(summary_path)
    validate_summary_schema(summary_payload)

    fission_functions = fission_payload.get("functions", [])
    ghidra_functions = ghidra_payload.get("functions", [])
    pairwise_rows = (
        summary_payload.get("pairwise", {})
        .get("pyghidra_vs_fission", {})
        .get("comparisons", [])
    )

    fission_addrs = [canonical_address(item.get("address", "0x0")) for item in fission_functions]
    ghidra_addrs = [canonical_address(item.get("address", "0x0")) for item in ghidra_functions]
    pairwise_addrs = [canonical_address(item.get("address", "0x0")) for item in pairwise_rows]

    if not is_non_decreasing_hex(fission_addrs):
        raise RuntimeError(f"fission_full.json function ordering is not sorted (limit={limit}, run={run_idx})")
    if not is_non_decreasing_hex(ghidra_addrs):
        raise RuntimeError(f"ghidra_full.json function ordering is not sorted (limit={limit}, run={run_idx})")
    if pairwise_addrs and not is_non_decreasing_hex(pairwise_addrs):
        raise RuntimeError(
            f"benchmark_summary.json pairwise comparison ordering is not sorted (limit={limit}, run={run_idx})"
        )

    return {
        "output_dir": output_dir,
        "fission_addrs": fission_addrs,
        "ghidra_addrs": ghidra_addrs,
        "pairwise_addrs": pairwise_addrs,
        "fission_sig": key_signature(fission_functions),
        "ghidra_sig": key_signature(ghidra_functions),
        "pairwise_sig": key_signature(pairwise_rows),
    }


def parse_limits(text: str) -> list[int]:
    out: list[int] = []
    for piece in text.split(","):
        piece = piece.strip()
        if not piece:
            continue
        value = int(piece)
        if value <= 0:
            raise ValueError("limits must be positive")
        out.append(value)
    if not out:
        raise ValueError("at least one limit must be provided")
    return out


def validate_determinism(limit: int, runs: list[dict[str, Any]]) -> None:
    baseline = runs[0]
    for idx, current in enumerate(runs[1:], start=2):
        if current["fission_addrs"] != baseline["fission_addrs"]:
            raise RuntimeError(f"limit={limit}: fission address list differs between run1 and run{idx}")
        if current["ghidra_addrs"] != baseline["ghidra_addrs"]:
            raise RuntimeError(f"limit={limit}: ghidra address list differs between run1 and run{idx}")
        if current["pairwise_addrs"] != baseline["pairwise_addrs"]:
            raise RuntimeError(f"limit={limit}: pairwise address list differs between run1 and run{idx}")
        if current["fission_sig"] != baseline["fission_sig"]:
            raise RuntimeError(f"limit={limit}: fission key-shape differs between run1 and run{idx}")
        if current["ghidra_sig"] != baseline["ghidra_sig"]:
            raise RuntimeError(f"limit={limit}: ghidra key-shape differs between run1 and run{idx}")
        if current["pairwise_sig"] != baseline["pairwise_sig"]:
            raise RuntimeError(f"limit={limit}: pairwise key-shape differs between run1 and run{idx}")


def main() -> int:
    args = parse_args()
    limits = parse_limits(args.limits)

    binary = args.binary.expanduser().resolve()
    fission_bin = args.fission_bin.expanduser().resolve()
    ghidra_dir = args.ghidra_dir.expanduser().resolve()

    if not binary.is_file():
        raise FileNotFoundError(f"binary not found: {binary}")
    if not fission_bin.is_file():
        raise FileNotFoundError(f"fission_cli not found: {fission_bin}")
    if not ghidra_dir.exists():
        raise FileNotFoundError(f"ghidra dir not found: {ghidra_dir}")
    if not BENCH_SCRIPT.is_file():
        raise FileNotFoundError(f"benchmark script not found: {BENCH_SCRIPT}")
    if args.repeat < 2:
        raise ValueError("--repeat must be >= 2 for determinism checks")

    ts = time.strftime("%Y%m%d-%H%M%S")
    output_root = (
        args.output_root.expanduser().resolve()
        if args.output_root
        else ROOT_DIR / "artifacts" / "batch_benchmark" / f"regression-limit-validation-{ts}"
    )
    output_root.mkdir(parents=True, exist_ok=True)

    print(f"[*] binary      : {binary}")
    print(f"[*] fission_cli : {fission_bin}")
    print(f"[*] ghidra_dir  : {ghidra_dir}")
    print(f"[*] limits      : {limits}")
    print(f"[*] repeat      : {args.repeat}")
    print(f"[*] output_root : {output_root}")

    for limit in limits:
        run_results: list[dict[str, Any]] = []
        for run_idx in range(1, args.repeat + 1):
            run_results.append(
                run_one(
                    binary=binary,
                    fission_bin=fission_bin,
                    ghidra_dir=ghidra_dir,
                    limit=limit,
                    run_idx=run_idx,
                    timeout=args.timeout,
                    output_root=output_root,
                )
            )

        validate_determinism(limit, run_results)
        print(
            f"[ok] limit={limit}: artifacts/schema/order/determinism checks passed "
            f"(runs={args.repeat})"
        )

    print("[ok] regression validation complete")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[-] {exc}", file=sys.stderr)
        raise SystemExit(1)
