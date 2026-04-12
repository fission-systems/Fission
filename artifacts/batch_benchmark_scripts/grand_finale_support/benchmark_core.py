#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

try:
    from rapidfuzz import fuzz as _rapidfuzz_fuzz
except Exception:
    _rapidfuzz_fuzz = None


def _safe_float(value: Any, default: float = 0.0) -> float:
    try:
        if value is None:
            return default
        return float(value)
    except (TypeError, ValueError):
        return default


def _safe_int(value: Any, default: int = 0) -> int:
    try:
        if value is None:
            return default
        return int(value)
    except (TypeError, ValueError):
        return default


def _lookup_path(data: dict[str, Any], path: tuple[str, ...], default: Any = None) -> Any:
    cur: Any = data
    for key in path:
        if not isinstance(cur, dict):
            return default
        cur = cur.get(key)
        if cur is None:
            return default
    return cur


def _canonical_indirect_flags(preview_build_stats: dict[str, Any]) -> dict[str, Any]:
    unsupported_indirect = _safe_int(
        preview_build_stats.get("unsupported_indirect_control_count"), 0
    )
    unsupported_call = _safe_int(
        preview_build_stats.get("unsupported_indirect_call_count"), 0
    )
    unsupported_external = _safe_int(
        preview_build_stats.get("unsupported_external_target_count"), 0
    )
    preserved = _safe_int(
        preview_build_stats.get("indirect_surface_preserved_count"), 0
    )
    dispatcher = _safe_int(
        preview_build_stats.get("dispatcher_shape_recovered_count"), 0
    )
    refined = _safe_int(
        preview_build_stats.get("indirect_target_set_refined_count"), 0
    )
    return {
        "has_unresolved_unsupported_indirect": unsupported_indirect > 0
        or unsupported_call > 0
        or unsupported_external > 0,
        "has_preserved_indirect_surface": preserved > 0,
        "has_dispatcher_recovery": dispatcher > 0,
        "has_indirect_target_proof": refined > 0,
    }

def find_repo_root() -> Path:
    current = Path(__file__).resolve()
    for parent in [current.parent, *current.parents]:
        if (parent / "Cargo.toml").exists() and (parent / ".git").exists():
            return parent
    raise RuntimeError("Could not locate repository root from script path")


ROOT_DIR = find_repo_root()
DEFAULT_RESULTS_DIR = ROOT_DIR / "artifacts" / "batch_benchmark"
DEFAULT_GHIDRA_DIRS = (
    ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC",
    ROOT_DIR / "ghidra_11.4.2_PUBLIC",
)
BASE_TYPES_JSON = ROOT_DIR / "crates" / "fission-signatures" / "data" / "win_types" / "base_types.json"

BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
LINE_COMMENT_RE = re.compile(r"//.*?$", re.MULTILINE)
HEX_RE = re.compile(r"0x[0-9a-fA-F]+")
AUTO_FUNC_RE = re.compile(r"\b(?:FUN|sub)_[0-9a-fA-F]+\b")
AUTO_SYMBOL_RE = re.compile(r"\b(?:DAT|LAB|UNK|EXT)_[0-9a-fA-F]+\b")
AUTO_VAR_RE = re.compile(
    r"\b(?:local|param|extraout|in|unaff|uStack|puStack|iVar|uVar|bVar|cVar|sVar|lVar|auStack)"
    r"[A-Za-z0-9_]*\b"
)
SYNTHETIC_FAILURE_PREFIX = "// Decompilation failed:"
SIMILARITY_BACKEND = "rapidfuzz" if _rapidfuzz_fuzz is not None else "difflib"

ROW_FIDELITY_TARGETS: tuple[tuple[str, str], ...] = (
    ("0x140001160", "primary"),
    ("0x140008900", "secondary"),
    ("0x140007da0", "canary"),
    ("0x140008090", "canary"),
    ("0x140006ef0", "canary"),
    ("0x140006c20", "canary"),
    ("0x140006fe0", "canary"),
)


def select_row_fidelity_targets(role_filter: str) -> list[tuple[str, str]]:
    if role_filter == "all":
        return list(ROW_FIDELITY_TARGETS)
    if role_filter == "canary-only":
        return [
            (address, role)
            for address, role in ROW_FIDELITY_TARGETS
            if role == "canary"
        ]
    if role_filter == "primary-secondary-only":
        return [
            (address, role)
            for address, role in ROW_FIDELITY_TARGETS
            if role in ("primary", "secondary")
        ]
    return list(ROW_FIDELITY_TARGETS)

from .metrics import collect_code_metrics, load_struct_pointer_aliases, normalize_address
from .resource_monitor import (
    HAS_PSUTIL,
    collect_macos_activity_snapshot,
    run_popen_with_resource_monitor,
    start_self_resource_monitor,
    summarize_macos_activity_delta,
)
from .runners import list_functions_with_fission, run_ghidra_binary_with_meta, sample_functions


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Benchmark whole-binary decompilation quality and speed between "
            "Fission and Ghidra (pyghidra)."
        )
    )
    parser.add_argument("binary", type=Path, help="Path to the target binary")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help=(
            "Directory to write benchmark artifacts into. "
            "If omitted, a fixed per-binary folder is used: "
            "artifacts/batch_benchmark/<binary>-latest"
        ),
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=None,
        help="Path to Ghidra installation directory",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=None,
        help="Path to a prebuilt fission_cli binary",
    )
    parser.add_argument(
        "--profile",
        choices=("balanced", "quality", "speed"),
        default="balanced",
        help="Fission decompiler profile",
    )
    parser.add_argument(
        "--function-discovery-profile",
        choices=("conservative", "balanced", "aggressive"),
        default="balanced",
        help="Fission function discovery profile",
    )
    parser.add_argument(
        "--compiler-id",
        default=None,
        help="Optional Fission compiler ID override (auto|windows|gcc|clang|default)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=1800,
        help="Process timeout in seconds for each whole-binary run",
    )
    parser.add_argument(
        "--ghidra-func-timeout",
        type=int,
        default=60,
        help="Per-function decompile timeout for Ghidra",
    )
    parser.add_argument(
        "--skip-thunks",
        action="store_true",
        help="Skip thunk functions on the Ghidra side",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        metavar="N",
        help="Decompile only first N functions (for faster validation)",
    )
    parser.add_argument(
        "--pairwise-similarity-mode",
        choices=("auto", "full", "shared-full", "sampled"),
        default="auto",
        help=(
            "Pairwise similarity workload mode: auto (choose by shared size), "
            "full/shared-full (all shared functions), sampled (deterministic subset)."
        ),
    )
    parser.add_argument(
        "--pairwise-sample-size",
        type=int,
        default=5000,
        help="Maximum shared functions to compare when pairwise mode is sampled/auto.",
    )
    parser.add_argument(
        "--pairwise-auto-shared-full-max",
        type=int,
        default=2000,
        help=(
            "When --pairwise-similarity-mode=auto, use shared-full only if shared_count <= N. "
            "Default: 2000 (larger sets switch to sampled)."
        ),
    )
    parser.add_argument(
        "--raw-similarity",
        action="store_true",
        help=(
            "Also compute raw (non-normalized) code similarity. "
            "Disabled by default to reduce pairwise CPU cost."
        ),
    )
    parser.add_argument(
        "--aggregate-similarity-mode",
        choices=("weighted", "sequence"),
        default="weighted",
        help=(
            "How to compute aggregate normalized similarity: "
            "weighted (fast weighted mean, default) or sequence (slow full-string matcher)."
        ),
    )

    # ── Ghidra output cache ───────────────────────────────────────────────────
    parser.add_argument(
        "--ghidra-cache-dir",
        type=Path,
        default=None,
        help="Directory for storing/loading cached Ghidra output JSON files.",
    )
    parser.add_argument(
        "--save-ghidra-cache",
        action="store_true",
        help="Save Ghidra output to the cache directory after a live run.",
    )
    parser.add_argument(
        "--use-ghidra-cache",
        action="store_true",
        help="Load Ghidra output from cache instead of running pyghidra.",
    )

    # ── Auto function limit ───────────────────────────────────────────────────
    parser.add_argument(
        "--auto-limit-functions",
        type=int,
        default=None,
        metavar="N",
        help=(
            "If the binary has more than N functions, automatically apply "
            "--limit N to prevent Ghidra from timing out on large binaries."
        ),
    )

    # ── Regression gate ───────────────────────────────────────────────────────
    parser.add_argument(
        "--baseline-dir",
        type=Path,
        default=None,
        help=(
            "Path to a previous benchmark output directory containing "
            "benchmark_summary.json. Used to detect regressions."
        ),
    )
    parser.add_argument(
        "--regression-threshold",
        type=float,
        default=2.0,
        metavar="PP",
        help=(
            "Allowed drop in avg_normalized_similarity (percentage points) "
            "before the run is considered a regression. Default: 2.0 pp."
        ),
    )
    parser.add_argument(
        "--row-fidelity-role-filter",
        choices=("all", "canary-only", "primary-secondary-only"),
        default="all",
        help=(
            "Filter row-fidelity target roles for preflight/acceptance reporting. "
            "all=use full target set, canary-only=only canary rows, "
            "primary-secondary-only=only primary and secondary rows."
        ),
    )

    parser.add_argument(
        "--timestamped-output",
        action="store_true",
        help=(
            "Use timestamped output folder naming (legacy behavior). "
            "Default is fixed folder naming for stable latest reports."
        ),
    )

    return parser.parse_args()


def resolve_binary(path: Path) -> Path:
    binary = path.expanduser().resolve()
    if not binary.is_file():
        raise FileNotFoundError(f"binary not found: {binary}")
    return binary


# ── Ghidra output cache helpers ───────────────────────────────────────────────

def _binary_sha256_prefix(binary_path: Path, n: int = 8) -> str:
    """Return first `n` hex chars of the binary's SHA-256 hash."""
    h = hashlib.sha256()
    with binary_path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()[:n]


def _ghidra_version_tag(ghidra_dir: Path) -> str:
    """Extract a short version string from the Ghidra installation directory name."""
    name = ghidra_dir.name  # e.g. "ghidra_11.4.2_PUBLIC"
    # Keep only alphanumeric/dot/underscore segments
    return re.sub(r"[^A-Za-z0-9._]", "_", name)[:24]


def resolve_ghidra_cache_path(
    cache_dir: Path,
    binary_path: Path,
    ghidra_dir: Path,
    limit: int | None,
) -> Path:
    sha = _binary_sha256_prefix(binary_path)
    ver = _ghidra_version_tag(ghidra_dir)
    limit_tag = f"limit{limit}" if limit is not None else "full"
    filename = f"{binary_path.stem}_{ver}_{sha}_{limit_tag}.json"
    return cache_dir / filename


def load_ghidra_cache(cache_path: Path) -> dict[str, Any]:
    with cache_path.open("r", encoding="utf-8") as fh:
        payload = json.load(fh)
    meta = payload.get("_meta", {})
    meta["from_cache"] = True
    meta["cache_path"] = str(cache_path)
    entries: dict[str, dict[str, Any]] = {}
    for entry in payload.get("functions", []):
        address = canonical_address(entry.get("address", "0x0"))
        entries[address] = entry
        entries[address].setdefault("address", address)
    return {"meta": meta, "entries": entries}


def save_ghidra_cache(cache_path: Path, raw_output_path: Path) -> None:
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(raw_output_path, cache_path)


# ── Function-count helper for auto-limit ─────────────────────────────────────

