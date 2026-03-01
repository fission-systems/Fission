#!/usr/bin/env python3
"""
generate_suites.py — Auto-generate benchmark suite YAML files.

Scans built binaries, extracts symbols, and combines with expected_patterns.yaml
to produce per-platform suite YAML files for benchmark_v4.py.

Usage:
    python3 generate_suites.py [--samples-dir DIR] [-o suites/]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

try:
    import yaml as _yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False

# Import from sibling module
sys.path.insert(0, str(Path(__file__).resolve().parent))
from extract_symbols import extract_symbols, TEST_FUNCTION_NAMES


def _project_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


# ── Source file → category mapping ───────────────────────────────────────────

SOURCE_CATEGORIES = {
    "test_arithmetic_idioms": "arithmetic",
    "test_control_flow": "control",
    "test_structs_classes": "structs",
    "test_calling_conventions": "calling",
    "test_string_memory": "strings",
    "test_advanced_patterns": "advanced",
    "test_real_world_algorithms": "algorithms",
}


# ── Expected patterns loader ────────────────────────────────────────────────

def load_expected_patterns(patterns_path: Path) -> dict[str, dict]:
    """
    Load expected_patterns.yaml → flat map: function_name → {patterns: [...]}
    """
    if not patterns_path.exists():
        print(f"Warning: {patterns_path} not found, using empty patterns", file=sys.stderr)
        return {}

    text = patterns_path.read_text(encoding="utf-8")
    if _YAML_AVAILABLE:
        data = _yaml.safe_load(text)
    else:
        # Minimal fallback parser for our simple YAML structure
        import re
        data = {}
        current_cat = None
        current_func = None
        for line in text.splitlines():
            if line.startswith("#") or not line.strip():
                continue
            cat_m = re.match(r"^(\w+):\s*$", line)
            if cat_m:
                current_cat = cat_m.group(1)
                data[current_cat] = {}
                continue
            func_m = re.match(r"^  (\w+):\s*$", line)
            if func_m and current_cat:
                current_func = func_m.group(1)
                data[current_cat][current_func] = {}
                continue
            pat_m = re.match(r'^\s+patterns:\s*\[(.+)\]\s*$', line)
            if pat_m and current_cat and current_func:
                raw = pat_m.group(1)
                patterns = [p.strip().strip('"').strip("'") for p in raw.split(",")]
                data[current_cat][current_func]["patterns"] = patterns

    # Flatten to func_name → {patterns, notes}
    flat: dict[str, dict] = {}
    for cat_name, funcs in data.items():
        if not isinstance(funcs, dict):
            continue
        for func_name, info in funcs.items():
            if isinstance(info, dict):
                flat[func_name] = info
    return flat


# ── Platform detection from path ─────────────────────────────────────────────

PLATFORM_MAP = {
    ("macos", "arm64"): {"platform": "macos", "arch": "arm64", "format": "macho"},
    ("macos", "x64"):   {"platform": "macos", "arch": "x86_64", "format": "macho"},
    ("linux", "x64"):   {"platform": "linux", "arch": "x86_64", "format": "elf"},
    ("windows", "x64"): {"platform": "windows", "arch": "x86_64", "format": "pe"},
}


def classify_binary(path: Path, samples_root: Path) -> dict[str, str] | None:
    """Classify a binary by platform/arch/source/opt from its path."""
    rel = path.relative_to(samples_root)
    parts = rel.parts  # e.g. ("macos", "arm64", "test_control_flow_arm64_O0")

    if len(parts) < 3:
        return None

    os_name = parts[0]   # macos, linux, windows
    arch_dir = parts[1]  # arm64, x64
    basename = path.stem  # remove .exe if present

    key = (os_name, arch_dir)
    if key not in PLATFORM_MAP:
        return None
    info = dict(PLATFORM_MAP[key])

    # Extract source name and optimization level from basename
    # Pattern: test_XXX_arm64_O0 or test_XXX_x64_O2
    for src_name in SOURCE_CATEGORIES:
        if basename.startswith(src_name):
            info["source"] = src_name
            info["category"] = SOURCE_CATEGORIES[src_name]
            # Extract opt level
            suffix = basename[len(src_name):]  # e.g. "_arm64_O0"
            if "_O0" in suffix:
                info["opt"] = "O0"
            elif "_O2" in suffix:
                info["opt"] = "O2"
            else:
                info["opt"] = "unknown"
            break
    else:
        return None

    return info


# ── Suite generation ─────────────────────────────────────────────────────────

def generate_suite_for_platform(
    platform_key: str,
    binaries: list[tuple[Path, dict]],
    patterns_map: dict[str, dict],
    samples_root: Path,
) -> dict[str, Any]:
    """Generate a single suite YAML structure for one platform."""
    suite: dict[str, Any] = {
        "name": f"benchmark-{platform_key}",
        "description": f"Auto-generated benchmark suite for {platform_key}",
        "binaries": [],
    }

    for bin_path, bin_info in sorted(binaries, key=lambda x: (x[1].get("source", ""), x[1].get("opt", ""))):
        # Extract symbols
        sym_data = extract_symbols(str(bin_path), test_only=True, skip_crt=True)

        if not sym_data["symbols"]:
            print(f"  Warning: no test symbols in {bin_path.name}", file=sys.stderr)
            continue

        # Build relative path from project root
        try:
            rel_path = bin_path.relative_to(_project_root())
        except ValueError:
            rel_path = bin_path

        binary_entry: dict[str, Any] = {
            "path": str(rel_path),
            "source": bin_info.get("source", "unknown"),
            "category": bin_info.get("category", "unknown"),
            "opt_level": bin_info.get("opt", "unknown"),
            "format": bin_info.get("format", "unknown"),
            "arch": bin_info.get("arch", "unknown"),
            "functions": [],
        }

        for sym in sym_data["symbols"]:
            func_name = sym["name"]
            # Use demangled base name if available, otherwise clean mangled name
            clean_name = sym.get("demangled_name", "")
            if not clean_name:
                clean_name = func_name.lstrip("_") if func_name.startswith("_") else func_name

            func_entry: dict[str, Any] = {
                "address": sym["address_hex"],
                "name": clean_name,
            }

            # Add expected patterns if available
            if clean_name in patterns_map:
                pats = patterns_map[clean_name].get("patterns", [])
                if pats:
                    func_entry["expected_patterns"] = pats

            binary_entry["functions"].append(func_entry)

        suite["binaries"].append(binary_entry)

    return suite


def discover_binaries(samples_dir: Path) -> list[tuple[Path, dict]]:
    """Find all test binaries and classify them."""
    results = []
    for platform_dir in ("macos", "linux", "windows"):
        pdir = samples_dir / platform_dir
        if not pdir.exists():
            continue
        for arch_dir in pdir.iterdir():
            if not arch_dir.is_dir():
                continue
            for f in arch_dir.iterdir():
                if not f.is_file():
                    continue
                # Skip non-test files
                if not f.name.startswith("test_"):
                    continue
                info = classify_binary(f, samples_dir)
                if info:
                    results.append((f, info))
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate benchmark suite YAML files")
    parser.add_argument(
        "--samples-dir", default=None,
        help="Path to samples/ directory (default: auto-detect)",
    )
    parser.add_argument(
        "-o", "--output-dir", default=None,
        help="Output directory for suite YAML files (default: scripts/benchmark/suites/)",
    )
    parser.add_argument(
        "--patterns", default=None,
        help="Path to expected_patterns.yaml (default: scripts/benchmark/expected_patterns.yaml)",
    )
    args = parser.parse_args()

    root = _project_root()
    samples_dir = Path(args.samples_dir) if args.samples_dir else root / "samples"
    output_dir = Path(args.output_dir) if args.output_dir else root / "scripts" / "benchmark" / "suites"
    patterns_path = Path(args.patterns) if args.patterns else root / "scripts" / "benchmark" / "expected_patterns.yaml"

    output_dir.mkdir(parents=True, exist_ok=True)

    # Load expected patterns
    patterns_map = load_expected_patterns(patterns_path)
    print(f"Loaded {len(patterns_map)} function patterns from {patterns_path.name}")

    # Discover binaries
    all_binaries = discover_binaries(samples_dir)
    print(f"Found {len(all_binaries)} test binaries in {samples_dir}")

    if not all_binaries:
        print("No test binaries found! Run scripts/build/build_test_binaries.sh first.")
        return 1

    # Group by platform_arch
    groups: dict[str, list] = {}
    for bin_path, bin_info in all_binaries:
        key = f"{bin_info['platform']}_{bin_info['arch']}"
        groups.setdefault(key, []).append((bin_path, bin_info))

    # Generate suite per platform
    total_funcs = 0
    for platform_key, binaries in sorted(groups.items()):
        suite = generate_suite_for_platform(platform_key, binaries, patterns_map, samples_dir)
        n_funcs = sum(len(b["functions"]) for b in suite["binaries"])
        total_funcs += n_funcs

        out_path = output_dir / f"suite_{platform_key}.yaml"
        if _YAML_AVAILABLE:
            with open(out_path, "w", encoding="utf-8") as f:
                _yaml.dump(suite, f, default_flow_style=False, allow_unicode=True, sort_keys=False)
        else:
            # Fallback: write as JSON
            out_path = out_path.with_suffix(".json")
            with open(out_path, "w", encoding="utf-8") as f:
                json.dump(suite, f, indent=2, ensure_ascii=False)

        print(f"  → {out_path.name}: {len(suite['binaries'])} binaries, {n_funcs} functions")

    # Also generate a combined "all" suite
    all_suite: dict[str, Any] = {
        "name": "benchmark-all",
        "description": "Combined benchmark suite — all platforms/architectures",
        "binaries": [],
    }
    for platform_key in sorted(groups.keys()):
        binaries = groups[platform_key]
        partial = generate_suite_for_platform(platform_key, binaries, patterns_map, samples_dir)
        all_suite["binaries"].extend(partial["binaries"])

    out_path = output_dir / "suite_all.yaml"
    if _YAML_AVAILABLE:
        with open(out_path, "w", encoding="utf-8") as f:
            _yaml.dump(all_suite, f, default_flow_style=False, allow_unicode=True, sort_keys=False)
    else:
        out_path = out_path.with_suffix(".json")
        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(all_suite, f, indent=2, ensure_ascii=False)

    print(f"\n  → {out_path.name}: {len(all_suite['binaries'])} binaries, {total_funcs} functions (combined)")
    print(f"\nDone. {len(groups)} platform suites + 1 combined suite generated.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
