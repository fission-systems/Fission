#!/usr/bin/env python3
"""
extract_symbols.py — Extract function symbols and addresses from binaries.

Supports Mach-O, ELF, and PE binaries using platform-native tools (nm, objdump).
Outputs JSON suitable for suite YAML generation and PyGhidra batch input.

Usage:
    python3 extract_symbols.py <binary> [--json] [--filter-test]
    python3 extract_symbols.py --batch <dir_or_glob> [-o output.json]

Examples:
    python3 extract_symbols.py samples/macos/arm64/test_control_flow_arm64_O0
    python3 extract_symbols.py --batch "samples/**/*_O0" -o symbols.json
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


# ── Binary format detection ──────────────────────────────────────────────────

def detect_format(binary_path: str) -> str:
    """Detect binary format: 'macho', 'elf', 'pe', or 'unknown'."""
    try:
        result = subprocess.run(
            ["file", binary_path],
            capture_output=True, text=True, timeout=10,
        )
        output = result.stdout.lower()
        if "mach-o" in output:
            return "macho"
        if "elf" in output:
            return "elf"
        if "pe32" in output:
            return "pe"
    except Exception:
        pass
    # Fallback: check extension
    if binary_path.endswith(".exe"):
        return "pe"
    return "unknown"


def detect_arch(binary_path: str) -> str:
    """Detect architecture: 'x86_64', 'arm64', 'x86', or 'unknown'."""
    try:
        result = subprocess.run(
            ["file", binary_path],
            capture_output=True, text=True, timeout=10,
        )
        output = result.stdout.lower()
        if "arm64" in output or "aarch64" in output:
            return "arm64"
        if "x86-64" in output or "x86_64" in output:
            return "x86_64"
        if "80386" in output or "i386" in output:
            return "x86"
    except Exception:
        pass
    return "unknown"


# ── Symbol extraction per format ─────────────────────────────────────────────

def _extract_macho(binary_path: str) -> list[dict[str, Any]]:
    """Extract symbols from Mach-O binary using nm.

    macOS nm doesn't support --defined-only/--extern-only (GNU flags).
    We use plain `nm` and filter by symbol type T/t (text/code).
    Mach-O symbols are prefixed with an underscore that we strip.
    """
    symbols = []
    seen_addrs: set[int] = set()
    try:
        result = subprocess.run(
            ["nm", binary_path],
            capture_output=True, text=True, timeout=30,
        )
        for line in result.stdout.splitlines():
            # macOS nm format: "0000000100003f00 T __Z10div64_by_3y"
            # or "0000000100003f00 T _main"
            m = re.match(r"([0-9a-fA-F]+)\s+([TtSs])\s+_(.+)", line)
            if m:
                addr = int(m.group(1), 16)
                sym_type = m.group(2)
                name = m.group(3)
                if addr in seen_addrs:
                    continue
                seen_addrs.add(addr)
                entry: dict[str, Any] = {
                    "name": name, "address": addr, "address_hex": f"0x{addr:x}",
                }
                if sym_type in ("t", "s"):
                    entry["local"] = True
                symbols.append(entry)
    except Exception as e:
        print(f"Warning: nm failed for {binary_path}: {e}", file=sys.stderr)

    return sorted(symbols, key=lambda s: s["address"])


def _extract_elf(binary_path: str) -> list[dict[str, Any]]:
    """Extract symbols from ELF binary using nm or readelf."""
    symbols = []
    try:
        result = subprocess.run(
            ["nm", "--defined-only", binary_path],
            capture_output=True, text=True, timeout=30,
        )
        for line in result.stdout.splitlines():
            m = re.match(r"([0-9a-fA-F]+)\s+[Tt]\s+(.+)", line)
            if m:
                addr = int(m.group(1), 16)
                name = m.group(2)
                symbols.append({"name": name, "address": addr, "address_hex": f"0x{addr:x}"})
    except Exception as e:
        print(f"Warning: nm failed for {binary_path}: {e}", file=sys.stderr)

    # Fallback: try readelf (for cross-compiled binaries on macOS)
    if not symbols:
        for readelf_cmd in ("readelf", "x86_64-linux-musl-readelf", "llvm-readelf"):
            try:
                result = subprocess.run(
                    [readelf_cmd, "-s", "-W", binary_path],
                    capture_output=True, text=True, timeout=30,
                )
                for line in result.stdout.splitlines():
                    # Format: "    42: 0000000000401130    85 FUNC    GLOBAL DEFAULT   14 main"
                    m = re.match(
                        r"\s*\d+:\s+([0-9a-fA-F]+)\s+(\d+)\s+FUNC\s+"
                        r"(?:GLOBAL|LOCAL|WEAK)\s+\w+\s+\d+\s+(.+)",
                        line,
                    )
                    if m:
                        addr = int(m.group(1), 16)
                        size = int(m.group(2))
                        name = m.group(3).strip()
                        if addr > 0 and name and not name.startswith("$"):
                            symbols.append({
                                "name": name, "address": addr,
                                "address_hex": f"0x{addr:x}", "size": size,
                            })
                if symbols:
                    break
            except FileNotFoundError:
                continue
            except Exception:
                continue

    return sorted(symbols, key=lambda s: s["address"])


def _extract_pe(binary_path: str) -> list[dict[str, Any]]:
    """Extract symbols from PE (MinGW) binary using nm or objdump."""
    symbols = []

    # MinGW PE binaries retain symbol tables; nm works
    for nm_cmd in ("nm", "x86_64-w64-mingw32-nm", "llvm-nm"):
        try:
            result = subprocess.run(
                [nm_cmd, "--defined-only", binary_path],
                capture_output=True, text=True, timeout=30,
            )
            for line in result.stdout.splitlines():
                m = re.match(r"([0-9a-fA-F]+)\s+[Tt]\s+(.+)", line)
                if m:
                    addr = int(m.group(1), 16)
                    name = m.group(2)
                    symbols.append({"name": name, "address": addr, "address_hex": f"0x{addr:x}"})
            if symbols:
                break
        except FileNotFoundError:
            continue
        except Exception:
            continue

    # Fallback: objdump -t
    if not symbols:
        for objdump_cmd in ("objdump", "x86_64-w64-mingw32-objdump", "llvm-objdump"):
            try:
                result = subprocess.run(
                    [objdump_cmd, "-t", binary_path],
                    capture_output=True, text=True, timeout=30,
                )
                for line in result.stdout.splitlines():
                    m = re.match(
                        r"\[?\s*([0-9a-fA-F]+)\]?\s+.*\s+\.text\s+[0-9a-fA-F]+\s+(.+)",
                        line,
                    )
                    if m:
                        addr = int(m.group(1), 16)
                        name = m.group(2).strip()
                        if name and not name.startswith("."):
                            symbols.append({
                                "name": name, "address": addr,
                                "address_hex": f"0x{addr:x}",
                            })
                if symbols:
                    break
            except FileNotFoundError:
                continue
            except Exception:
                continue

    return sorted(symbols, key=lambda s: s["address"])


# ── Filtering ────────────────────────────────────────────────────────────────

# Known test function names from our test sources
TEST_FUNCTION_NAMES = {
    # test_arithmetic_idioms.cpp
    "divide_by_3", "divide_by_7", "divide_by_10", "divide_by_100",
    "signed_div_3", "signed_div_5", "signed_div_7",
    "mod_2", "mod_4", "mod_8", "mod_16", "mod_256",
    "multiply_by_2", "multiply_by_4", "multiply_by_8",
    "unsigned_div_2", "unsigned_div_4",
    "signed_mod_2", "signed_mod_4", "signed_mod_8",
    "absolute_value", "divmod", "negate_float", "negate_double",
    "complex_arith", "div64_by_3", "div64_by_1000",
    "multiply_by_3", "multiply_by_5", "multiply_by_7", "multiply_by_15",
    # test_control_flow.cpp
    "day_name", "sparse_switch", "classify_temperature",
    "sum_range", "count_digits", "find_first_set", "matrix_multiply",
    "read_until_sentinel", "count_set_bits_loop",
    "safe_divide", "clamp", "fatal_error", "process_command",
    "constant_condition_test", "memzero_manual", "fibonacci",
    # test_structs_classes.cpp
    "make_point", "rect_area", "rect_perimeter", "validate_header",
    "list_insert", "list_length", "list_sum", "list_free",
    "total_salary", "find_employee",
    # test_calling_conventions.cpp
    "one_param", "two_params", "three_params", "four_params",
    "six_params", "eight_params", "ten_params",
    "mixed_params", "float_params",
    "make_result", "make_small", "sum_variadic", "log_message",
    "apply_op", "add", "sub", "mul", "dispatch_op",
    "factorial_tail", "factorial", "process_array", "sum_callback",
    # test_string_memory.cpp
    "get_greeting", "my_strlen", "zero_buffer", "my_memcpy",
    "sb_init", "sb_append", "sb_free", "format_address",
    "classify_command", "count_words", "duplicate_string", "find_string",
    "popcount32", "parse_packet",
    # test_advanced_patterns.cpp
    "rotate_left", "rotate_right", "bswap32", "bswap16",
    "count_leading_zeros", "count_trailing_zeros",
    "next_token", "dispatch_operation",
    "handle_add", "handle_sub", "handle_mul", "handle_div",
    "handle_mod", "handle_and", "handle_or", "handle_xor",
    "fnv1a_hash", "murmur3_mix", "crc32_init", "crc32_compute",
    "get_flag", "set_flag", "clear_flag", "extract_bits", "insert_bits",
    "complex_expression", "multi_ternary", "min3", "max3",
    # test_real_world_algorithms.cpp
    "insertion_sort", "partition", "quicksort", "binary_search",
    "ht_hash", "ht_init", "ht_put", "ht_get",
    "rb_create", "rb_push", "rb_pop", "rb_free",
    "base64_encode", "parse_command", "execute_command",
    "matrix_transpose", "matrix_trace",
    # common
    "main",
}

# Symbols to skip (runtime/CRT internals)
SKIP_PATTERNS = [
    r"^_?__",                    # __libc, __cxa, __gmon, etc.
    r"^_?\.L",                   # .Lxxx labels
    r"^_?deregister_",           # deregister_tm_clones
    r"^_?register_",             # register_tm_clones
    r"^_?frame_dummy",
    r"^_?__do_global",
    r"^_?__libc",
    r"^_?_start$",
    r"^_?_init$",
    r"^_?_fini$",
    r"^_?_?crt",                 # CRT init
    r"^_?atexit",
    r"^_?exit$",
    r"^\$",                      # $x, $d labels (ARM)
    r"^_?GCC_except_table",
    r"^_?__tcf_",
    r"^_?__static_initialization",
    r"^_?_GLOBAL__",
    r"^_?\.hidden",
    r"^_?memset$",
    r"^_?memcpy$",
    r"^_?strlen$",
    r"^_?printf$",
    r"^_?puts$",
    r"^_?malloc$",
    r"^_?free$",
    r"^_?realloc$",
    r"^_?snprintf$",
    r"^_?strcmp$",
    r"^_?strncpy$",
    r"^_?strdup$",
    r"^_?calloc$",
    r"^_?fprintf$",
    r"^_?vsnprintf$",
    r"^_?abort$",
    r"^_?strtol$",
]


def filter_symbols(
    symbols: list[dict],
    *,
    test_only: bool = False,
    skip_crt: bool = True,
) -> list[dict]:
    """Filter symbols to remove CRT/runtime noise and optionally keep only test functions."""
    filtered = []
    for sym in symbols:
        name = sym["name"]
        # For matching, also check demangled name if available
        demangled_name = sym.get("demangled_name", "")
        clean = name.lstrip("_") if name.startswith("_") else name

        if skip_crt:
            skip = False
            for pat in SKIP_PATTERNS:
                if re.match(pat, name):
                    skip = True
                    break
            if skip:
                continue

        if test_only:
            # Check: raw name, stripped name, or demangled base name
            if clean not in TEST_FUNCTION_NAMES and demangled_name not in TEST_FUNCTION_NAMES:
                continue

        filtered.append(sym)
    return filtered


# ── C++ demangling ───────────────────────────────────────────────────────────

def demangle_symbols(symbols: list[dict]) -> list[dict]:
    """Attempt to demangle C++ symbols using c++filt.

    On macOS Mach-O, mangled names after stripping one underscore become _Z...
    but c++filt needs __Z... (the original nm form), so we prepend _ back.
    """
    mangled = [s for s in symbols if "_Z" in s["name"]]
    if not mangled:
        return symbols

    # Build filt_names: for _Z... names, prepend _ to get __Z... for macOS c++filt
    filt_names = []
    for s in mangled:
        name = s["name"]
        if name.startswith("_Z"):
            filt_names.append("_" + name)  # __Z... form
        else:
            filt_names.append(name)

    try:
        result = subprocess.run(
            ["c++filt"] + filt_names,
            capture_output=True, text=True, timeout=10,
        )
        demangled_lines = result.stdout.strip().splitlines()

        for i, sym in enumerate(mangled):
            if i >= len(demangled_lines):
                break
            dm = demangled_lines[i]
            orig = filt_names[i]
            if dm != orig:  # c++filt successfully demangled
                sym["demangled"] = dm
                m = re.match(r"([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)\s*\(", dm)
                if m:
                    sym["demangled_name"] = m.group(1)
    except Exception:
        pass

    return symbols


# ── Public API ───────────────────────────────────────────────────────────────

def extract_symbols(
    binary_path: str,
    *,
    test_only: bool = False,
    skip_crt: bool = True,
    demangle: bool = True,
) -> dict[str, Any]:
    """
    Extract symbols from a binary.

    Returns:
        {
            "binary": "/path/to/binary",
            "format": "macho|elf|pe",
            "arch": "x86_64|arm64|...",
            "symbols": [{name, address, address_hex, ...}, ...],
            "count": N,
        }
    """
    fmt = detect_format(binary_path)
    arch = detect_arch(binary_path)

    if fmt == "macho":
        raw_symbols = _extract_macho(binary_path)
    elif fmt == "elf":
        raw_symbols = _extract_elf(binary_path)
    elif fmt == "pe":
        raw_symbols = _extract_pe(binary_path)
    else:
        print(f"Warning: Unknown format for {binary_path}", file=sys.stderr)
        raw_symbols = []

    if demangle:
        raw_symbols = demangle_symbols(raw_symbols)

    symbols = filter_symbols(raw_symbols, test_only=test_only, skip_crt=skip_crt)

    return {
        "binary": str(binary_path),
        "format": fmt,
        "arch": arch,
        "symbols": symbols,
        "count": len(symbols),
    }


def extract_batch(
    paths: list[str],
    *,
    test_only: bool = False,
    skip_crt: bool = True,
) -> dict[str, Any]:
    """Extract symbols from multiple binaries. Returns a combined manifest."""
    results = {}
    total = 0
    for p in sorted(paths):
        info = extract_symbols(p, test_only=test_only, skip_crt=skip_crt)
        results[p] = info
        total += info["count"]
    return {"binaries": results, "total_symbols": total, "binary_count": len(results)}


# ── CLI ──────────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Extract function symbols from binaries for benchmarking",
    )
    parser.add_argument("binary", nargs="?", help="Path to a single binary")
    parser.add_argument("--batch", metavar="GLOB", help="Glob pattern for batch extraction")
    parser.add_argument("-o", "--output", metavar="FILE", help="Output JSON file")
    parser.add_argument("--json", action="store_true", help="JSON output (default for --batch)")
    parser.add_argument(
        "--test-only", action="store_true",
        help="Only include known test function names",
    )
    parser.add_argument(
        "--no-skip-crt", action="store_true",
        help="Don't skip CRT/runtime symbols",
    )
    args = parser.parse_args()

    if args.batch:
        paths = sorted(glob.glob(args.batch, recursive=True))
        paths = [p for p in paths if os.path.isfile(p)]
        if not paths:
            print(f"No files match pattern: {args.batch}", file=sys.stderr)
            return 1
        manifest = extract_batch(paths, test_only=args.test_only, skip_crt=not args.no_skip_crt)
        output = json.dumps(manifest, indent=2)
        if args.output:
            Path(args.output).write_text(output, encoding="utf-8")
            print(f"Wrote {manifest['binary_count']} binaries, {manifest['total_symbols']} symbols → {args.output}")
        else:
            print(output)
        return 0

    if not args.binary:
        parser.print_help()
        return 1

    info = extract_symbols(
        args.binary,
        test_only=args.test_only,
        skip_crt=not args.no_skip_crt,
    )

    if args.json:
        print(json.dumps(info, indent=2))
    else:
        print(f"Binary: {info['binary']}")
        print(f"Format: {info['format']}  Arch: {info['arch']}")
        print(f"Functions: {info['count']}")
        print()
        for sym in info["symbols"]:
            demangled = sym.get("demangled", "")
            dm_str = f"  → {demangled}" if demangled else ""
            local_str = " [local]" if sym.get("local") else ""
            print(f"  {sym['address_hex']:20s}  {sym['name']}{local_str}{dm_str}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