def count_binary_functions(binary_path: Path, fission_bin: Path) -> int | None:
    """Return the number of functions in the binary via fission_cli --list."""
    try:
        result = subprocess.run(
            [str(fission_bin), str(binary_path), "--list"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
            check=False,
        )
        count = sum(
            1
            for line in result.stdout.splitlines()
            if len(line.split()) >= 3 and line.split()[0].startswith("0x")
        )
        return count if count > 0 else None
    except Exception:
        return None


def resolve_effective_limit(
    binary_path: Path,
    fission_bin: Path,
    explicit_limit: int | None,
    auto_limit: int | None,
) -> int | None:
    """
    Return the effective function limit to use.

    Priority:
      1. Explicit --limit N always wins.
      2. --auto-limit-functions N: apply if function count exceeds N.
      3. No limit.
    """
    if explicit_limit is not None:
        return explicit_limit
    if auto_limit is None:
        return None
    func_count = count_binary_functions(binary_path, fission_bin)
    if func_count is not None and func_count > auto_limit:
        print(
            f"[*] Auto-limit: binary has {func_count} functions > {auto_limit}; "
            f"applying --limit {auto_limit}"
        )
        return auto_limit
    return None


# ── Regression gate ───────────────────────────────────────────────────────────

def load_baseline_summary(baseline_dir: Path) -> dict[str, Any] | None:
    candidate = baseline_dir / "benchmark_summary.json"
    if not candidate.is_file():
        print(f"[!] No benchmark_summary.json found in baseline dir: {baseline_dir}", file=sys.stderr)
        return None
    with candidate.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def _pairwise_row_map(summary_payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    rows = _lookup_path(
        summary_payload,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    if not isinstance(rows, list):
        return {}
    return {
        str(row.get("address", "")): row
        for row in rows
        if isinstance(row, dict) and row.get("address")
    }


def _engine_entries_map(summary_payload: dict[str, Any], engine: str) -> dict[str, Any]:
    entries = _lookup_path(summary_payload, ("engines", engine, "entries"), {})
    return entries if isinstance(entries, dict) else {}


def _build_row_fidelity_gate(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    row_targets: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    cur_rows_map = _pairwise_row_map(current_benchmark)
    base_rows_map = _pairwise_row_map(baseline_summary_json)
    cur_fission_entries = _engine_entries_map(current_benchmark, "fission")
    base_fission_entries = _engine_entries_map(baseline_summary_json, "fission")

    row_results: list[dict[str, Any]] = []
    improved = 0
    degraded = 0
    unchanged = 0
    missing = 0
    failed_targets: list[str] = []

    targets = row_targets or list(ROW_FIDELITY_TARGETS)

    for address, role in targets:
        cur_row = cur_rows_map.get(address)
        base_row = base_rows_map.get(address)
        if cur_row is None or base_row is None:
            status = "missing"
            missing += 1
            failed_targets.append(address)
            row_results.append(
                {
                    "address": address,
                    "role": role,
                    "status": status,
                    "present_in_current": cur_row is not None,
                    "present_in_baseline": base_row is not None,
                    "failure_reasons": ["missing_row"],
                }
            )
            continue

        cur_norm = _safe_float(cur_row.get("normalized_similarity", 0.0), 0.0)
        base_norm = _safe_float(base_row.get("normalized_similarity", 0.0), 0.0)
        delta = cur_norm - base_norm

        cur_fission = cur_fission_entries.get(address, {})
        base_fission = base_fission_entries.get(address, {})
        cur_stats = cur_fission.get("preview_build_stats") if isinstance(cur_fission, dict) else {}
        base_stats = base_fission.get("preview_build_stats") if isinstance(base_fission, dict) else {}
        if not isinstance(cur_stats, dict):
            cur_stats = {}
        if not isinstance(base_stats, dict):
            base_stats = {}

        failure_reasons: list[str] = []
        cur_success = bool(cur_row.get("fission_success", False))
        base_success = bool(base_row.get("fission_success", False))
        if base_success and not cur_success:
            failure_reasons.append("new_failure")
        if delta < -1e-9:
            failure_reasons.append("similarity_regressed")

        if failure_reasons:
            status = "degraded"
            degraded += 1
            failed_targets.append(address)
        elif delta > 1e-9:
            status = "improved"
            improved += 1
        else:
            status = "unchanged"
            unchanged += 1

        row_results.append(
            {
                "address": address,
                "role": role,
                "status": status,
                "failure_reasons": failure_reasons,
                "pyghidra_name": cur_row.get("pyghidra_name") or base_row.get("pyghidra_name") or "",
                "fission_name": cur_row.get("fission_name") or base_row.get("fission_name") or "",
                "previous_normalized_similarity": round(base_norm, 3),
                "current_normalized_similarity": round(cur_norm, 3),
                "normalized_similarity_delta": round(delta, 3),
                "previous_fission_success": base_success,
                "current_fission_success": cur_success,
                "previous_unsupported_indirect_control_count": _safe_int(
                    base_stats.get("unsupported_indirect_control_count"), 0
                ),
                "current_unsupported_indirect_control_count": _safe_int(
                    cur_stats.get("unsupported_indirect_control_count"), 0
                ),
                "previous_indirect_surface_preserved_count": _safe_int(
                    base_stats.get("indirect_surface_preserved_count"), 0
                ),
                "current_indirect_surface_preserved_count": _safe_int(
                    cur_stats.get("indirect_surface_preserved_count"), 0
                ),
                "previous_dispatcher_shape_recovered_count": _safe_int(
                    base_stats.get("dispatcher_shape_recovered_count"), 0
                ),
                "current_dispatcher_shape_recovered_count": _safe_int(
                    cur_stats.get("dispatcher_shape_recovered_count"), 0
                ),
            }
        )

    return {
        "status": "failed" if failed_targets else "passed",
        "failed_target_count": len(failed_targets),
        "failed_targets": failed_targets,
        "improved_count": improved,
        "degraded_count": degraded,
        "unchanged_count": unchanged,
        "missing_count": missing,
        "rows": row_results,
    }


def _build_baseline_regression_report(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    threshold_pp: float,
    row_targets: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    regressions: list[str] = []

    cur_quality = (
        current_benchmark
        .get("summary", {})
        .get("quality", {})
        .get("pyghidra_vs_fission", {})
    )
    base_quality = (
        baseline_summary_json
        .get("summary", {})
        .get("quality", {})
        .get("pyghidra_vs_fission", {})
    )

    cur_sim = float(cur_quality.get("avg_normalized_similarity", 0.0) or 0.0)
    base_sim = float(base_quality.get("avg_normalized_similarity", 0.0) or 0.0)
    sim_delta = cur_sim - base_sim
    if sim_delta < -threshold_pp:
        regressions.append(
            f"avg_norm_sim: {base_sim:.2f}% → {cur_sim:.2f}% "
            f"(Δ={sim_delta:.2f}pp, threshold={-threshold_pp:.2f}pp)"
        )

    cur_both_success = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("both_success_rate_pct", 0.0) or 0.0
    )
    base_both_success = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("both_success_rate_pct", 0.0) or 0.0
    )
    both_success_delta = cur_both_success - base_both_success
    if both_success_delta < -1.0:
        regressions.append(
            f"both_success_rate: {base_both_success:.3f}% -> {cur_both_success:.3f}% "
            f"(Δ={both_success_delta:.3f}pp, threshold=-1.000pp)"
        )

    cur_high_div = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("high_divergence_pct", 0.0) or 0.0
    )
    base_high_div = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("intersection", {})
        .get("pyghidra_vs_fission", {})
        .get("high_divergence_pct", 0.0) or 0.0
    )
    high_div_delta = cur_high_div - base_high_div
    if high_div_delta > 2.0:
        regressions.append(
            f"high_divergence_pct: {base_high_div:.3f}% -> {cur_high_div:.3f}% "
            f"(Δ={high_div_delta:.3f}pp, threshold=+2.000pp)"
        )

    cur_succ = float(
        current_benchmark.get("summary", {})
        .get("kpi", {})
        .get("engines", {})
        .get("fission", {})
        .get("quality_kpi", {})
        .get("success_rate_pct", 100.0) or 100.0
    )
    base_succ = float(
        baseline_summary_json.get("summary", {})
        .get("kpi", {})
        .get("engines", {})
        .get("fission", {})
        .get("quality_kpi", {})
        .get("success_rate_pct", 100.0) or 100.0
    )
    if cur_succ - base_succ < -1.0:
        regressions.append(
            f"fission success_rate: {base_succ:.3f}% → {cur_succ:.3f}% "
            f"(Δ={cur_succ - base_succ:.3f}pp)"
        )

    cur_goto = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("goto_total", 0) or 0
    )
    base_goto = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("goto_total", 0) or 0
    )
    if base_goto > 0 and cur_goto > base_goto * 1.10:
        regressions.append(
            f"fission goto_total: {base_goto} → {cur_goto} "
            f"(+{((cur_goto - base_goto) / base_goto * 100):.1f}%, threshold=10%)"
        )

    cur_readability_penalty = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("readability_control_flow_penalty", 0) or 0
    )
    base_readability_penalty = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("readability_control_flow_penalty", 0) or 0
    )
    if (
        base_readability_penalty > 0
        and cur_readability_penalty > int(base_readability_penalty * 1.10)
    ):
        regressions.append(
            f"fission readability_control_flow_penalty: {base_readability_penalty} -> {cur_readability_penalty} "
            f"(+{((cur_readability_penalty - base_readability_penalty) / base_readability_penalty * 100):.1f}%, threshold=10%)"
        )

    cur_undef_total = int(
        current_benchmark.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("undefined_return_type_total", 0) or 0
    )
    base_undef_total = int(
        baseline_summary_json.get("summary", {})
        .get("engines", {})
        .get("fission", {})
        .get("undefined_return_type_total", 0) or 0
    )
    cur_success_count = max(
        int(
            current_benchmark.get("summary", {})
            .get("engines", {})
            .get("fission", {})
            .get("success_count", 0) or 0
        ),
        1,
    )
    base_success_count = max(
        int(
            baseline_summary_json.get("summary", {})
            .get("engines", {})
            .get("fission", {})
            .get("success_count", 0) or 0
        ),
        1,
    )
    cur_undef_rate = (cur_undef_total / cur_success_count) * 100.0
    base_undef_rate = (base_undef_total / base_success_count) * 100.0
    undef_delta = cur_undef_rate - base_undef_rate
    if undef_delta > 5.0:
        regressions.append(
            f"fission undefined_return_type_rate: {base_undef_rate:.3f}% -> {cur_undef_rate:.3f}% "
            f"(Δ={undef_delta:.3f}pp, threshold=+5.000pp)"
        )
    cur_unsupported_indirect = _safe_int(
        _lookup_path(
            current_benchmark,
            ("summary", "engines", "fission", "unsupported_indirect_control_count"),
            0,
        ),
        0,
    )
    base_unsupported_indirect = _safe_int(
        _lookup_path(
            baseline_summary_json,
            ("summary", "engines", "fission", "unsupported_indirect_control_count"),
            0,
        ),
        0,
    )
    if cur_unsupported_indirect > base_unsupported_indirect:
        regressions.append(
            f"fission unsupported_indirect_control_count: {base_unsupported_indirect} -> {cur_unsupported_indirect}"
        )

    row_fidelity_gate = _build_row_fidelity_gate(
        current_benchmark,
        baseline_summary_json,
        row_targets=row_targets,
    )
    if row_fidelity_gate.get("status") != "passed":
        regressions.append(
            "row_fidelity_gate failed for "
            + ", ".join(str(addr) for addr in row_fidelity_gate.get("failed_targets", []))
        )

    return {
        "status": "failed" if regressions else "passed",
        "regressions": regressions,
        "threshold_pp": round(float(threshold_pp), 3),
        "row_fidelity_gate": row_fidelity_gate,
        "top_degraded_functions": collect_top_degraded_functions_vs_previous(
            current_benchmark,
            baseline_summary_json,
            limit=20,
            similarity_drop_pp_threshold=0.0,
        ),
    }


def check_regression(
    current_benchmark: dict[str, Any],
    baseline_summary_json: dict[str, Any],
    threshold_pp: float,
    row_targets: list[tuple[str, str]] | None = None,
) -> bool:
    """
    Compare current run against baseline.  Returns True if a regression is detected.
    """
    report = _build_baseline_regression_report(
        current_benchmark,
        baseline_summary_json,
        threshold_pp,
        row_targets=row_targets,
    )
    if report.get("status") != "passed":
        print("\n[REGRESSION DETECTED]", file=sys.stderr)
        for msg in report.get("regressions", []):
            print(f"  - {msg}", file=sys.stderr)
        return True

    print("[*] Regression check passed — no significant degradation detected.")
    return False


def extract_snapshot_metrics(summary_payload: dict[str, Any]) -> dict[str, float]:
    fission_success_count = _safe_int(
        _lookup_path(summary_payload, ("summary", "engines", "fission", "success_count"), 0),
        0,
    )
    fission_undefined_return_total = _safe_int(
        _lookup_path(summary_payload, ("summary", "engines", "fission", "undefined_return_type_total"), 0),
        0,
    )
    fission_undefined_return_rate = (
        (fission_undefined_return_total / max(fission_success_count, 1)) * 100.0
    )

    metrics: dict[str, float] = {
        "avg_normalized_similarity_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "quality", "pyghidra_vs_fission", "avg_normalized_similarity"), 0.0),
            0.0,
        ),
        "both_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "intersection", "pyghidra_vs_fission", "both_success_rate_pct"), 0.0),
            0.0,
        ),
        "high_divergence_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "intersection", "pyghidra_vs_fission", "high_divergence_pct"), 0.0),
            0.0,
        ),
        "fission_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "quality_kpi", "success_rate_pct"), 0.0),
            0.0,
        ),
        "fission_direct_success_rate_pct": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "quality_kpi", "direct_success_rate_pct"), 0.0),
            0.0,
        ),
        "wall_speedup_vs_pyghidra": _safe_float(
            _lookup_path(summary_payload, ("summary", "speed", "fission", "wall_speedup_vs_pyghidra"), 0.0),
            0.0,
        ),
        "fission_throughput_func_per_sec": _safe_float(
            _lookup_path(summary_payload, ("summary", "kpi", "engines", "fission", "performance_kpi", "throughput_func_per_sec"), 0.0),
            0.0,
        ),
        "fission_wall_sec": _safe_float(
            _lookup_path(summary_payload, ("summary", "speed", "fission", "wall_sec"), 0.0),
            0.0,
        ),
        "fission_goto_total": float(
            _safe_int(_lookup_path(summary_payload, ("summary", "engines", "fission", "goto_total"), 0), 0)
        ),
        "fission_readability_control_flow_penalty": float(
            _safe_int(_lookup_path(summary_payload, ("summary", "engines", "fission", "readability_control_flow_penalty"), 0), 0)
        ),
        "fission_undefined_return_type_total": float(
            fission_undefined_return_total
        ),
        "fission_undefined_return_type_rate_pct": round(fission_undefined_return_rate, 6),
    }
    return metrics


def _anti_pattern_total(metrics: dict[str, Any]) -> int:
    anti = (metrics or {}).get("anti_pattern_counts") or {}
    return sum(int(v or 0) for v in anti.values())


