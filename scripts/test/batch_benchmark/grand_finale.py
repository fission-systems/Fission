#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

from grand_finale_support.metrics import load_struct_pointer_aliases, normalize_address
from grand_finale_support.report_md import write_markdown_report
from grand_finale_support.runners import (
    list_functions_with_fission,
    run_fission_function,
    run_ghidra_binary,
    sample_functions,
)
from grand_finale_support.summary import aggregate_global_report, summarize_binary


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_RESULTS_DIR = ROOT_DIR / "artifacts" / "grand_finale"
DEFAULT_GHIDRA_DIR = ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
BASE_TYPES_JSON = ROOT_DIR / "crates" / "fission-signatures" / "data" / "win_types" / "base_types.json"
MANDATORY_SAMPLE_ADDRESSES: dict[str, list[str]] = {
    "cmkr": [
        "0x140001000",
        "0x140003270",
        "0x1400034a0",
        "0x1400036e0",
        "0x140003920",
        "0x140004010",
    ],
    "everything": [
        "0x1401120c0",
        "0x14011d840",
        "0x140123c80",
        "0x14014df40",
        "0x140183590",
    ],
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Function-sampled decompilation quality benchmark between Fission and Ghidra."
    )
    parser.add_argument("binaries", nargs="+", help="Target binaries to benchmark")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_RESULTS_DIR,
        help="Directory to write benchmark artifacts into",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=ROOT_DIR / "target" / "release" / "fission_cli",
        help="Path to a prebuilt fission_cli binary with native_decomp enabled",
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=DEFAULT_GHIDRA_DIR,
        help="Path to Ghidra installation directory",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=30,
        help="Number of functions to sample from each binary (0 means all)",
    )
    parser.add_argument(
        "--per-func-timeout",
        type=int,
        default=90,
        help="Per-function timeout in seconds for Fission and Ghidra decompilation",
    )
    parser.add_argument(
        "--skip-ghidra",
        action="store_true",
        help="Collect only Fission metrics",
    )
    parser.add_argument(
        "--skip-preview-compare",
        action="store_true",
        help="Do not run the extra mlil-preview comparison pass",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")
    if not args.skip_ghidra and not args.ghidra_dir.exists():
        raise SystemExit(f"Ghidra dir not found: {args.ghidra_dir}")

    struct_ptr_aliases = load_struct_pointer_aliases(BASE_TYPES_JSON)
    binary_reports: list[dict[str, Any]] = []

    for binary_str in args.binaries:
        binary_path = Path(binary_str).resolve()
        binary_name = binary_path.stem
        print(f"[*] Benchmarking {binary_name} ...", flush=True)

        functions = sample_functions(
            binary_name,
            list_functions_with_fission(ROOT_DIR, binary_path, args.fission_bin),
            args.limit,
            MANDATORY_SAMPLE_ADDRESSES,
        )
        fission_entries: dict[str, dict[str, Any]] = {}
        preview_entries: dict[str, dict[str, Any]] = {}
        for address, name in functions:
            print(f"    [Fission] {address} {name}", flush=True)
            entry = run_fission_function(
                ROOT_DIR,
                binary_path,
                address,
                args.fission_bin,
                args.per_func_timeout,
                struct_ptr_aliases,
                engine="auto",
            )
            entry.setdefault("address", address)
            entry.setdefault("name", name)
            fission_entries[normalize_address(address)] = entry
            if not args.skip_preview_compare:
                print(f"    [Preview] {address} {name}", flush=True)
                preview = run_fission_function(
                    ROOT_DIR,
                    binary_path,
                    address,
                    args.fission_bin,
                    args.per_func_timeout,
                    struct_ptr_aliases,
                    engine="mlil-preview",
                )
                preview.setdefault("address", address)
                preview.setdefault("name", name)
                preview_entries[normalize_address(address)] = preview

        ghidra_entries: dict[str, dict[str, Any]] = {}
        ghidra_init_sec = 0.0
        if not args.skip_ghidra:
            ghidra_init_sec, ghidra_entries = run_ghidra_binary(
                binary_path,
                functions,
                args.ghidra_dir,
                args.per_func_timeout,
                struct_ptr_aliases,
            )
        else:
            for address, name in functions:
                ghidra_entries[normalize_address(address)] = {
                    "address": address,
                    "name": name,
                    "success": False,
                    "failure_kind": "skipped",
                    "decomp_sec": 0.0,
                }

        report = summarize_binary(binary_name, functions, fission_entries, ghidra_entries, preview_entries)
        report["ghidra_init_sec"] = round(ghidra_init_sec, 6)
        binary_reports.append(report)

        artifact = {
            "binary": str(binary_path),
            "summary": report,
            "functions": {
                "fission": fission_entries,
                "mlil_preview": preview_entries,
                "ghidra": ghidra_entries,
            },
        }
        (output_dir / f"{binary_name}_grand_finale.json").write_text(json.dumps(artifact, indent=2))

    final_report = {
        "binaries": binary_reports,
        "global": aggregate_global_report(binary_reports),
    }
    (output_dir / "grand_finale_summary.json").write_text(json.dumps(final_report, indent=2))
    write_markdown_report(final_report, output_dir / "grand_finale_summary.md")
    print(f"[+] Wrote report to {output_dir}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