def collect_top_degraded_functions_vs_previous(
    current_benchmark: dict[str, Any],
    previous_summary_payload: dict[str, Any],
    limit: int = 20,
    similarity_drop_pp_threshold: float = 1.0,
) -> dict[str, Any]:
    cur_rows = _lookup_path(
        current_benchmark,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    prev_rows = _lookup_path(
        previous_summary_payload,
        ("pairwise", "pyghidra_vs_fission", "comparisons"),
        [],
    )
    cur_rows_map = {
        str(row.get("address", "")): row
        for row in cur_rows
        if isinstance(row, dict) and row.get("address")
    }
    prev_rows_map = {
        str(row.get("address", "")): row
        for row in prev_rows
        if isinstance(row, dict) and row.get("address")
    }

    cur_fission_entries = _lookup_path(current_benchmark, ("engines", "fission", "entries"), {})
    prev_fission_entries = _lookup_path(previous_summary_payload, ("engines", "fission", "entries"), {})
    if not isinstance(cur_fission_entries, dict):
        cur_fission_entries = {}
    if not isinstance(prev_fission_entries, dict):
        prev_fission_entries = {}

    degraded_rows: list[dict[str, Any]] = []
    for address in sorted(set(cur_rows_map) & set(prev_rows_map), key=lambda a: int(a, 16)):
        cur_row = cur_rows_map[address]
        prev_row = prev_rows_map[address]

        cur_norm = _safe_float(cur_row.get("normalized_similarity", 0.0), 0.0)
        prev_norm = _safe_float(prev_row.get("normalized_similarity", 0.0), 0.0)
        delta = cur_norm - prev_norm
        if delta >= -abs(similarity_drop_pp_threshold):
            continue

        cur_fission = cur_fission_entries.get(address, {}) if isinstance(cur_fission_entries.get(address, {}), dict) else {}
        prev_fission = prev_fission_entries.get(address, {}) if isinstance(prev_fission_entries.get(address, {}), dict) else {}
        cur_metrics = cur_fission.get("metrics") if isinstance(cur_fission.get("metrics"), dict) else {}
        prev_metrics = prev_fission.get("metrics") if isinstance(prev_fission.get("metrics"), dict) else {}

        reasons: list[str] = []

        prev_success = bool(prev_fission.get("success", prev_row.get("fission_success", False)))
        cur_success = bool(cur_fission.get("success", cur_row.get("fission_success", False)))
        if prev_success and not cur_success:
            reasons.append("new_failure")

        if bool(cur_fission.get("fell_back", False)) and not bool(prev_fission.get("fell_back", False)):
            reasons.append("new_fallback")

        goto_delta = _safe_int(cur_metrics.get("goto_count", 0), 0) - _safe_int(prev_metrics.get("goto_count", 0), 0)
        if goto_delta > 0:
            reasons.append("goto_increase")

        undef_delta = int(bool(cur_metrics.get("undefined_return_type", False))) - int(bool(prev_metrics.get("undefined_return_type", False)))
        if undef_delta > 0:
            reasons.append("new_undefined_return")

        unknown_type_var_delta = _safe_int(cur_metrics.get("unknown_type_var_count", 0), 0) - _safe_int(prev_metrics.get("unknown_type_var_count", 0), 0)
        if unknown_type_var_delta > 0:
            reasons.append("unknown_type_var_increase")

        anti_delta = _anti_pattern_total(cur_metrics) - _anti_pattern_total(prev_metrics)
        if anti_delta > 0:
            reasons.append("anti_pattern_increase")

        if not reasons:
            reasons.append("semantic_divergence")

        degraded_rows.append(
            {
                "address": address,
                "pyghidra_name": cur_row.get("pyghidra_name") or prev_row.get("pyghidra_name") or "",
                "fission_name": cur_row.get("fission_name") or prev_row.get("fission_name") or "",
                "previous_normalized_similarity": round(prev_norm, 3),
                "current_normalized_similarity": round(cur_norm, 3),
                "normalized_similarity_delta": round(delta, 3),
                "previous_fission_success": prev_success,
                "current_fission_success": cur_success,
                "goto_delta": goto_delta,
                "unknown_type_var_delta": unknown_type_var_delta,
                "anti_pattern_delta": anti_delta,
                "reason_tags": reasons,
            }
        )

    degraded_rows.sort(
        key=lambda row: (
            row.get("normalized_similarity_delta", 0.0),
            -len(row.get("reason_tags", [])),
            row.get("address", ""),
        )
    )
    return {
        "similarity_drop_pp_threshold": round(abs(similarity_drop_pp_threshold), 3),
        "degraded_function_count": len(degraded_rows),
        "top_degraded": degraded_rows[: max(limit, 1)],
    }


def compare_with_previous_summary(
    current_benchmark: dict[str, Any],
    previous_summary_payload: dict[str, Any],
) -> dict[str, Any]:
    previous_metrics = extract_snapshot_metrics(previous_summary_payload)
    current_metrics = extract_snapshot_metrics(current_benchmark)

    metric_specs = [
        ("avg_normalized_similarity_pct", "avg normalized similarity (%)", "higher_is_better"),
        ("both_success_rate_pct", "both-success rate (%)", "higher_is_better"),
        ("high_divergence_pct", "high divergence (%)", "lower_is_better"),
        ("fission_success_rate_pct", "fission success rate (%)", "higher_is_better"),
        ("fission_direct_success_rate_pct", "fission direct success rate (%)", "higher_is_better"),
        ("wall_speedup_vs_pyghidra", "wall speedup vs pyghidra (x)", "higher_is_better"),
        ("fission_throughput_func_per_sec", "fission throughput (func/s)", "higher_is_better"),
        ("fission_wall_sec", "fission wall time (s)", "lower_is_better"),
        ("fission_goto_total", "fission goto total", "lower_is_better"),
        (
            "fission_readability_control_flow_penalty",
            "fission readability control-flow penalty",
            "lower_is_better",
        ),
        (
            "fission_undefined_return_type_total",
            "fission undefined return type total",
            "lower_is_better",
        ),
        (
            "fission_undefined_return_type_rate_pct",
            "fission undefined return type rate (%)",
            "lower_is_better",
        ),
    ]

    rows: list[dict[str, Any]] = []
    improved_count = 0
    degraded_count = 0
    unchanged_count = 0
    tolerance = 1e-9

    for key, label, direction in metric_specs:
        prev = _safe_float(previous_metrics.get(key, 0.0), 0.0)
        cur = _safe_float(current_metrics.get(key, 0.0), 0.0)
        delta = cur - prev
        if abs(delta) <= tolerance:
            status = "unchanged"
            unchanged_count += 1
        elif direction == "higher_is_better":
            if delta > 0:
                status = "improved"
                improved_count += 1
            else:
                status = "degraded"
                degraded_count += 1
        else:
            if delta < 0:
                status = "improved"
                improved_count += 1
            else:
                status = "degraded"
                degraded_count += 1

        rows.append(
            {
                "key": key,
                "label": label,
                "direction": direction,
                "previous": round(prev, 6),
                "current": round(cur, 6),
                "delta": round(delta, 6),
                "status": status,
            }
        )

    previous_generated_at = _lookup_path(previous_summary_payload, ("summary", "generated_at"), "unknown")
    current_generated_at = _lookup_path(current_benchmark, ("summary", "generated_at"), "unknown")
    degraded_functions = collect_top_degraded_functions_vs_previous(
        current_benchmark,
        previous_summary_payload,
        limit=20,
        similarity_drop_pp_threshold=1.0,
    )
    return {
        "previous_generated_at": previous_generated_at,
        "current_generated_at": current_generated_at,
        "improved_count": improved_count,
        "degraded_count": degraded_count,
        "unchanged_count": unchanged_count,
        "metrics": rows,
        "degraded_functions": degraded_functions,
        "overall": (
            "improved"
            if improved_count > degraded_count
            else "degraded"
            if degraded_count > improved_count
            else "mixed"
        ),
    }


def write_previous_comparison_files(
    output_dir: Path,
    comparison: dict[str, Any],
) -> tuple[Path, Path]:
    compare_json_path = output_dir / "benchmark_delta_vs_previous.json"
    compare_md_path = output_dir / "benchmark_delta_vs_previous.md"

    with compare_json_path.open("w", encoding="utf-8") as fh:
        json.dump(comparison, fh, indent=2)

    lines = [
        "# Benchmark Delta vs Previous Result",
        "",
        f"- Previous generated_at: {comparison.get('previous_generated_at', 'unknown')}",
        f"- Current generated_at: {comparison.get('current_generated_at', 'unknown')}",
        f"- Overall: {comparison.get('overall', 'mixed')}",
        (
            "- Counts: "
            f"improved={comparison.get('improved_count', 0)}, "
            f"degraded={comparison.get('degraded_count', 0)}, "
            f"unchanged={comparison.get('unchanged_count', 0)}"
        ),
        "",
        "| Metric | Direction | Previous | Current | Delta | Status |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for row in comparison.get("metrics", []):
        lines.append(
            f"| {row.get('label', row.get('key', 'metric'))} | {row.get('direction', '')} | "
            f"{_safe_float(row.get('previous', 0.0), 0.0):.6f} | "
            f"{_safe_float(row.get('current', 0.0), 0.0):.6f} | "
            f"{_safe_float(row.get('delta', 0.0), 0.0):+.6f} | {row.get('status', '')} |"
        )

    degraded = comparison.get("degraded_functions", {}) if isinstance(comparison.get("degraded_functions", {}), dict) else {}
    degraded_rows = degraded.get("top_degraded", []) if isinstance(degraded.get("top_degraded", []), list) else []
    lines.extend(
        [
            "",
            "## Top Degraded Functions vs Previous",
            "",
            (
                "- Threshold (drop): "
                f"{_safe_float(degraded.get('similarity_drop_pp_threshold', 0.0), 0.0):.3f}pp"
            ),
            f"- Degraded function count: {int(degraded.get('degraded_function_count', 0) or 0)}",
            "",
        ]
    )
    if degraded_rows:
        lines.extend(
            [
                "| Address | Function | Prev sim | Cur sim | Delta | Reasons |",
                "| --- | --- | ---: | ---: | ---: | --- |",
            ]
        )
        for row in degraded_rows:
            fn_name = row.get("fission_name") or row.get("pyghidra_name") or ""
            reasons = ", ".join(row.get("reason_tags", []))
            lines.append(
                f"| {row.get('address', '')} | {fn_name} | "
                f"{_safe_float(row.get('previous_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('current_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('normalized_similarity_delta', 0.0), 0.0):+.3f}pp | {reasons} |"
            )
    else:
        lines.append("- No significantly degraded functions detected.")

    with compare_md_path.open("w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")

    return compare_json_path, compare_md_path


def write_baseline_regression_files(
    output_dir: Path,
    report: dict[str, Any],
) -> tuple[Path, Path]:
    report_json_path = output_dir / "benchmark_regression_gate.json"
    report_md_path = output_dir / "benchmark_regression_gate.md"

    with report_json_path.open("w", encoding="utf-8") as fh:
        json.dump(report, fh, indent=2)

    row_gate = report.get("row_fidelity_gate", {}) if isinstance(report.get("row_fidelity_gate"), dict) else {}
    degraded_rows = (
        report.get("top_degraded_functions", {}).get("top_degraded", [])
        if isinstance(report.get("top_degraded_functions", {}), dict)
        else []
    )
    lines = [
        "# Benchmark Regression Gate vs Baseline",
        "",
        f"- Status: {report.get('status', 'unknown')}",
        f"- Similarity regression threshold: {float(report.get('threshold_pp', 0.0) or 0.0):.3f}pp",
        "",
        "## Regression Reasons",
        "",
    ]
    regressions = report.get("regressions", [])
    if regressions:
        lines.extend([f"- {msg}" for msg in regressions])
    else:
        lines.append("- No baseline regression detected.")

    lines.extend(
        [
            "",
            "## Row Fidelity Gate",
            "",
            f"- Status: {row_gate.get('status', 'unknown')}",
            f"- Failed targets: {int(row_gate.get('failed_target_count', 0) or 0)}",
            "",
            "| Role | Address | Prev sim | Cur sim | Delta | Status | Reasons |",
            "| --- | --- | ---: | ---: | ---: | --- | --- |",
        ]
    )
    for row in row_gate.get("rows", []):
        lines.append(
            f"| {row.get('role', '')} | {row.get('address', '')} | "
            f"{_safe_float(row.get('previous_normalized_similarity', 0.0), 0.0):.3f}% | "
            f"{_safe_float(row.get('current_normalized_similarity', 0.0), 0.0):.3f}% | "
            f"{_safe_float(row.get('normalized_similarity_delta', 0.0), 0.0):+.3f}pp | "
            f"{row.get('status', '')} | {', '.join(row.get('failure_reasons', [])) or 'none'} |"
        )

    lines.extend(
        [
            "",
            "## Top Regressions vs Baseline",
            "",
        ]
    )
    if degraded_rows:
        lines.extend(
            [
                "| Address | Function | Prev sim | Cur sim | Delta | Reasons |",
                "| --- | --- | ---: | ---: | ---: | --- |",
            ]
        )
        for row in degraded_rows[:10]:
            fn_name = row.get("fission_name") or row.get("pyghidra_name") or ""
            lines.append(
                f"| {row.get('address', '')} | {fn_name} | "
                f"{_safe_float(row.get('previous_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('current_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('normalized_similarity_delta', 0.0), 0.0):+.3f}pp | "
                f"{', '.join(row.get('reason_tags', []))} |"
            )
    else:
        lines.append("- No degraded rows recorded.")

    with report_md_path.open("w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")

    return report_json_path, report_md_path


def print_previous_comparison_summary(comparison: dict[str, Any]) -> None:
    improved = int(comparison.get("improved_count", 0) or 0)
    degraded = int(comparison.get("degraded_count", 0) or 0)
    unchanged = int(comparison.get("unchanged_count", 0) or 0)
    print(
        "Previous comparison: "
        f"overall={comparison.get('overall', 'mixed')}, "
        f"improved={improved}, degraded={degraded}, unchanged={unchanged}"
    )
    degraded_rows = [row for row in comparison.get("metrics", []) if row.get("status") == "degraded"]
    if degraded_rows:
        top = degraded_rows[:3]
        print("  Top degraded metrics:")
        for row in top:
            print(
                "   - "
                f"{row.get('label', row.get('key', 'metric'))}: "
                f"{_safe_float(row.get('previous', 0.0), 0.0):.6f} -> "
                f"{_safe_float(row.get('current', 0.0), 0.0):.6f} "
                f"(Δ={_safe_float(row.get('delta', 0.0), 0.0):+.6f})"
            )
    degraded_functions = comparison.get("degraded_functions", {})
    if isinstance(degraded_functions, dict):
        print(
            "  Degraded functions: "
            f"{int(degraded_functions.get('degraded_function_count', 0) or 0)} "
            "(vs previous run)"
        )


def resolve_default_output_dir(binary_path: Path, timestamped: bool) -> Path:
    if timestamped:
        timestamp = time.strftime("%Y%m%d-%H%M%S")
        return DEFAULT_RESULTS_DIR / f"{binary_path.stem}-{timestamp}"
    return DEFAULT_RESULTS_DIR / f"{binary_path.stem}-latest"


def resolve_ghidra_dir(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value.expanduser().resolve())

    env_dir = os.environ.get("GHIDRA_INSTALL_DIR")
    if env_dir:
        candidates.append(Path(env_dir).expanduser().resolve())

    candidates.extend(path.resolve() for path in DEFAULT_GHIDRA_DIRS)

    for candidate in candidates:
        if candidate.exists():
            return candidate

    checked = ", ".join(str(path) for path in candidates if path)
    raise FileNotFoundError(
        "Ghidra installation directory not found. "
        f"Checked: {checked if checked else '(none)'}"
    )


def resolve_fission_bin(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value.expanduser().resolve())

    candidates.extend(
        [
            (ROOT_DIR / "target" / "debug" / "fission_cli").resolve(),
            (ROOT_DIR / "target" / "release" / "fission_cli").resolve(),
        ]
    )

    for candidate in candidates:
        if candidate.is_file():
            return candidate

    checked = ", ".join(str(path) for path in candidates if path)
    raise FileNotFoundError(
        "fission_cli binary not found. "
        "Build it first (Rust-only path is supported). "
        f"Checked: {checked if checked else '(none)'}"
    )


def ensure_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def add_library_search_path(env: dict[str, str], key: str, value: str) -> None:
    current = env.get(key, "")
    env[key] = value if not current else f"{value}{os.pathsep}{current}"


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"

    text = str(value).strip()
    if not text:
        return "0x0"

    if text.lower().startswith("0x"):
        return f"0x{int(text, 16):x}"

    return f"0x{int(text, 16):x}"


def build_seeded_function_set(
    binary_path: Path,
    fission_bin: Path,
    limit: int | None,
    timeout_sec: int,
) -> list[tuple[str, str]]:
    discovered = list_functions_with_fission(
        ROOT_DIR,
        binary_path,
        fission_bin,
        timeout_sec=timeout_sec,
    )
    if not discovered:
        return []
    sampled = sample_functions(
        binary_path.name,
        discovered,
        limit or len(discovered),
        {},
    )
    return [(canonical_address(address), name) for address, name in sampled]


def build_address_alignment_summary(
    left_addrs: list[str] | set[str],
    right_addrs: list[str] | set[str],
) -> dict[str, Any]:
    left_norm = {canonical_address(addr) for addr in left_addrs}
    right_norm = {canonical_address(addr) for addr in right_addrs}
    shared = sorted(left_norm & right_norm, key=lambda addr: int(addr, 16))
    left_only = sorted(left_norm - right_norm, key=lambda addr: int(addr, 16))
    right_only = sorted(right_norm - left_norm, key=lambda addr: int(addr, 16))
    return {
        "left_total_count": len(left_norm),
        "right_total_count": len(right_norm),
        "shared_count": len(shared),
        "left_only_count": len(left_only),
        "right_only_count": len(right_only),
        "shared_of_left_pct": round((len(shared) / max(len(left_norm), 1)) * 100.0, 3),
        "shared_of_right_pct": round((len(shared) / max(len(right_norm), 1)) * 100.0, 3),
        "coverage_ratio_pct": round((len(shared) / max(len(left_norm), len(right_norm), 1)) * 100.0, 3),
        "shared_addresses": shared,
        "left_only": left_only,
        "right_only": right_only,
    }


def normalize_code(code: str | None) -> str:
    if not code:
        return ""

    text = code.replace("\r\n", "\n")
    text = BLOCK_COMMENT_RE.sub(" ", text)
    text = LINE_COMMENT_RE.sub("", text)
    text = AUTO_FUNC_RE.sub("FUNC", text)
    text = AUTO_SYMBOL_RE.sub("SYM", text)
    text = AUTO_VAR_RE.sub("VAR", text)
    text = HEX_RE.sub("HEX", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text


def classify_decompilation_result(
    code: str | None,
    error: str | None,
    reported_success: bool,
) -> tuple[bool, str | None]:
    if error:
        return False, "explicit_error"
    if code and code.lstrip().startswith(SYNTHETIC_FAILURE_PREFIX):
        return False, "synthetic_failure"
    if reported_success:
        return True, None
    return False, "unknown_failure"


def similarity_percent(left: str, right: str) -> float:
    if not left and not right:
        return 100.0
    if not left or not right:
        return 0.0
    if _rapidfuzz_fuzz is not None:
        return round(float(_rapidfuzz_fuzz.ratio(left, right)), 2)
    return round(difflib.SequenceMatcher(None, left, right).ratio() * 100.0, 2)


NATIVE_TIMING_PHASE_KEYS = (
    "follow_flow_ms",
    "main_perform_ms",
    "analysis_passes_ms",
    "callee_preanalysis_ms",
    "callgraph_reanalysis_ms",
    "print_ms",
    "postprocess_ms",
    "smart_constant_replace_ms",
    "cfg_structurizer_ms",
    "loop_normalize_ms",
    "stage1_rerun_ms",
    "stage2_rerun_ms",
)


def summarize_native_hot_paths(
    entries: dict[str, dict[str, Any]],
    limit: int = 10,
) -> list[dict[str, Any]]:
    hot_rows = sorted(
        (entry for entry in entries.values() if entry.get("native_timing")),
        key=lambda entry: float(entry.get("decomp_sec", 0.0) or 0.0),
        reverse=True,
    )[:limit]

    summary_rows: list[dict[str, Any]] = []
    for entry in hot_rows:
        native_timing = entry.get("native_timing") or {}
        phase_rows = sorted(
            (
                {
                    "phase": phase,
                    "ms": round(float(native_timing.get(phase, 0.0) or 0.0), 3),
                }
                for phase in NATIVE_TIMING_PHASE_KEYS
            ),
            key=lambda row: row["ms"],
            reverse=True,
        )
        summary_rows.append(
            {
                "address": entry.get("address"),
                "name": entry.get("name"),
                "decomp_sec": round(float(entry.get("decomp_sec", 0.0) or 0.0), 6),
                "callee_preanalysis_count": int(
                    native_timing.get("callee_preanalysis_count", 0) or 0
                ),
                "callgraph_reanalysis_count": int(
                    native_timing.get("callgraph_reanalysis_count", 0) or 0
                ),
                "top_native_phases": phase_rows[:3],
            }
        )

    return summary_rows


def run_fission_full(
    binary_path: Path,
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    profile: str,
    function_discovery_profile: str | None,
    compiler_id: str | None,
    struct_ptr_aliases: dict[str, str],
    public_engine: str = "fission",
    limit: int | None = None,
    seeded_functions: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    cli_engine = "mlil_preview"
    raw_output_path = output_dir / f"{public_engine}_full.json"
    stdout_log_path = output_dir / f"{public_engine}_stdout.log"
    stderr_log_path = output_dir / f"{public_engine}_stderr.log"
    temp_output = tempfile.NamedTemporaryFile(prefix="fission-benchmark-", suffix=".json", delete=False)
    temp_output.close()
    temp_addresses: tempfile.NamedTemporaryFile | None = None

    cmd = [
        str(fission_bin),
        str(binary_path),
        "--decomp-all",
        "--engine",
        cli_engine,
        "--benchmark",
        "--ghidra-compat",
        "--profile",
        profile,
        "-o",
        temp_output.name,
    ]
    if function_discovery_profile:
        cmd.extend(["--function-discovery-profile", function_discovery_profile])
    if compiler_id:
        cmd.extend(["--compiler-id", compiler_id])
    if seeded_functions:
        temp_addresses = tempfile.NamedTemporaryFile(
            prefix="fission-benchmark-addresses-",
            suffix=".txt",
            delete=False,
            mode="w",
            encoding="utf-8",
        )
        for address, _name in seeded_functions:
            temp_addresses.write(f"{canonical_address(address)}\n")
        temp_addresses.close()
        cmd.extend(["--addresses-file", temp_addresses.name])
    elif limit is not None:
        cmd.extend(["--decomp-limit", str(limit)])

    env = os.environ.copy()
    bin_dir = str(fission_bin.parent)
    add_library_search_path(env, "DYLD_LIBRARY_PATH", bin_dir)
    add_library_search_path(env, "LD_LIBRARY_PATH", bin_dir)

    pre_activity = collect_macos_activity_snapshot()
    post_activity: dict[str, Any] | None = None
    wall_start = time.perf_counter()
    resources: dict[str, Any] = {}
    try:
        if HAS_PSUTIL:
            popen = subprocess.Popen(
                cmd,
                cwd=ROOT_DIR,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            completed, resources = run_popen_with_resource_monitor(
                popen, timeout_sec=timeout_sec, interval_sec=0.5
            )
        else:
            completed = subprocess.run(
                cmd,
                cwd=ROOT_DIR,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=timeout_sec,
                check=False,
            )
        wall_clock_sec = time.perf_counter() - wall_start
        post_activity = collect_macos_activity_snapshot()
        stdout_log_path.write_text(completed.stdout, encoding="utf-8")
        stderr_log_path.write_text(completed.stderr, encoding="utf-8")
        had_nonzero_exit = completed.returncode != 0
        payload_exists = Path(temp_output.name).exists() and Path(temp_output.name).stat().st_size > 0
        if had_nonzero_exit and not payload_exists:
            raise RuntimeError(
                f"fission_cli failed with exit code {completed.returncode}.\n"
                f"stdout log: {stdout_log_path}\n"
                f"stderr log: {stderr_log_path}\n"
                "tail stdout:\n"
                f"stdout:\n{completed.stdout[-4000:]}\n"
                "tail stderr:\n"
                f"stderr:\n{completed.stderr[-4000:]}"
            )
        shutil.copyfile(temp_output.name, raw_output_path)
        with raw_output_path.open("r", encoding="utf-8") as handle:
            payload = json.load(handle)
    except subprocess.TimeoutExpired as exc:
        post_activity = collect_macos_activity_snapshot()
        partial_stdout = getattr(exc, "stdout", None) or ""
        partial_stderr = getattr(exc, "stderr", None) or ""
        stdout_log_path.write_text(partial_stdout, encoding="utf-8")
        stderr_log_path.write_text(partial_stderr, encoding="utf-8")
        raise RuntimeError(
            f"fission_cli timed out after {timeout_sec}s.\n"
            f"stdout log: {stdout_log_path}\n"
            f"stderr log: {stderr_log_path}"
        ) from exc
    finally:
        if post_activity is None:
            post_activity = collect_macos_activity_snapshot()
        try:
            os.unlink(temp_output.name)
        except FileNotFoundError:
            pass
        if temp_addresses is not None:
            try:
                os.unlink(temp_addresses.name)
            except FileNotFoundError:
                pass

    entries: dict[str, dict[str, Any]] = {}
    for entry in payload.get("functions", []):
        address = canonical_address(entry.get("address", "0x0"))
        code = entry.get("code", "")
        error = entry.get("error")
        reported_success = "error" not in entry
        actual_success, failure_kind = classify_decompilation_result(
            code=code,
            error=error,
            reported_success=reported_success,
        )
        entries[address] = {
            "address": address,
            "name": entry.get("name", ""),
            "present": True,
            "success": actual_success,
            "reported_success": reported_success,
            "code": code,
            "normalized_code": normalize_code(code),
            "error": error,
            "failure_kind": failure_kind,
            "decomp_sec": float(entry.get("decomp_sec", 0.0) or 0.0),
            "native_timing": entry.get("native_timing"),
            "metrics": collect_code_metrics(code, struct_ptr_aliases) if actual_success else {},
            "engine_used": entry.get("engine_used", cli_engine),
            "fell_back": bool(entry.get("fell_back", False)),
            "fallback_kind": entry.get("fallback_reason"),
            "preview_build_stats": entry.get("preview_build_stats"),
        }

    meta = dict(payload.get("_meta", {}))
    meta["wall_clock_sec"] = round(wall_clock_sec, 6)
    meta["raw_output_path"] = str(raw_output_path)
    meta["public_engine"] = public_engine
    meta["cache_mode"] = "warm"
    meta["function_discovery_profile"] = function_discovery_profile
    meta["process_exit_code"] = int(completed.returncode)
    meta["nonzero_exit_with_payload"] = bool(completed.returncode != 0)
    if seeded_functions:
        meta["seeded_addresses"] = [canonical_address(address) for address, _ in seeded_functions]
        meta["seeded_function_count"] = len(seeded_functions)
        meta["independent_top_n_addresses"] = [canonical_address(address) for address, _ in seeded_functions]
    combined_resources = dict(resources) if resources else {}
    if pre_activity:
        combined_resources["macos_activity_pre"] = pre_activity
    if post_activity:
        combined_resources["macos_activity_post"] = post_activity
    activity_delta = summarize_macos_activity_delta(pre_activity, post_activity)
    if activity_delta:
        combined_resources["macos_activity_delta"] = activity_delta
    if combined_resources:
        meta["resources"] = combined_resources

    return {
        "meta": meta,
        "entries": entries,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def run_ghidra_full(
    binary_path: Path,
    ghidra_dir: Path,
    output_dir: Path,
    per_function_timeout_sec: int,
    skip_thunks: bool,
    struct_ptr_aliases: dict[str, str],
    limit: int | None = None,
    cache_dir: Path | None = None,
    use_cache: bool = False,
    save_cache: bool = False,
    seeded_functions: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    # ── Cache hit: skip live Ghidra run ──────────────────────────────────────
    if use_cache and cache_dir is not None:
        cache_path = resolve_ghidra_cache_path(cache_dir, binary_path, ghidra_dir, limit)
        if cache_path.is_file():
            print(f"[*] Ghidra cache hit: {cache_path.name}")
            return load_ghidra_cache(cache_path)
        else:
            print(f"[*] Ghidra cache miss ({cache_path.name}); running live Ghidra …")

    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)

    try:
        import pyghidra
    except ImportError as exc:
        raise RuntimeError("pyghidra is not installed. Run `pip install pyghidra`.") from exc

    if seeded_functions:
        os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)
        pyghidra.start()
        raw_output_path = output_dir / "ghidra_full.json"
        all_function_addrs: list[str] = []
        with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
            program = flat_api.getCurrentProgram()
            function_manager = program.getFunctionManager()
            functions = list(function_manager.getFunctions(True))
            functions.sort(key=lambda func: int(func.getEntryPoint().getOffset()))
            for func in functions:
                try:
                    if func.isExternal():
                        continue
                except Exception:
                    pass
                try:
                    if skip_thunks and func.isThunk():
                        continue
                except Exception:
                    pass
                all_function_addrs.append(canonical_address(int(func.getEntryPoint().getOffset())))
                if limit is not None and len(all_function_addrs) >= limit:
                    break

        meta, results = run_ghidra_binary_with_meta(
            binary_path,
            seeded_functions,
            ghidra_dir,
            per_function_timeout_sec,
            struct_ptr_aliases,
        )
        entries: dict[str, dict[str, Any]] = {}
        function_rows: list[dict[str, Any]] = []
        total_decomp_sec = 0.0
        for address, name in seeded_functions:
            normalized = normalize_address(address)
            result = results.get(normalized, {})
            code = str(result.get("code", "") or "")
            present = result.get("failure_kind") != "missing_function"
            entry = {
                "address": canonical_address(address),
                "name": result.get("name", name),
                "present": bool(present),
                "success": bool(result.get("success", False)),
                "reported_success": bool(result.get("success", False)),
                "code": code,
                "normalized_code": normalize_code(code),
                "error": result.get("error"),
                "failure_kind": result.get("failure_kind"),
                "decomp_sec": round(float(result.get("decomp_sec", 0.0) or 0.0), 6),
                "metrics": result.get("metrics", {}) if result.get("success") else {},
                "engine_used": "pyghidra",
                "fell_back": False,
            }
            total_decomp_sec += float(entry["decomp_sec"])
            entries[entry["address"]] = entry
            function_rows.append(entry)

        payload = {
            "_meta": {
                "seeded_addresses": [canonical_address(address) for address, _ in seeded_functions],
                "seeded_function_count": len(seeded_functions),
                "independent_top_n_addresses": all_function_addrs,
                "wall_clock_sec": round(float(meta.get("wall_sec", 0.0) or 0.0), 6),
                "init_sec": round(float(meta.get("init_sec", 0.0) or 0.0), 6),
                "total_decomp_sec": round(total_decomp_sec, 6),
                "public_engine": "pyghidra",
                "cache_mode": "warm",
                "resources": meta.get("resources", {}),
                "seeded_pairing": True,
            },
            "functions": function_rows,
        }
        raw_output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        return {
            "meta": payload["_meta"],
            "entries": entries,
            "stdout": "",
            "stderr": "",
        }

    pyghidra.start()

    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    monitor = ConsoleTaskMonitor()
    raw_output_path = output_dir / "ghidra_full.json"
    pre_activity = collect_macos_activity_snapshot()
    post_activity: dict[str, Any] | None = None
    wall_start = time.perf_counter()

    res_thread = None
    res_holder: dict[str, Any] = {}
    res_stop: threading.Event | None = None
    if HAS_PSUTIL:
        res_thread, res_holder, res_stop = start_self_resource_monitor(interval_sec=0.5)
    init_start = time.perf_counter()
    total_decomp_sec = 0.0
    entries: dict[str, dict[str, Any]] = {}

    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        decomp = DecompInterface()
        decomp.openProgram(program)
        init_sec = time.perf_counter() - init_start

        function_manager = program.getFunctionManager()
        functions = list(function_manager.getFunctions(True))
        functions.sort(key=lambda func: int(func.getEntryPoint().getOffset()))

        for func in functions:
            try:
                if func.isExternal():
                    continue
            except Exception:
                pass

            try:
                if skip_thunks and func.isThunk():
                    continue
            except Exception:
                pass

            address = canonical_address(int(func.getEntryPoint().getOffset()))
            name = str(func.getName())

            start = time.perf_counter()
            code = ""
            error: str | None = None
            success = False

            try:
                result = decomp.decompileFunction(func, per_function_timeout_sec, monitor)
                if result and result.decompileCompleted() and result.getDecompiledFunction():
                    code = str(result.getDecompiledFunction().getC())
                    success = True
                else:
                    if result is not None:
                        error = str(result.getErrorMessage() or "decompile did not complete")
                    else:
                        error = "null decompile result"
            except Exception as exc:
                error = str(exc)

            elapsed = time.perf_counter() - start
            total_decomp_sec += elapsed
            actual_success, failure_kind = classify_decompilation_result(
                code=code,
                error=error,
                reported_success=success,
            )

            entries[address] = {
                "address": address,
                "name": name,
                "success": actual_success,
                "reported_success": success,
                "code": code,
                "normalized_code": normalize_code(code),
                "error": error,
                "failure_kind": failure_kind,
                "decomp_sec": round(elapsed, 6),
                "metrics": collect_code_metrics(code, struct_ptr_aliases) if actual_success else {},
                "engine_used": "pyghidra",
                "fell_back": False,
            }

            if limit is not None and len(entries) >= limit:
                break

        try:
            decomp.dispose()
        except Exception:
            pass

    wall_clock_sec = time.perf_counter() - wall_start
    post_activity = collect_macos_activity_snapshot()

    if HAS_PSUTIL and res_stop is not None and res_thread is not None:
        res_stop.set()
        res_thread.join(timeout=3.0)

    ghidra_resources = dict(res_holder) if res_holder else {}
    if pre_activity:
        ghidra_resources["macos_activity_pre"] = pre_activity
    if post_activity:
        ghidra_resources["macos_activity_post"] = post_activity
    activity_delta = summarize_macos_activity_delta(pre_activity, post_activity)
    if activity_delta:
        ghidra_resources["macos_activity_delta"] = activity_delta

    meta_dict: dict[str, Any] = {
        "tool": "ghidra",
        "backend": "pyghidra",
        "ghidra_install_dir": str(ghidra_dir),
        "function_count": len(entries),
        "init_sec": round(init_sec, 6),
        "total_decomp_sec": round(total_decomp_sec, 6),
        "wall_clock_sec": round(wall_clock_sec, 6),
        "per_function_timeout_sec": per_function_timeout_sec,
        "skip_thunks": skip_thunks,
        "cache_mode": "warm",
    }
    if ghidra_resources:
        meta_dict["resources"] = ghidra_resources

    payload = {
        "_meta": meta_dict,
        "functions": list(entries.values()),
    }
    with raw_output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)

    # ── Optionally persist Ghidra output to cache ─────────────────────────────
    if save_cache and cache_dir is not None:
        cache_path = resolve_ghidra_cache_path(cache_dir, binary_path, ghidra_dir, limit)
        try:
            save_ghidra_cache(cache_path, raw_output_path)
            print(f"[*] Ghidra output cached: {cache_path.name}")
        except Exception as exc:
            print(f"[!] Failed to save Ghidra cache: {exc}", file=sys.stderr)

    return {
        "meta": payload["_meta"],
        "entries": entries,
        "raw_output_path": str(raw_output_path),
    }


def summarize_engine_failures(entries: dict[str, dict[str, Any]]) -> dict[str, Any]:
    success_count = sum(1 for entry in entries.values() if entry.get("success"))
    failure_count = max(len(entries) - success_count, 0)
    return {
        "reported_success_count": sum(1 for entry in entries.values() if entry.get("reported_success", False)),
        "success_count": success_count,
        "failure_count": failure_count,
        "timeout_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "timeout"),
        "explicit_error_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "explicit_error"),
        "synthetic_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "synthetic_failure"),
        "unknown_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "unknown_failure"),
    }


def is_fission_direct_success(entry: dict[str, Any]) -> bool:
    if not entry.get("success") or entry.get("fell_back"):
        return False
    engine_used = str(entry.get("engine_used") or "").strip()
    if not engine_used:
        return False
    return engine_used not in {"pyghidra", "pcode_dump", "legacy"}


def summarize_engine_quality(entries: dict[str, dict[str, Any]], *, fission: bool = False) -> dict[str, Any]:
    success_entries = [entry for entry in entries.values() if entry.get("success")]
    def preview_stat_total(key: str) -> int:
        if not fission:
            return 0
        return sum(
            _safe_int((entry.get("preview_build_stats") or {}).get(key), 0)
            for entry in entries.values()
        )
    goto_values = [int((entry.get("metrics") or {}).get("goto_count", 0)) for entry in success_entries]
    label_values = [int((entry.get("metrics") or {}).get("top_level_label_count", 0)) for entry in success_entries]
    fpu_values = [int((entry.get("metrics") or {}).get("fpu_op_count", 0)) for entry in success_entries]
    jump_table_values = [int((entry.get("metrics") or {}).get("jump_table_count", 0)) for entry in success_entries]
    generic_local_values = [
        int((entry.get("metrics") or {}).get("local_name_generic_count", 0) or 0)
        for entry in success_entries
    ]
    generic_param_values = [
        int((entry.get("metrics") or {}).get("param_name_generic_count", 0) or 0)
        for entry in success_entries
    ]
    avg_line_values = [
        float((entry.get("metrics") or {}).get("avg_line_length_chars", 0) or 0.0)
        for entry in success_entries
    ]
    nesting_values = [
        int((entry.get("metrics") or {}).get("max_brace_nesting_depth", 0) or 0)
        for entry in success_entries
    ]
    comment_ratio_values = [
        float((entry.get("metrics") or {}).get("comment_char_ratio", 0) or 0.0)
        for entry in success_entries
    ]
    local_share_values = [
        float((entry.get("metrics") or {}).get("local_identifier_share", 0) or 0.0)
        for entry in success_entries
    ]
    local_mention_values = [
        int((entry.get("metrics") or {}).get("local_mention_count", 0) or 0)
        for entry in success_entries
    ]
    anti_keys = (
        "nested_cast",
        "line_over_200_chars",
        "ternary_operator",
        "address_of_chain",
        "double_semicolon",
    )
    anti_pattern_totals: dict[str, int] = {k: 0 for k in anti_keys}
    for entry in success_entries:
        ap = (entry.get("metrics") or {}).get("anti_pattern_counts") or {}
        for k in anti_keys:
            anti_pattern_totals[k] += int(ap.get(k, 0) or 0)

    return {
        "goto_total": sum(goto_values),
        "goto_median": round(statistics.median(goto_values), 3) if goto_values else 0.0,
        "top_level_label_total": sum(label_values),
        "top_level_label_median": round(statistics.median(label_values), 3) if label_values else 0.0,
        "empty_if_total": sum(int((entry.get("metrics") or {}).get("empty_if_count", 0)) for entry in success_entries),
        "constant_if_total": sum(int((entry.get("metrics") or {}).get("constant_if_count", 0)) for entry in success_entries),
        "direct_success_count": sum(1 for entry in success_entries if fission and is_fission_direct_success(entry)),
        "fpu_op_total": sum(fpu_values),
        "fpu_function_count": sum(1 for value in fpu_values if value > 0),
        "jump_table_total": sum(jump_table_values),
        "jump_table_function_count": sum(1 for value in jump_table_values if value > 0),
        "fell_back_count": sum(1 for entry in entries.values() if fission and entry.get("fell_back")),
        "unsupported_indirect_control_count": preview_stat_total("unsupported_indirect_control_count"),
        "unsupported_indirect_call_count": preview_stat_total("unsupported_indirect_call_count"),
        "unsupported_external_target_count": preview_stat_total("unsupported_external_target_count"),
        "indirect_surface_preserved_count": preview_stat_total("indirect_surface_preserved_count"),
        "indirect_target_set_refined_count": preview_stat_total("indirect_target_set_refined_count"),
        "dispatcher_shape_recovered_count": preview_stat_total("dispatcher_shape_recovered_count"),
        "materialization_stabilized_count": preview_stat_total("materialization_stabilized_count"),
        "proof_payload_direct_emit_count": preview_stat_total("proof_payload_direct_emit_count"),
        "pass_rerun_skipped_by_preservation_count": preview_stat_total("pass_rerun_skipped_by_preservation_count"),
        "dispatcher_proof_completed_count": preview_stat_total("dispatcher_proof_completed_count"),
        "dispatcher_proof_failed_count": preview_stat_total("dispatcher_proof_failed_count"),
        "candidate_scoped_jump_resolver_count": preview_stat_total("candidate_scoped_jump_resolver_count"),
        "sccp_skipped_by_admission_count": preview_stat_total("sccp_skipped_by_admission_count"),
        "memory_fact_prefilter_skip_count": preview_stat_total("memory_fact_prefilter_skip_count"),
        # Phase 4 pass quality indicators
        "undefined_return_type_total": sum(
            1 for entry in success_entries
            if bool((entry.get("metrics") or {}).get("undefined_return_type", False))
        ),
        "ptr_offset_total": sum(
            int((entry.get("metrics") or {}).get("ptr_offset_count", 0))
            for entry in success_entries
        ),
        "index_expr_total": sum(
            int((entry.get("metrics") or {}).get("index_expr_count", 0))
            for entry in success_entries
        ),
        "unknown_type_var_total": sum(
            int((entry.get("metrics") or {}).get("unknown_type_var_count", 0))
            for entry in success_entries
        ),
        "generic_local_name_sum": sum(generic_local_values),
        "generic_param_name_sum": sum(generic_param_values),
        "named_local_sum": sum(
            int((entry.get("metrics") or {}).get("named_local_count", 0) or 0)
            for entry in success_entries
        ),
        "named_param_sum": sum(
            int((entry.get("metrics") or {}).get("named_param_count", 0) or 0)
            for entry in success_entries
        ),
        "heuristic_avg_line_length_mean": round(statistics.fmean(avg_line_values), 3)
        if avg_line_values
        else 0.0,
        "heuristic_max_brace_nesting_mean": round(statistics.fmean(nesting_values), 3)
        if nesting_values
        else 0.0,
        "heuristic_max_brace_nesting_median": round(statistics.median(nesting_values), 3)
        if nesting_values
        else 0.0,
        "heuristic_comment_char_ratio_mean": round(statistics.fmean(comment_ratio_values), 6)
        if comment_ratio_values
        else 0.0,
        "heuristic_local_identifier_share_mean": round(statistics.fmean(local_share_values), 4)
        if local_share_values
        else 0.0,
        "heuristic_local_mention_total": sum(local_mention_values),
        "heuristic_anti_pattern_totals": anti_pattern_totals,
        "readability_control_flow_penalty": sum(goto_values) + sum(label_values),
        "readability_name_generic_total": sum(generic_local_values) + sum(generic_param_values),
    }


def percentile(values: list[float], percent: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    if len(sorted_values) == 1:
        return round(sorted_values[0], 6)
    rank = ((len(sorted_values) - 1) * percent) / 100.0
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return round(sorted_values[lower], 6)
    lower_value = sorted_values[lower]
    upper_value = sorted_values[upper]
    interp = lower_value + (upper_value - lower_value) * (rank - lower)
    return round(interp, 6)


def summarize_engine_quality_kpi(
    *,
    function_count: int,
    success_count: int,
    reported_success_count: int,
    timeout_count: int,
    fell_back_count: int = 0,
    direct_success_count: int = 0,
) -> dict[str, float]:
    total = max(function_count, 1)
    return {
        "success_rate_pct": round((success_count / total) * 100.0, 3),
        "failure_rate_pct": round(((function_count - success_count) / total) * 100.0, 3),
        "reported_success_rate_pct": round((reported_success_count / total) * 100.0, 3),
        "timeout_rate_pct": round((timeout_count / total) * 100.0, 3),
        "fallback_rate_pct": round((fell_back_count / total) * 100.0, 3),
        "direct_success_rate_pct": round((direct_success_count / total) * 100.0, 3),
    }


def summarize_engine_performance_kpi(entries: dict[str, dict[str, Any]], wall_sec: float) -> dict[str, float]:
    decomp_samples = [float(entry.get("decomp_sec", 0.0) or 0.0) for entry in entries.values()]
    function_count = len(entries)
    wall = max(float(wall_sec or 0.0), 1e-9)
    p50 = percentile(decomp_samples, 50.0)
    p95 = percentile(decomp_samples, 95.0)
    p99 = percentile(decomp_samples, 99.0)
    return {
        "wall_sec": round(float(wall_sec or 0.0), 6),
        "throughput_func_per_sec": round(function_count / wall, 3),
        "decomp_p50_sec": p50,
        "decomp_p95_sec": p95,
        "decomp_p99_sec": p99,
        "tail_ratio_p99_over_p50": round(p99 / max(p50, 1e-9), 3),
    }


def summarize_engine_memory_kpi(resources: dict[str, Any], function_count: int) -> dict[str, float]:
    total = max(function_count, 1)
    activity_delta = resources.get("macos_activity_delta", {}) if resources else {}
    peak_rss_mb = float(resources.get("max_rss_mb", 0.0) or 0.0)
    avg_rss_mb = float(resources.get("avg_rss_mb", 0.0) or 0.0)
    return {
        "peak_rss_mb": round(peak_rss_mb, 3),
        "avg_rss_mb": round(avg_rss_mb, 3),
        "peak_rss_mb_per_function": round(peak_rss_mb / total, 6),
        "avg_rss_mb_per_function": round(avg_rss_mb / total, 6),
        "vm_pageins_delta": round(float(activity_delta.get("vm_pageins", 0.0) or 0.0), 3),
        "vm_pageouts_delta": round(float(activity_delta.get("vm_pageouts", 0.0) or 0.0), 3),
        "vm_swapins_delta": round(float(activity_delta.get("vm_swapins", 0.0) or 0.0), 3),
        "vm_swapouts_delta": round(float(activity_delta.get("vm_swapouts", 0.0) or 0.0), 3),
        "vm_free_mb_delta": round(float(activity_delta.get("vm_free_mb", 0.0) or 0.0), 3),
    }


def summarize_engine_cpu_kpi(resources: dict[str, Any], wall_sec: float, function_count: int) -> dict[str, float]:
    avg_cpu_pct = float(resources.get("avg_cpu_pct", 0.0) or 0.0)
    max_cpu_pct = float(resources.get("max_cpu_pct", 0.0) or 0.0)
    activity_delta = resources.get("macos_activity_delta", {}) if resources else {}
    estimated_cpu_seconds = max(float(wall_sec or 0.0) * (avg_cpu_pct / 100.0), 0.0)
    return {
        "avg_cpu_pct": round(avg_cpu_pct, 3),
        "max_cpu_pct": round(max_cpu_pct, 3),
        "estimated_cpu_seconds": round(estimated_cpu_seconds, 6),
        "func_per_cpu_second": round(function_count / max(estimated_cpu_seconds, 1e-9), 3),
        "cpu_user_pct_delta": round(float(activity_delta.get("cpu_user_pct", 0.0) or 0.0), 3),
        "cpu_sys_pct_delta": round(float(activity_delta.get("cpu_sys_pct", 0.0) or 0.0), 3),
        "cpu_idle_pct_delta": round(float(activity_delta.get("cpu_idle_pct", 0.0) or 0.0), 3),
        "loadavg_1m_delta": round(float(activity_delta.get("loadavg_1m", 0.0) or 0.0), 3),
    }


def build_two_way_kpi_delta(
    pyghidra: dict[str, Any],
    fission: dict[str, Any],
) -> dict[str, Any]:
    return {
        "quality": {
            "success_rate_pct_diff": round(
                float(fission["quality_kpi"]["success_rate_pct"]) - float(pyghidra["quality_kpi"]["success_rate_pct"]),
                3,
            ),
            "timeout_rate_pct_diff": round(
                float(fission["quality_kpi"]["timeout_rate_pct"]) - float(pyghidra["quality_kpi"]["timeout_rate_pct"]),
                3,
            ),
            "fallback_rate_pct_diff": round(
                float(fission["quality_kpi"]["fallback_rate_pct"]) - float(pyghidra["quality_kpi"]["fallback_rate_pct"]),
                3,
            ),
        },
        "performance": {
            "throughput_speedup_vs_pyghidra": round(
                float(fission["performance_kpi"]["throughput_func_per_sec"])
                / max(float(pyghidra["performance_kpi"]["throughput_func_per_sec"]), 1e-9),
                3,
            ),
            "tail_ratio_diff": round(
                float(fission["performance_kpi"]["tail_ratio_p99_over_p50"])
                - float(pyghidra["performance_kpi"]["tail_ratio_p99_over_p50"]),
                3,
            ),
        },
        "memory": {
            "peak_rss_mb_ratio_fission_over_pyghidra": round(
                float(fission["memory_kpi"]["peak_rss_mb"]) / max(float(pyghidra["memory_kpi"]["peak_rss_mb"]), 1e-9),
                3,
            ),
            "avg_rss_mb_ratio_fission_over_pyghidra": round(
                float(fission["memory_kpi"]["avg_rss_mb"]) / max(float(pyghidra["memory_kpi"]["avg_rss_mb"]), 1e-9),
                3,
            ),
        },
        "cpu": {
            "avg_cpu_pct_diff": round(
                float(fission["cpu_kpi"]["avg_cpu_pct"]) - float(pyghidra["cpu_kpi"]["avg_cpu_pct"]),
                3,
            ),
            "func_per_cpu_second_ratio_fission_over_pyghidra": round(
                float(fission["cpu_kpi"]["func_per_cpu_second"])
                / max(float(pyghidra["cpu_kpi"]["func_per_cpu_second"]), 1e-9),
                3,
            ),
        },
    }


def select_sampled_addresses(shared_addrs: list[str], sample_size: int) -> list[str]:
    if sample_size <= 0 or len(shared_addrs) <= sample_size:
        return shared_addrs

    # Deterministic striding keeps runs reproducible across environments.
    last_idx = len(shared_addrs) - 1
    selected_idx = {
        round(i * last_idx / (sample_size - 1)) for i in range(sample_size)
    }
    return [addr for idx, addr in enumerate(shared_addrs) if idx in selected_idx]


def resolve_pairwise_mode(mode: str, shared_count: int, auto_shared_full_max: int) -> str:
    if mode != "auto":
        return mode
    # Keep auto mode conservative so post-processing doesn't dominate runtime.
    threshold = max(int(auto_shared_full_max or 0), 1)
    return "sampled" if shared_count > threshold else "shared-full"


def build_intersection_kpi_from_pair(pair: dict[str, Any]) -> dict[str, Any]:
    summary = pair.get("summary", {})
    rows = pair.get("comparisons", [])
    both_success_rows = [
        row
        for row in rows
        if row.get("both_success")
    ]
    norm_scores = [float(row.get("normalized_similarity", 0.0) or 0.0) for row in both_success_rows]

    high_agreement = sum(1 for score in norm_scores if score >= 90.0)
    medium_divergence = sum(1 for score in norm_scores if 50.0 <= score < 90.0)
    high_divergence = sum(1 for score in norm_scores if score < 50.0)
    denominator = max(len(norm_scores), 1)

    return {
        "shared_function_count": int(summary.get("shared_count", 0) or 0),
        "both_success_count": int(summary.get("both_success_count", 0) or 0),
        "both_success_rate_pct": round(
            (float(summary.get("both_success_count", 0) or 0) / max(float(summary.get("shared_count", 0) or 1), 1.0))
            * 100.0,
            3,
        ),
        "avg_normalized_similarity": round(statistics.fmean(norm_scores), 2) if norm_scores else 0.0,
        "median_normalized_similarity": round(statistics.median(norm_scores), 2) if norm_scores else 0.0,
        "aggregate_normalized_similarity": round(float(summary.get("aggregate_normalized_similarity", 0.0) or 0.0), 2),
        "high_agreement_pct": round((high_agreement / denominator) * 100.0, 3),
        "medium_divergence_pct": round((medium_divergence / denominator) * 100.0, 3),
        "high_divergence_pct": round((high_divergence / denominator) * 100.0, 3),
        "pairwise_mode": summary.get("pairwise_mode", "unknown"),
        "evaluated_shared_count": int(summary.get("evaluated_shared_count", 0) or 0),
    }


def build_row_fidelity_targets_snapshot(
    pair: dict[str, Any],
    row_targets: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    rows = pair.get("comparisons", [])
    row_map = {
        str(row.get("address", "")): row
        for row in rows
        if isinstance(row, dict) and row.get("address")
    }
    target_rows: list[dict[str, Any]] = []
    targets = row_targets or list(ROW_FIDELITY_TARGETS)

    for address, role in targets:
        row = row_map.get(address)
        if row is None:
            target_rows.append({"address": address, "role": role, "present": False})
            continue
        target_rows.append(
            {
                "address": address,
                "role": role,
                "present": True,
                "pyghidra_name": row.get("pyghidra_name", ""),
                "fission_name": row.get("fission_name", ""),
                "normalized_similarity": round(_safe_float(row.get("normalized_similarity", 0.0), 0.0), 3),
                "both_success": bool(row.get("both_success", False)),
                "fission_unsupported_indirect_control_count": _safe_int(
                    row.get("fission_unsupported_indirect_control_count"), 0
                ),
                "fission_indirect_surface_preserved_count": _safe_int(
                    row.get("fission_indirect_surface_preserved_count"), 0
                ),
                "fission_dispatcher_shape_recovered_count": _safe_int(
                    row.get("fission_dispatcher_shape_recovered_count"), 0
                ),
                "fission_dispatcher_proof_completed_count": _safe_int(
                    row.get("fission_dispatcher_proof_completed_count"), 0
                ),
                "fission_dispatcher_proof_failed_count": _safe_int(
                    row.get("fission_dispatcher_proof_failed_count"), 0
                ),
                "fission_proof_payload_direct_emit_count": _safe_int(
                    row.get("fission_proof_payload_direct_emit_count"), 0
                ),
            }
        )
    return {
        "target_count": len(targets),
        "present_count": sum(1 for row in target_rows if row.get("present")),
        "rows": target_rows,
    }


def build_proof_fidelity_summary(pair: dict[str, Any]) -> dict[str, Any]:
    rows = [
        row for row in pair.get("comparisons", [])
        if isinstance(row, dict) and row.get("both_success", False)
    ]

    def _subset(predicate: Any) -> tuple[int, float]:
        matched = [row for row in rows if predicate(row)]
        sims = [_safe_float(row.get("normalized_similarity", 0.0), 0.0) for row in matched]
        return len(matched), round(statistics.fmean(sims), 3) if sims else 0.0

    proof_completed_count, proof_completed_avg = _subset(
        lambda row: _safe_int(row.get("fission_dispatcher_proof_completed_count"), 0) > 0
    )
    proof_failed_count, proof_failed_avg = _subset(
        lambda row: _safe_int(row.get("fission_dispatcher_proof_failed_count"), 0) > 0
    )
    direct_emit_count, direct_emit_avg = _subset(
        lambda row: _safe_int(row.get("fission_proof_payload_direct_emit_count"), 0) > 0
    )
    dispatcher_count, dispatcher_avg = _subset(
        lambda row: bool(row.get("fission_has_dispatcher_recovery", False))
    )
    return {
        "proof_completed_row_count": proof_completed_count,
        "proof_completed_row_avg_normalized_similarity": proof_completed_avg,
        "proof_failed_row_count": proof_failed_count,
        "proof_failed_row_avg_normalized_similarity": proof_failed_avg,
        "proof_direct_emit_row_count": direct_emit_count,
        "proof_direct_emit_row_avg_normalized_similarity": direct_emit_avg,
        "dispatcher_row_count": dispatcher_count,
        "dispatcher_row_avg_normalized_similarity": dispatcher_avg,
    }


def build_residue_family_summary(pair: dict[str, Any]) -> dict[str, Any]:
    rows = [row for row in pair.get("comparisons", []) if isinstance(row, dict)]
    return {
        "unsupported_indirect_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_unresolved_unsupported_indirect", False))
        ),
        "preserved_indirect_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_preserved_indirect_surface", False))
        ),
        "dispatcher_recovery_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_dispatcher_recovery", False))
        ),
        "target_proof_row_count": sum(
            1 for row in rows if bool(row.get("fission_has_indirect_target_proof", False))
        ),
        "proof_failed_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_dispatcher_proof_failed_count"), 0) > 0
        ),
        "proof_completed_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_dispatcher_proof_completed_count"), 0) > 0
        ),
        "materialization_stabilized_row_count": sum(
            1 for row in rows if _safe_int(row.get("fission_materialization_stabilized_count"), 0) > 0
        ),
    }


def build_perf_admission_summary(fission_summary: dict[str, Any]) -> dict[str, Any]:
    return {
        "candidate_scoped_jump_resolver_count": _safe_int(
            fission_summary.get("candidate_scoped_jump_resolver_count"), 0
        ),
        "sccp_skipped_by_admission_count": _safe_int(
            fission_summary.get("sccp_skipped_by_admission_count"), 0
        ),
        "memory_fact_prefilter_skip_count": _safe_int(
            fission_summary.get("memory_fact_prefilter_skip_count"), 0
        ),
        "pass_rerun_skipped_by_preservation_count": _safe_int(
            fission_summary.get("pass_rerun_skipped_by_preservation_count"), 0
        ),
    }


def build_layered_quality_from_pair(pair: dict[str, Any]) -> dict[str, Any]:
    rows = pair.get("comparisons", [])
    left_label = str(pair.get("left_label", "left"))
    right_label = str(pair.get("right_label", "right"))
    left_sec_key = f"{left_label}_decomp_sec"
    right_sec_key = f"{right_label}_decomp_sec"

    both_success_rows = [row for row in rows if row.get("both_success")]
    if not both_success_rows:
        return {
            "layering": "max_decomp_sec_terciles",
            "q33_sec": 0.0,
            "q66_sec": 0.0,
            "layers": {},
        }

    complexity_values = [
        max(
            _safe_float(row.get(left_sec_key, 0.0), 0.0),
            _safe_float(row.get(right_sec_key, 0.0), 0.0),
        )
        for row in both_success_rows
    ]
    q33 = percentile(complexity_values, 33.333)
    q66 = percentile(complexity_values, 66.667)

    bucket_rows: dict[str, list[dict[str, Any]]] = {
        "small": [],
        "medium": [],
        "large": [],
    }
    for row in both_success_rows:
        complexity = max(
            _safe_float(row.get(left_sec_key, 0.0), 0.0),
            _safe_float(row.get(right_sec_key, 0.0), 0.0),
        )
        if complexity <= q33:
            bucket_rows["small"].append(row)
        elif complexity <= q66:
            bucket_rows["medium"].append(row)
        else:
            bucket_rows["large"].append(row)

    layers: dict[str, Any] = {}
    for name, layer_rows in bucket_rows.items():
        norms = [_safe_float(r.get("normalized_similarity", 0.0), 0.0) for r in layer_rows]
        high_div = sum(1 for score in norms if score < 50.0)
        layers[name] = {
            "count": len(layer_rows),
            "avg_normalized_similarity": round(statistics.fmean(norms), 3) if norms else 0.0,
            "median_normalized_similarity": round(statistics.median(norms), 3) if norms else 0.0,
            "high_divergence_pct": round((high_div / max(len(norms), 1)) * 100.0, 3),
        }

    return {
        "layering": "max_decomp_sec_terciles",
        "q33_sec": round(float(q33), 6),
        "q66_sec": round(float(q66), 6),
        "layers": layers,
    }


def build_pairwise_engine_comparison(
    left_label: str,
    left: dict[str, Any],
    right_label: str,
    right: dict[str, Any],
    pairwise_similarity_mode: str,
    pairwise_sample_size: int,
    pairwise_auto_shared_full_max: int,
    compute_raw_similarity: bool,
    aggregate_similarity_mode: str,
) -> dict[str, Any]:
    left_entries = left["entries"]
    right_entries = right["entries"]
    left_addrs = {
        address for address, entry in left_entries.items() if entry.get("present", True)
    }
    right_addrs = {
        address for address, entry in right_entries.items() if entry.get("present", True)
    }
    shared_addrs = sorted(left_addrs & right_addrs, key=lambda addr: int(addr, 16))
    left_only = sorted(left_addrs - right_addrs, key=lambda addr: int(addr, 16))
    right_only = sorted(right_addrs - left_addrs, key=lambda addr: int(addr, 16))
    effective_mode = resolve_pairwise_mode(
        pairwise_similarity_mode,
        len(shared_addrs),
        pairwise_auto_shared_full_max,
    )
    eval_addrs = (
        select_sampled_addresses(shared_addrs, pairwise_sample_size)
        if effective_mode == "sampled"
        else shared_addrs
    )

    rows: list[dict[str, Any]] = []
    successful_rows: list[dict[str, Any]] = []
    aggregate_left_parts: list[str] = []
    aggregate_right_parts: list[str] = []
    weighted_norm_sum = 0.0
    weighted_norm_weight = 0

    for address in eval_addrs:
        left_entry = left_entries[address]
        right_entry = right_entries[address]
        raw_similarity = (
            similarity_percent(left_entry.get("code", ""), right_entry.get("code", ""))
            if compute_raw_similarity
            else None
        )
        left_norm = left_entry.get("normalized_code", "")
        right_norm = right_entry.get("normalized_code", "")
        normalized_similarity = similarity_percent(left_norm, right_norm)
        both_success = bool(left_entry.get("success") and right_entry.get("success"))
        row = {
            "address": address,
            f"{left_label}_name": left_entry.get("name", ""),
            f"{right_label}_name": right_entry.get("name", ""),
            f"{left_label}_success": left_entry.get("success", False),
            f"{right_label}_success": right_entry.get("success", False),
            f"{left_label}_failure_kind": left_entry.get("failure_kind"),
            f"{right_label}_failure_kind": right_entry.get("failure_kind"),
            f"{left_label}_decomp_sec": left_entry.get("decomp_sec", 0.0),
            f"{right_label}_decomp_sec": right_entry.get("decomp_sec", 0.0),
            "raw_similarity": raw_similarity,
            "normalized_similarity": normalized_similarity,
            f"{left_label}_error": left_entry.get("error"),
            f"{right_label}_error": right_entry.get("error"),
            "both_success": both_success,
        }
        for label, entry in ((left_label, left_entry), (right_label, right_entry)):
            preview_build_stats = entry.get("preview_build_stats") or {}
            row[f"{label}_unsupported_indirect_control_count"] = _safe_int(
                preview_build_stats.get("unsupported_indirect_control_count"), 0
            )
            row[f"{label}_unsupported_indirect_call_count"] = _safe_int(
                preview_build_stats.get("unsupported_indirect_call_count"), 0
            )
            row[f"{label}_unsupported_external_target_count"] = _safe_int(
                preview_build_stats.get("unsupported_external_target_count"), 0
            )
            row[f"{label}_indirect_surface_preserved_count"] = _safe_int(
                preview_build_stats.get("indirect_surface_preserved_count"), 0
            )
            row[f"{label}_indirect_target_set_refined_count"] = _safe_int(
                preview_build_stats.get("indirect_target_set_refined_count"), 0
            )
            row[f"{label}_dispatcher_shape_recovered_count"] = _safe_int(
                preview_build_stats.get("dispatcher_shape_recovered_count"), 0
            )
            row[f"{label}_dispatcher_proof_completed_count"] = _safe_int(
                preview_build_stats.get("dispatcher_proof_completed_count"), 0
            )
            row[f"{label}_dispatcher_proof_failed_count"] = _safe_int(
                preview_build_stats.get("dispatcher_proof_failed_count"), 0
            )
            row[f"{label}_proof_payload_direct_emit_count"] = _safe_int(
                preview_build_stats.get("proof_payload_direct_emit_count"), 0
            )
            row[f"{label}_materialization_stabilized_count"] = _safe_int(
                preview_build_stats.get("materialization_stabilized_count"), 0
            )
            row[f"{label}_pass_rerun_skipped_by_preservation_count"] = _safe_int(
                preview_build_stats.get("pass_rerun_skipped_by_preservation_count"), 0
            )
            row[f"{label}_candidate_scoped_jump_resolver_count"] = _safe_int(
                preview_build_stats.get("candidate_scoped_jump_resolver_count"), 0
            )
            row[f"{label}_sccp_skipped_by_admission_count"] = _safe_int(
                preview_build_stats.get("sccp_skipped_by_admission_count"), 0
            )
            row[f"{label}_memory_fact_prefilter_skip_count"] = _safe_int(
                preview_build_stats.get("memory_fact_prefilter_skip_count"), 0
            )
            for flag_name, flag_value in _canonical_indirect_flags(preview_build_stats).items():
                row[f"{label}_{flag_name}"] = flag_value
        rows.append(row)
        if both_success:
            successful_rows.append(row)
            if aggregate_similarity_mode == "sequence":
                aggregate_left_parts.append(left_norm)
                aggregate_right_parts.append(right_norm)
            weight = max(len(left_norm), len(right_norm), 1)
            weighted_norm_sum += normalized_similarity * weight
            weighted_norm_weight += weight

    normalized_scores = [row["normalized_similarity"] for row in successful_rows]
    raw_scores = [
        row["raw_similarity"]
        for row in successful_rows
        if row.get("raw_similarity") is not None
    ]
    if aggregate_similarity_mode == "sequence":
        aggregate_normalized_similarity = similarity_percent(
            "\n".join(aggregate_left_parts),
            "\n".join(aggregate_right_parts),
        )
    else:
        aggregate_normalized_similarity = round(
            weighted_norm_sum / max(weighted_norm_weight, 1),
            2,
        )

    return {
        "left_label": left_label,
        "right_label": right_label,
        "comparisons": rows,
        "left_only": left_only,
        "right_only": right_only,
        "summary": {
            "left_total_count": len(left_addrs),
            "right_total_count": len(right_addrs),
            "shared_count": len(shared_addrs),
            "left_only_count": len(left_only),
            "right_only_count": len(right_only),
            "shared_of_left_pct": round((len(shared_addrs) / max(len(left_addrs), 1)) * 100.0, 3),
            "shared_of_right_pct": round((len(shared_addrs) / max(len(right_addrs), 1)) * 100.0, 3),
            "coverage_ratio_pct": round((len(shared_addrs) / max(len(left_addrs), len(right_addrs), 1)) * 100.0, 3),
            "both_success_count": len(successful_rows),
            "aggregate_normalized_similarity": aggregate_normalized_similarity,
            "aggregate_normalized_similarity_method": aggregate_similarity_mode,
            "avg_normalized_similarity": round(statistics.fmean(normalized_scores), 2) if normalized_scores else 0.0,
            "median_normalized_similarity": round(statistics.median(normalized_scores), 2) if normalized_scores else 0.0,
            "avg_raw_similarity": round(statistics.fmean(raw_scores), 2) if raw_scores else 0.0,
            "pairwise_mode": effective_mode,
            "evaluated_shared_count": len(eval_addrs),
            "raw_similarity_enabled": bool(compute_raw_similarity),
        },
    }


def build_comparison(
    binary_path: Path,
    pyghidra: dict[str, Any],
    fission: dict[str, Any],
    pairwise_similarity_mode: str,
    pairwise_sample_size: int,
    pairwise_auto_shared_full_max: int,
    raw_similarity: bool,
    aggregate_similarity_mode: str,
    row_fidelity_targets_filter: list[tuple[str, str]] | None = None,
) -> dict[str, Any]:
    pair_py_fission = build_pairwise_engine_comparison(
        "pyghidra",
        pyghidra,
        "fission",
        fission,
        pairwise_similarity_mode=pairwise_similarity_mode,
        pairwise_sample_size=pairwise_sample_size,
        pairwise_auto_shared_full_max=pairwise_auto_shared_full_max,
        compute_raw_similarity=raw_similarity,
        aggregate_similarity_mode=aggregate_similarity_mode,
    )

    py_failures = summarize_engine_failures(pyghidra["entries"])
    py_quality = summarize_engine_quality(pyghidra["entries"])
    py_resources = pyghidra["meta"].get("resources", {})
    py_perf_kpi = summarize_engine_performance_kpi(
        pyghidra["entries"],
        float(pyghidra["meta"].get("wall_clock_sec", 0.0) or 0.0),
    )
    py_mem_kpi = summarize_engine_memory_kpi(py_resources, len(pyghidra["entries"]))
    py_cpu_kpi = summarize_engine_cpu_kpi(
        py_resources,
        float(pyghidra["meta"].get("wall_clock_sec", 0.0) or 0.0),
        len(pyghidra["entries"]),
    )
    py_quality_kpi = summarize_engine_quality_kpi(
        function_count=len(pyghidra["entries"]),
        success_count=int(py_failures["success_count"]),
        reported_success_count=int(py_failures["reported_success_count"]),
        timeout_count=int(py_failures["timeout_count"]),
    )

    fission_failures = summarize_engine_failures(fission["entries"])
    fission_quality = summarize_engine_quality(fission["entries"], fission=True)
    fission_resources = fission["meta"].get("resources", {})
    fission_perf_kpi = summarize_engine_performance_kpi(
        fission["entries"],
        float(fission["meta"].get("wall_clock_sec", 0.0) or 0.0),
    )
    fission_mem_kpi = summarize_engine_memory_kpi(fission_resources, len(fission["entries"]))
    fission_cpu_kpi = summarize_engine_cpu_kpi(
        fission_resources,
        float(fission["meta"].get("wall_clock_sec", 0.0) or 0.0),
        len(fission["entries"]),
    )
    fission_quality_kpi = summarize_engine_quality_kpi(
        function_count=len(fission["entries"]),
        success_count=int(fission_failures["success_count"]),
        reported_success_count=int(fission_failures["reported_success_count"]),
        timeout_count=int(fission_failures["timeout_count"]),
        fell_back_count=int(fission_quality.get("fell_back_count", 0)),
        direct_success_count=int(fission_quality.get("direct_success_count", 0)),
    )

    engine_kpi = {
        "pyghidra": {
            "quality_kpi": py_quality_kpi,
            "performance_kpi": py_perf_kpi,
            "memory_kpi": py_mem_kpi,
            "cpu_kpi": py_cpu_kpi,
        },
        "fission": {
            "quality_kpi": fission_quality_kpi,
            "performance_kpi": fission_perf_kpi,
            "memory_kpi": fission_mem_kpi,
            "cpu_kpi": fission_cpu_kpi,
        },
    }
    kpi_delta = build_two_way_kpi_delta(engine_kpi["pyghidra"], engine_kpi["fission"])
    intersection_kpi = {
        "pyghidra_vs_fission": build_intersection_kpi_from_pair(pair_py_fission),
    }
    layered_quality = {
        "pyghidra_vs_fission": build_layered_quality_from_pair(pair_py_fission),
    }
    proof_fidelity = {
        "pyghidra_vs_fission": build_proof_fidelity_summary(pair_py_fission),
    }
    residue_families = {
        "pyghidra_vs_fission": build_residue_family_summary(pair_py_fission),
    }
    row_fidelity_targets = {
        "pyghidra_vs_fission": build_row_fidelity_targets_snapshot(
            pair_py_fission,
            row_targets=row_fidelity_targets_filter,
        ),
    }
    independent_top_n_coverage = build_address_alignment_summary(
        pyghidra["meta"].get("independent_top_n_addresses", list(pyghidra["entries"].keys())),
        fission["meta"].get("independent_top_n_addresses", list(fission["entries"].keys())),
    )

    summary = {
        "binary": str(binary_path),
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "cache_mode": "warm",
        "pairwise_similarity_backend": SIMILARITY_BACKEND,
        "pairwise_raw_similarity_enabled": bool(raw_similarity),
        "pairwise_aggregate_similarity_mode": aggregate_similarity_mode,
        "pairwise_auto_shared_full_max": int(pairwise_auto_shared_full_max),
        "row_fidelity_role_filter": "all",
        "engines": {
            "pyghidra": {
                **pyghidra["meta"],
                "function_count": len(pyghidra["entries"]),
                **py_failures,
                **py_quality,
                **engine_kpi["pyghidra"],
            },
            "fission": {
                **fission["meta"],
                "function_count": len(fission["entries"]),
                **fission_failures,
                **fission_quality,
                **engine_kpi["fission"],
            },
        },
        "coverage": {
            "pyghidra_vs_fission": pair_py_fission["summary"],
            "independent_top_n_pyghidra_vs_fission": independent_top_n_coverage,
        },
        "quality": {
            "pyghidra_vs_fission": pair_py_fission["summary"],
        },
        "quality_layers": layered_quality,
        "proof_fidelity": proof_fidelity,
        "residue_families": residue_families,
        "row_fidelity_targets": row_fidelity_targets,
        "resources": {
            "pyghidra": py_resources,
            "fission": fission_resources,
        },
        "kpi": {
            "engines": engine_kpi,
            "delta": kpi_delta,
            "intersection": intersection_kpi,
        },
        "speed": {
            "pyghidra": {
                "init_sec": round(float(pyghidra["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(pyghidra["meta"].get("total_decomp_sec", 0.0)), 6),
                "wall_sec": round(float(pyghidra["meta"].get("wall_clock_sec", 0.0)), 6),
            },
            "fission": {
                "init_sec": round(float(fission["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(fission["meta"].get("total_decomp_sec", 0.0)), 6),
                "postprocess_sec": round(float(fission["meta"].get("total_postprocess_sec", 0.0)), 6),
                "wall_sec": round(float(fission["meta"].get("wall_clock_sec", 0.0)), 6),
                "wall_speedup_vs_pyghidra": round(
                    float(pyghidra["meta"].get("wall_clock_sec", 0.0))
                    / max(float(fission["meta"].get("wall_clock_sec", 0.0)), 1e-9),
                    3,
                ),
            },
        },
        "samples": {
            "pyghidra_vs_fission_lowest_similarity": sorted(pair_py_fission["comparisons"], key=lambda row: row["normalized_similarity"])[:20],
            "fission_hot_path_phases": summarize_native_hot_paths(fission["entries"]),
        },
        "admission_and_preservation": {
            "fission": build_perf_admission_summary(
                {
                    **fission_quality,
                    **engine_kpi["fission"],
                }
            ),
        },
        "public_summary_line": "",
    }

    summary["public_summary_line"] = (
        f"fission vs pyghidra wall speedup {summary['speed']['fission']['wall_speedup_vs_pyghidra']}x; "
        f"throughput speedup {summary['kpi']['delta']['performance']['throughput_speedup_vs_pyghidra']}x; "
        f"seeded shared coverage {summary['coverage']['pyghidra_vs_fission']['coverage_ratio_pct']:.2f}%; "
        f"independent top-N coverage {summary['coverage']['independent_top_n_pyghidra_vs_fission']['coverage_ratio_pct']:.2f}%; "
        f"fission direct-success {summary['engines']['fission']['direct_success_count']}/{summary['engines']['fission']['function_count']}; "
        f"fission unsupported indirect {summary['engines']['fission']['unsupported_indirect_control_count']}; "
        f"fission indirect-surface preserved {summary['engines']['fission']['indirect_surface_preserved_count']}; "
        f"fission jump-table functions {summary['engines']['fission']['jump_table_function_count']}; "
        f"fission dispatcher recovered {summary['engines']['fission']['dispatcher_shape_recovered_count']}"
    )

    return {
        "summary": summary,
        "pairwise": {
            "pyghidra_vs_fission": pair_py_fission,
        },
        "engines": {
            "pyghidra": pyghidra,
            "fission": fission,
        },
    }


def write_summary_files(
    output_dir: Path,
    benchmark: dict[str, Any],
) -> tuple[Path, Path]:
    summary_json_path = output_dir / "benchmark_summary.json"
    summary_md_path = output_dir / "benchmark_summary.md"

    with summary_json_path.open("w", encoding="utf-8") as handle:
        json.dump(benchmark, handle, indent=2)

    summary = benchmark["summary"]
    low_rows = summary["samples"]["pyghidra_vs_fission_lowest_similarity"]
    hot_rows = summary["samples"].get("fission_hot_path_phases", [])
    baseline_gate = benchmark.get("baseline_regression_gate")
    if not isinstance(baseline_gate, dict):
        baseline_gate = {}
    def _format_macos_snapshot(snapshot: dict[str, Any] | None) -> str:
        if not snapshot:
            return "n/a"
        load1 = snapshot.get("loadavg_1m", "n/a")
        cpu_idle = snapshot.get("cpu_idle_pct", "n/a")
        free_mb = snapshot.get("vm_free_mb", "n/a")
        return f"load1={load1}, cpu_idle={cpu_idle}%, vm_free={free_mb}MB"

    lines = [
        f"# Whole Decomp Benchmark: {Path(summary['binary']).name}",
        "",
        "## Why 2-Way",
        "",
        "- `pyghidra`: Python-host baseline",
        "- `fission`: Rust-first decompiler path",
        "",
        "## Summary",
        "",
        f"- Generated: {summary['generated_at']}",
        f"- Cache mode: `{summary['cache_mode']}`",
        f"- Similarity backend: `{summary.get('pairwise_similarity_backend', 'unknown')}`",
        f"- Raw similarity enabled: `{summary.get('pairwise_raw_similarity_enabled', False)}`",
        f"- Aggregate similarity mode: `{summary.get('pairwise_aggregate_similarity_mode', 'weighted')}`",
        f"- Public summary: {summary['public_summary_line']}",
        "",
    ]
    if baseline_gate:
        row_gate = baseline_gate.get("row_fidelity_gate", {})
        lines.extend(
            [
                "## Baseline Gate",
                "",
                f"- Status: `{baseline_gate.get('status', 'unknown')}`",
                f"- Similarity regression threshold: `{float(baseline_gate.get('threshold_pp', 0.0) or 0.0):.3f}pp`",
                f"- Row fidelity gate: `{row_gate.get('status', 'unknown')}`",
                f"- Failed targets: `{', '.join(row_gate.get('failed_targets', [])) or 'none'}`",
                "",
            ]
        )
    lines.extend(
        [
            "## Function Set Alignment",
            "",
            "| Pair | Left total | Right total | Shared | Left-only | Right-only | Coverage | Mode | Evaluated shared |",
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |",
        ]
    )
    for label, pair in summary["coverage"].items():
        lines.append(
            f"| `{label}` | {pair.get('left_total_count', 0)} | {pair.get('right_total_count', 0)} | "
            f"{pair.get('shared_count', 0)} | {pair.get('left_only_count', 0)} | {pair.get('right_only_count', 0)} | "
            f"{pair.get('coverage_ratio_pct', 0):.2f}% | {pair.get('pairwise_mode', 'n/a')} | "
            f"{pair.get('evaluated_shared_count', 0)} |"
        )

    lines.extend([
        "",
        "## Speed",
        "",
        f"- pyghidra wall: {summary['speed']['pyghidra']['wall_sec']:.3f}s",
        f"- fission wall: {summary['speed']['fission']['wall_sec']:.3f}s",
        f"- fission vs pyghidra speedup: {summary['speed']['fission']['wall_speedup_vs_pyghidra']:.3f}x",
        "",
    ])

    lines.extend([
        "## Intersection KPI",
        "",
        "| Pair | Both-success rate | Avg norm sim | Median norm sim | Aggregate norm sim | High agreement | High divergence |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for label, inter in summary.get("kpi", {}).get("intersection", {}).items():
        lines.append(
            f"| `{label}` | {inter.get('both_success_rate_pct', 0):.3f}% | "
            f"{inter.get('avg_normalized_similarity', 0):.2f}% | {inter.get('median_normalized_similarity', 0):.2f}% | "
            f"{inter.get('aggregate_normalized_similarity', 0):.2f}% | "
            f"{inter.get('high_agreement_pct', 0):.3f}% | {inter.get('high_divergence_pct', 0):.3f}% |"
        )

    lines.extend([
        "",
        "## Core KPI (Quality / Performance / Memory / CPU)",
        "",
        "| Engine | Success rate | Timeout rate | Fallback rate | Throughput (func/s) | p95 (s) | p99/p50 | Peak RSS (MB) | Avg CPU (%) | Func/CPU-sec |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    kpi_engines = summary.get("kpi", {}).get("engines", {})
    for label in ("pyghidra", "fission"):
        q = kpi_engines.get(label, {}).get("quality_kpi", {})
        p = kpi_engines.get(label, {}).get("performance_kpi", {})
        m = kpi_engines.get(label, {}).get("memory_kpi", {})
        c = kpi_engines.get(label, {}).get("cpu_kpi", {})
        lines.append(
            f"| `{label}` | {q.get('success_rate_pct', 0):.3f}% | {q.get('timeout_rate_pct', 0):.3f}% | "
            f"{q.get('fallback_rate_pct', 0):.3f}% | {p.get('throughput_func_per_sec', 0):.3f} | "
            f"{p.get('decomp_p95_sec', 0):.6f} | {p.get('tail_ratio_p99_over_p50', 0):.3f} | "
            f"{m.get('peak_rss_mb', 0):.3f} | {c.get('avg_cpu_pct', 0):.3f} | {c.get('func_per_cpu_second', 0):.3f} |"
        )

    delta = summary.get("kpi", {}).get("delta", {})
    lines.extend([
        "",
        "- Delta quality (fission - pyghidra): "
        f"success {delta.get('quality', {}).get('success_rate_pct_diff', 0):.3f}%, "
        f"timeout {delta.get('quality', {}).get('timeout_rate_pct_diff', 0):.3f}%, "
        f"fallback {delta.get('quality', {}).get('fallback_rate_pct_diff', 0):.3f}%",
        "- Delta performance: "
        f"throughput speedup {delta.get('performance', {}).get('throughput_speedup_vs_pyghidra', 0):.3f}x, "
        f"tail-ratio diff {delta.get('performance', {}).get('tail_ratio_diff', 0):.3f}",
        "- Delta memory: "
        f"peak RSS ratio {delta.get('memory', {}).get('peak_rss_mb_ratio_fission_over_pyghidra', 0):.3f}x, "
        f"avg RSS ratio {delta.get('memory', {}).get('avg_rss_mb_ratio_fission_over_pyghidra', 0):.3f}x",
        "- Delta CPU: "
        f"avg CPU diff {delta.get('cpu', {}).get('avg_cpu_pct_diff', 0):.3f}%, "
        f"func/CPU-sec ratio {delta.get('cpu', {}).get('func_per_cpu_second_ratio_fission_over_pyghidra', 0):.3f}x",
        "",
        "## Engine Coverage / Quality",
        "",
        "| Engine | Success | Failure | Timeout | goto total | label total | empty if | constant if | undef ret | ptr offset | idx expr | unkn vars |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ])
    for label, engine in summary["engines"].items():
        lines.append(
            f"| `{label}` | {engine.get('success_count', 0)} | {engine.get('failure_count', 0)} | "
            f"{engine.get('timeout_count', 0)} | {engine.get('goto_total', 0)} | "
            f"{engine.get('top_level_label_total', 0)} | {engine.get('empty_if_total', 0)} | "
            f"{engine.get('constant_if_total', 0)} | "
            f"{engine.get('undefined_return_type_total', 0)} | "
            f"{engine.get('ptr_offset_total', 0)} | "
            f"{engine.get('index_expr_total', 0)} | "
            f"{engine.get('unknown_type_var_total', 0)} |"
        )

    lines.extend(
        [
            "",
            "## Heuristic readability proxies",
            "",
            "Per-function aggregates from `collect_code_metrics` (text heuristics only; not semantic truth).",
            "",
            "| Metric | pyghidra | fission |",
            "| --- | ---: | ---: |",
        ]
    )
    heur_rows = [
        ("avg line length (chars, mean)", "heuristic_avg_line_length_mean", "float3"),
        ("max brace nesting (median)", "heuristic_max_brace_nesting_median", "float3"),
        ("max brace nesting (mean)", "heuristic_max_brace_nesting_mean", "float3"),
        ("comment char ratio (mean)", "heuristic_comment_char_ratio_mean", "float6"),
        ("local_ identifier share (mean)", "heuristic_local_identifier_share_mean", "float4"),
        ("local_N mention total", "heuristic_local_mention_total", "int"),
        ("generic local name assignments (sum)", "generic_local_name_sum", "int"),
        ("generic param names (sum)", "generic_param_name_sum", "int"),
        ("named local declarations (sum)", "named_local_sum", "int"),
        ("named params (sum)", "named_param_sum", "int"),
        ("readability: goto + label total", "readability_control_flow_penalty", "int"),
        ("readability: generic local+param names", "readability_name_generic_total", "int"),
    ]
    for title, key, kind in heur_rows:
        pv = summary["engines"]["pyghidra"].get(key, 0)
        fv = summary["engines"]["fission"].get(key, 0)
        if kind == "int":
            lines.append(f"| {title} | {int(pv)} | {int(fv)} |")
        elif kind == "float3":
            lines.append(f"| {title} | {float(pv):.3f} | {float(fv):.3f} |")
        elif kind == "float4":
            lines.append(f"| {title} | {float(pv):.4f} | {float(fv):.4f} |")
        else:
            lines.append(f"| {title} | {float(pv):.6f} | {float(fv):.6f} |")

    py_anti = summary["engines"]["pyghidra"].get("heuristic_anti_pattern_totals") or {}
    fi_anti = summary["engines"]["fission"].get("heuristic_anti_pattern_totals") or {}
    anti_order = (
        "nested_cast",
        "line_over_200_chars",
        "ternary_operator",
        "address_of_chain",
        "double_semicolon",
    )
    lines.extend(
        [
            "",
            "### Anti-pattern regex totals (successful functions)",
            "",
            "| Pattern | pyghidra | fission |",
            "| --- | ---: | ---: |",
        ]
    )
    for ak in anti_order:
        lines.append(
            f"| `{ak}` | {int(py_anti.get(ak, 0) or 0)} | {int(fi_anti.get(ak, 0) or 0)} |"
        )

    lines.extend([
        "",
        "## Pairwise Quality",
        "",
        "| Pair | Shared | Both success | Aggregate norm sim | Avg norm sim |",
        "| --- | ---: | ---: | ---: | ---: |",
    ])
    for label, pair in summary["quality"].items():
        lines.append(
            f"| `{label}` | {pair.get('shared_count', 0)} | {pair.get('both_success_count', 0)} | "
            f"{pair.get('aggregate_normalized_similarity', 0):.2f}% | {pair.get('avg_normalized_similarity', 0):.2f}% |"
        )
    for label, pair in summary["quality"].items():
        lines.append(
            f"- `{label}` aggregate mode: `{pair.get('aggregate_normalized_similarity_method', 'unknown')}`; "
            f"raw similarity enabled: `{pair.get('raw_similarity_enabled', False)}`"
        )

    lines.extend(["", "## Proof vs Fidelity", ""])
    for label, block in summary.get("proof_fidelity", {}).items():
        lines.append(
            f"- {label}: proof-completed rows={int(block.get('proof_completed_row_count', 0) or 0)} "
            f"(avg sim {_safe_float(block.get('proof_completed_row_avg_normalized_similarity', 0.0), 0.0):.3f}%), "
            f"proof-failed rows={int(block.get('proof_failed_row_count', 0) or 0)} "
            f"(avg sim {_safe_float(block.get('proof_failed_row_avg_normalized_similarity', 0.0), 0.0):.3f}%), "
            f"direct-emit rows={int(block.get('proof_direct_emit_row_count', 0) or 0)} "
            f"(avg sim {_safe_float(block.get('proof_direct_emit_row_avg_normalized_similarity', 0.0), 0.0):.3f}%)"
        )

    residue = summary.get("residue_families", {}).get("pyghidra_vs_fission", {})
    lines.extend(
        [
            "",
            "## Residue Families",
            "",
            "| Unsupported rows | Preserved rows | Dispatcher rows | Target-proof rows | Proof-failed rows | Proof-completed rows | Materialized rows |",
            "| ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
            f"| {int(residue.get('unsupported_indirect_row_count', 0) or 0)} | "
            f"{int(residue.get('preserved_indirect_row_count', 0) or 0)} | "
            f"{int(residue.get('dispatcher_recovery_row_count', 0) or 0)} | "
            f"{int(residue.get('target_proof_row_count', 0) or 0)} | "
            f"{int(residue.get('proof_failed_row_count', 0) or 0)} | "
            f"{int(residue.get('proof_completed_row_count', 0) or 0)} | "
            f"{int(residue.get('materialization_stabilized_row_count', 0) or 0)} |",
        ]
    )

    row_targets = summary.get("row_fidelity_targets", {}).get("pyghidra_vs_fission", {})
    if row_targets:
        lines.extend(["", "## Fixed Row Targets", ""])
        lines.extend(
            [
                "| Role | Address | Similarity | Both success | Unsupported | Preserved | Dispatcher | Proof completed | Direct emit |",
                "| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |",
            ]
        )
        for row in row_targets.get("rows", []):
            if not row.get("present"):
                lines.append(
                    f"| {row.get('role', '')} | `{row.get('address', '')}` | n/a | no | n/a | n/a | n/a | n/a | n/a |"
                )
                continue
            lines.append(
                f"| {row.get('role', '')} | `{row.get('address', '')}` | "
                f"{_safe_float(row.get('normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{'yes' if row.get('both_success', False) else 'no'} | "
                f"{int(row.get('fission_unsupported_indirect_control_count', 0) or 0)} | "
                f"{int(row.get('fission_indirect_surface_preserved_count', 0) or 0)} | "
                f"{int(row.get('fission_dispatcher_shape_recovered_count', 0) or 0)} | "
                f"{int(row.get('fission_dispatcher_proof_completed_count', 0) or 0)} | "
                f"{int(row.get('fission_proof_payload_direct_emit_count', 0) or 0)} |"
            )

    lines.extend(["", "## Layered Quality (by max decomp-sec terciles)", ""])
    for label, layer in summary.get("quality_layers", {}).items():
        lines.append(
            f"- {label}: q33={_safe_float(layer.get('q33_sec', 0.0), 0.0):.6f}s, "
            f"q66={_safe_float(layer.get('q66_sec', 0.0), 0.0):.6f}s"
        )
        lines.append("| Layer | Count | Avg norm sim | Median norm sim | High divergence |")
        lines.append("| --- | ---: | ---: | ---: | ---: |")
        for bucket in ("small", "medium", "large"):
            row = layer.get("layers", {}).get(bucket, {})
            lines.append(
                f"| {bucket} | {int(row.get('count', 0) or 0)} | "
                f"{_safe_float(row.get('avg_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('median_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('high_divergence_pct', 0.0), 0.0):.3f}% |"
            )
        lines.append("")

    lines.extend(["", "## Resources (psutil + macOS activity snapshot)", ""])
    resources = summary.get("resources", {})
    has_psutil_rows = any(resources.get(label, {}).get("max_rss_mb", 0.0) for label in ("pyghidra", "fission"))
    has_macos_rows = any(
        resources.get(label, {}).get("macos_activity_pre") for label in ("pyghidra", "fission")
    )
    if has_psutil_rows:
        for label in ("pyghidra", "fission"):
            res = resources.get(label, {})
            lines.append(
                f"- {label}: max RSS {res.get('max_rss_mb', 0):.2f} MB, avg CPU {res.get('avg_cpu_pct', 0):.2f}%"
            )
    elif not HAS_PSUTIL:
        lines.append("- Install `pip install psutil` for resource usage metrics.")
    if has_macos_rows:
        for label in ("pyghidra", "fission"):
            res = resources.get(label, {})
            pre_text = _format_macos_snapshot(res.get("macos_activity_pre"))
            post_text = _format_macos_snapshot(res.get("macos_activity_post"))
            lines.append(f"- {label} macOS pre: {pre_text}")
            lines.append(f"- {label} macOS post: {post_text}")
    lines.extend(["", "## Representative Lowest Similarity (`pyghidra_vs_fission`)", ""])

    if low_rows:
        lines.append("| Address | pyghidra | fission | Norm Similarity | Fission Indirect |")
        lines.append("|---|---|---|---:|---|")
        for row in low_rows[:10]:
            indirect_flags = []
            if row.get("fission_has_unresolved_unsupported_indirect"):
                indirect_flags.append("unsupported")
            if row.get("fission_has_preserved_indirect_surface"):
                indirect_flags.append("preserved")
            if row.get("fission_has_dispatcher_recovery"):
                indirect_flags.append("dispatcher")
            if row.get("fission_has_indirect_target_proof"):
                indirect_flags.append("target-proof")
            lines.append(
                f"| `{row['address']}` | `{row['pyghidra_name']}` | `{row['fission_name']}` | "
                f"{row['normalized_similarity']:.2f}% | "
                f"`{','.join(indirect_flags) if indirect_flags else 'none'}` |"
            )
    else:
        lines.append("- No shared successful functions to compare.")

    if baseline_gate:
        row_gate = baseline_gate.get("row_fidelity_gate", {})
        lines.extend(["", "## Row Fidelity Gate vs Baseline", ""])
        lines.extend(
            [
                "| Role | Address | Prev sim | Cur sim | Delta | Status | Reasons |",
                "| --- | --- | ---: | ---: | ---: | --- | --- |",
            ]
        )
        for row in row_gate.get("rows", []):
            lines.append(
                f"| {row.get('role', '')} | `{row.get('address', '')}` | "
                f"{_safe_float(row.get('previous_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('current_normalized_similarity', 0.0), 0.0):.3f}% | "
                f"{_safe_float(row.get('normalized_similarity_delta', 0.0), 0.0):+.3f}pp | "
                f"{row.get('status', '')} | {', '.join(row.get('failure_reasons', [])) or 'none'} |"
            )

        degraded_rows = (
            baseline_gate.get("top_degraded_functions", {}).get("top_degraded", [])
            if isinstance(baseline_gate.get("top_degraded_functions", {}), dict)
            else []
        )
        lines.extend(["", "## Top Regressions vs Baseline", ""])
        if degraded_rows:
            lines.extend(
                [
                    "| Address | Function | Prev sim | Cur sim | Delta | Reasons |",
                    "| --- | --- | ---: | ---: | ---: | --- |",
                ]
            )
            for row in degraded_rows[:10]:
                fn_name = row.get("fission_name") or row.get("pyghidra_name") or ""
                lines.append(
                    f"| `{row.get('address', '')}` | `{fn_name}` | "
                    f"{_safe_float(row.get('previous_normalized_similarity', 0.0), 0.0):.3f}% | "
                    f"{_safe_float(row.get('current_normalized_similarity', 0.0), 0.0):.3f}% | "
                    f"{_safe_float(row.get('normalized_similarity_delta', 0.0), 0.0):+.3f}pp | "
                    f"{', '.join(row.get('reason_tags', []))} |"
                )
        else:
            lines.append("- No degraded rows recorded.")

    lines.extend(["", "## Fission Native Hot Paths", ""])
    if hot_rows:
        lines.append("| Address | Function | Decomp Sec | Top Phases | Helper Counts |")
        lines.append("|---|---|---:|---|---|")
        for row in hot_rows[:10]:
            phases = ", ".join(
                f"{phase['phase']}={phase['ms']:.3f}ms" for phase in row["top_native_phases"]
            )
            counts = (
                f"callee={row['callee_preanalysis_count']}, "
                f"callgraph={row['callgraph_reanalysis_count']}"
            )
            lines.append(
                f"| `{row['address']}` | `{row['name']}` | {row['decomp_sec']:.6f} | "
                f"{phases} | {counts} |"
            )
    else:
        lines.append("- No native timing data recorded.")

    lines.extend([
        "",
        "## Focus Metrics",
        "",
        f"- Fission FPU-op total: {summary['engines']['fission'].get('fpu_op_total', 0)}",
        f"- Fission FPU-op function count: {summary['engines']['fission'].get('fpu_function_count', 0)}",
        f"- Fission jump-table total: {summary['engines']['fission'].get('jump_table_total', 0)}",
        f"- Fission jump-table function count: {summary['engines']['fission'].get('jump_table_function_count', 0)}",
        f"- Fission materialization stabilized total: {summary['engines']['fission'].get('materialization_stabilized_count', 0)}",
        f"- Fission proof payload direct emit total: {summary['engines']['fission'].get('proof_payload_direct_emit_count', 0)}",
        f"- Fission dispatcher proof completed total: {summary['engines']['fission'].get('dispatcher_proof_completed_count', 0)}",
        f"- Fission dispatcher proof failed total: {summary['engines']['fission'].get('dispatcher_proof_failed_count', 0)}",
        f"- Fission candidate-scoped jump-resolver total: {summary['admission_and_preservation']['fission'].get('candidate_scoped_jump_resolver_count', 0)}",
        f"- Fission SCCP skipped by admission total: {summary['admission_and_preservation']['fission'].get('sccp_skipped_by_admission_count', 0)}",
        f"- Fission memory-fact prefilter skip total: {summary['admission_and_preservation']['fission'].get('memory_fact_prefilter_skip_count', 0)}",
        f"- Fission rerun skipped by preservation total: {summary['admission_and_preservation']['fission'].get('pass_rerun_skipped_by_preservation_count', 0)}",
    ])

    lines.extend(
        [
            "",
            "## Artifacts",
            "",
            "- `fission_full.json`: raw Rust decompiler output",
            "- `ghidra_full.json`: raw pyghidra whole-decomp output",
            "- `benchmark_summary.json`: merged metrics and per-function comparison",
        ]
    )

    with summary_md_path.open("w", encoding="utf-8") as handle:
        handle.write("\n".join(lines) + "\n")

    return summary_json_path, summary_md_path


def print_console_summary(
    summary: dict[str, Any],
    output_dir: Path,
    baseline_gate: dict[str, Any] | None = None,
) -> None:
    print("\n=== Whole Decomp Benchmark Summary ===")
    print(f"Binary: {summary['binary']}")
    print(summary["public_summary_line"])
    print(
        f"pyghidra wall={summary['speed']['pyghidra']['wall_sec']:.3f}s | "
        f"fission wall={summary['speed']['fission']['wall_sec']:.3f}s"
    )
    print(
        f"fission vs pyghidra similarity={summary['quality']['pyghidra_vs_fission']['avg_normalized_similarity']:.2f}%"
    )
    coverage = summary.get("coverage", {}).get("pyghidra_vs_fission", {})
    independent = summary.get("coverage", {}).get("independent_top_n_pyghidra_vs_fission", {})
    print(
        "Function-set alignment: "
        f"shared={coverage.get('shared_count', 0)}, "
        f"left_only={coverage.get('left_only_count', 0)}, "
        f"right_only={coverage.get('right_only_count', 0)}, "
        f"coverage={coverage.get('coverage_ratio_pct', 0):.2f}%, "
        f"mode={coverage.get('pairwise_mode', 'n/a')}"
    )
    if independent:
        print(
            "Independent top-N alignment: "
            f"shared={independent.get('shared_count', 0)}, "
            f"left_only={independent.get('left_only_count', 0)}, "
            f"right_only={independent.get('right_only_count', 0)}, "
            f"coverage={independent.get('coverage_ratio_pct', 0):.2f}%"
        )
    inter = summary.get("kpi", {}).get("intersection", {}).get("pyghidra_vs_fission", {})
    if inter:
        print(
            "Intersection KPI: "
            f"both_success={inter.get('both_success_rate_pct', 0):.3f}%, "
            f"avg_norm={inter.get('avg_normalized_similarity', 0):.2f}%, "
            f"high_div={inter.get('high_divergence_pct', 0):.3f}%"
        )
    kpi = summary.get("kpi", {}).get("engines", {})
    py_perf = kpi.get("pyghidra", {}).get("performance_kpi", {})
    fi_perf = kpi.get("fission", {}).get("performance_kpi", {})
    py_quality = kpi.get("pyghidra", {}).get("quality_kpi", {})
    fi_quality = kpi.get("fission", {}).get("quality_kpi", {})
    print(
        "KPI: "
        f"pyghidra succ={py_quality.get('success_rate_pct', 0):.3f}% thr={py_perf.get('throughput_func_per_sec', 0):.3f}func/s, "
        f"fission succ={fi_quality.get('success_rate_pct', 0):.3f}% thr={fi_perf.get('throughput_func_per_sec', 0):.3f}func/s"
    )
    print(
        "KPI tail-ratio p99/p50: "
        f"pyghidra={py_perf.get('tail_ratio_p99_over_p50', 0):.3f}, "
        f"fission={fi_perf.get('tail_ratio_p99_over_p50', 0):.3f}"
    )
    res = summary.get("resources", {})
    if any(res.get(label, {}).get("max_rss_mb", 0.0) for label in ("pyghidra", "fission")):
        print(
            f"Resources: pyghidra max_rss={res.get('pyghidra', {}).get('max_rss_mb', 0):.2f}MB | "
            f"fission max_rss={res.get('fission', {}).get('max_rss_mb', 0):.2f}MB"
        )
    elif not HAS_PSUTIL:
        print("Resources: (install psutil for metrics)")
    fission_macos = res.get("fission", {}).get("macos_activity_pre", {})
    if fission_macos:
        print(
            "macOS sample (fission pre): "
            f"load1={fission_macos.get('loadavg_1m', 'n/a')}, "
            f"cpu_idle={fission_macos.get('cpu_idle_pct', 'n/a')}%, "
            f"vm_free={fission_macos.get('vm_free_mb', 'n/a')}MB"
        )
    hot_rows = summary["samples"].get("fission_hot_path_phases", [])
    if hot_rows:
        top = hot_rows[0]
        phases = ", ".join(
            f"{phase['phase']}={phase['ms']:.3f}ms" for phase in top["top_native_phases"]
        )
        print(
            f"Top fission hot path: {top['address']} {top['name']} "
            f"(decomp={top['decomp_sec']:.6f}s, {phases}, "
            f"callee={top['callee_preanalysis_count']}, "
            f"callgraph={top['callgraph_reanalysis_count']})"
        )
    proof = summary.get("proof_fidelity", {}).get("pyghidra_vs_fission", {})
    if proof:
        print(
            "Proof/Fidelity: "
            f"completed_rows={int(proof.get('proof_completed_row_count', 0) or 0)} "
            f"(avg={_safe_float(proof.get('proof_completed_row_avg_normalized_similarity', 0.0), 0.0):.3f}%), "
            f"failed_rows={int(proof.get('proof_failed_row_count', 0) or 0)} "
            f"(avg={_safe_float(proof.get('proof_failed_row_avg_normalized_similarity', 0.0), 0.0):.3f}%), "
            f"direct_emit_rows={int(proof.get('proof_direct_emit_row_count', 0) or 0)}"
        )
    row_targets = summary.get("row_fidelity_targets", {}).get("pyghidra_vs_fission", {})
    if row_targets:
        present = [row for row in row_targets.get("rows", []) if row.get("present")]
        if present:
            top_targets = ", ".join(
                f"{row.get('address')}={_safe_float(row.get('normalized_similarity', 0.0), 0.0):.2f}%"
                for row in present[:4]
            )
            print(f"Row targets: {top_targets}")
    if baseline_gate:
        row_gate = baseline_gate.get("row_fidelity_gate", {}) if isinstance(baseline_gate, dict) else {}
        print(
            "Baseline gate: "
            f"status={baseline_gate.get('status', 'unknown')}, "
            f"row_fidelity={row_gate.get('status', 'unknown')}, "
            f"failed_targets={','.join(row_gate.get('failed_targets', [])) or 'none'}"
        )
    print(f"Artifacts: {output_dir}")


def main() -> int:
    args = parse_args()
    binary_path = resolve_binary(args.binary)
    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    fission_bin = resolve_fission_bin(args.fission_bin)

    output_dir = ensure_dir(
        args.output_dir.resolve()
        if args.output_dir
        else resolve_default_output_dir(binary_path, timestamped=bool(args.timestamped_output))
    )

    previous_summary_payload: dict[str, Any] | None = None
    baseline_summary_payload: dict[str, Any] | None = None
    previous_summary_path = output_dir / "benchmark_summary.json"
    if previous_summary_path.is_file():
        try:
            with previous_summary_path.open("r", encoding="utf-8") as fh:
                previous_summary_payload = json.load(fh)
            print(f"[*] Previous summary detected for auto-compare: {previous_summary_path}")
        except Exception as exc:
            print(f"[!] Failed to load previous summary for auto-compare: {exc}", file=sys.stderr)

    baseline_dir: Path | None = getattr(args, "baseline_dir", None)
    if baseline_dir is not None:
        baseline_dir = baseline_dir.expanduser().resolve()
        baseline_summary_payload = load_baseline_summary(baseline_dir)
        if baseline_summary_payload is not None:
            print(f"[*] Baseline summary detected: {baseline_dir / 'benchmark_summary.json'}")

    print(f"[*] Binary: {binary_path}")
    print(f"[*] Fission CLI: {fission_bin}")
    print(f"[*] Ghidra dir: {ghidra_dir}")
    print(f"[*] Output dir: {output_dir}")
    print(
        "[*] Fission config: "
        f"profile={args.profile}, "
        f"function_discovery_profile={args.function_discovery_profile}"
    )
    print(
        "[*] Pairwise config: "
        f"mode={args.pairwise_similarity_mode}, "
        f"sample_size={args.pairwise_sample_size}, "
        f"auto_shared_full_max={args.pairwise_auto_shared_full_max}, "
        f"raw_similarity={args.raw_similarity}, "
        f"aggregate_mode={args.aggregate_similarity_mode}, "
        f"backend={SIMILARITY_BACKEND}"
    )
    row_fidelity_targets_filter = select_row_fidelity_targets(
        str(getattr(args, "row_fidelity_role_filter", "all"))
    )
    print(
        "[*] Row-fidelity targets: "
        f"filter={args.row_fidelity_role_filter}, "
        f"count={len(row_fidelity_targets_filter)}"
    )
    if SIMILARITY_BACKEND != "rapidfuzz":
        print(
            "[!] rapidfuzz not found; falling back to difflib. "
            "For faster similarity matching: pip install rapidfuzz"
        )

    # ── Resolve effective function limit ──────────────────────────────────────
    effective_limit = resolve_effective_limit(
        binary_path=binary_path,
        fission_bin=fission_bin,
        explicit_limit=args.limit,
        auto_limit=getattr(args, "auto_limit_functions", None),
    )
    if effective_limit is not None:
        print(f"[*] Limit: first {effective_limit} canonical seed functions only")

    # ── Ghidra cache configuration ────────────────────────────────────────────
    ghidra_cache_dir: Path | None = getattr(args, "ghidra_cache_dir", None)
    if ghidra_cache_dir is not None:
        ghidra_cache_dir = ghidra_cache_dir.expanduser().resolve()
    use_ghidra_cache: bool = bool(getattr(args, "use_ghidra_cache", False))
    save_ghidra_cache: bool = bool(getattr(args, "save_ghidra_cache", False))
    if use_ghidra_cache and ghidra_cache_dir is None:
        ghidra_cache_dir = ROOT_DIR / "artifacts" / "ghidra_cache"
    if save_ghidra_cache and ghidra_cache_dir is None:
        ghidra_cache_dir = ROOT_DIR / "artifacts" / "ghidra_cache"

    struct_ptr_aliases = load_struct_pointer_aliases(BASE_TYPES_JSON)
    seeded_functions = build_seeded_function_set(
        binary_path=binary_path,
        fission_bin=fission_bin,
        limit=effective_limit,
        timeout_sec=args.timeout,
    )
    print(f"[*] Seeded function set: {len(seeded_functions)} canonical functions")

    fission = run_fission_full(
        binary_path=binary_path,
        fission_bin=fission_bin,
        output_dir=output_dir,
        timeout_sec=args.timeout,
        profile=args.profile,
        function_discovery_profile=args.function_discovery_profile,
        compiler_id=args.compiler_id,
        struct_ptr_aliases=struct_ptr_aliases,
        public_engine="fission",
        limit=effective_limit,
        seeded_functions=seeded_functions,
    )
    print(
        f"[*] Fission complete: functions={len(fission['entries'])}, "
        f"wall={float(fission['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    ghidra = run_ghidra_full(
        binary_path=binary_path,
        ghidra_dir=ghidra_dir,
        output_dir=output_dir,
        per_function_timeout_sec=args.ghidra_func_timeout,
        skip_thunks=args.skip_thunks,
        struct_ptr_aliases=struct_ptr_aliases,
        limit=effective_limit,
        cache_dir=ghidra_cache_dir,
        use_cache=use_ghidra_cache,
        save_cache=save_ghidra_cache,
        seeded_functions=seeded_functions,
    )
    print(
        f"[*] Ghidra complete: functions={len(ghidra['entries'])}, "
        f"wall={float(ghidra['meta'].get('wall_clock_sec', 0.0)):.3f}s"
        + (" (from cache)" if ghidra["meta"].get("from_cache") else "")
    )

    benchmark = build_comparison(
        binary_path,
        ghidra,
        fission,
        pairwise_similarity_mode=args.pairwise_similarity_mode,
        pairwise_sample_size=args.pairwise_sample_size,
        pairwise_auto_shared_full_max=args.pairwise_auto_shared_full_max,
        raw_similarity=bool(args.raw_similarity),
        aggregate_similarity_mode=args.aggregate_similarity_mode,
        row_fidelity_targets_filter=row_fidelity_targets_filter,
    )
    benchmark["summary"]["row_fidelity_role_filter"] = str(
        getattr(args, "row_fidelity_role_filter", "all")
    )
    if baseline_summary_payload is not None:
        benchmark["baseline_regression_gate"] = _build_baseline_regression_report(
            benchmark,
            baseline_summary_payload,
            float(getattr(args, "regression_threshold", 2.0)),
            row_targets=row_fidelity_targets_filter,
        )
    write_summary_files(output_dir, benchmark)
    print_console_summary(
        benchmark["summary"],
        output_dir,
        benchmark.get("baseline_regression_gate"),
    )

    if previous_summary_payload is not None:
        previous_comparison = compare_with_previous_summary(benchmark, previous_summary_payload)
        compare_json_path, compare_md_path = write_previous_comparison_files(output_dir, previous_comparison)
        print_previous_comparison_summary(previous_comparison)
        print(f"Previous delta artifacts: {compare_json_path}, {compare_md_path}")

    # ── Regression gate ───────────────────────────────────────────────────────
    if baseline_summary_payload is not None:
        report = benchmark.get("baseline_regression_gate")
        if isinstance(report, dict):
            report_json_path, report_md_path = write_baseline_regression_files(output_dir, report)
            print(f"Baseline gate artifacts: {report_json_path}, {report_md_path}")
        threshold = float(getattr(args, "regression_threshold", 2.0))
        if check_regression(
            benchmark,
            baseline_summary_payload,
            threshold,
            row_targets=row_fidelity_targets_filter,
        ):
            return 1

    return 0
