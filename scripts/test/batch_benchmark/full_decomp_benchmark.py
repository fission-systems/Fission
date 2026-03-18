#!/usr/bin/env python3
from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT_DIR = Path(__file__).resolve().parents[3]
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

from grand_finale_support.metrics import collect_code_metrics, load_struct_pointer_aliases
from grand_finale_support.resource_monitor import (
    HAS_PSUTIL,
    run_popen_with_resource_monitor,
    start_self_resource_monitor,
)


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
        help="Directory to write benchmark artifacts into",
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
        help="Path to a prebuilt fission_cli binary with native_decomp enabled",
    )
    parser.add_argument(
        "--profile",
        choices=("balanced", "quality", "speed"),
        default="balanced",
        help="Fission decompiler profile",
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
    return parser.parse_args()


def resolve_binary(path: Path) -> Path:
    binary = path.expanduser().resolve()
    if not binary.is_file():
        raise FileNotFoundError(f"binary not found: {binary}")
    return binary


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
        "Build it first with native_decomp enabled. "
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
    compiler_id: str | None,
    struct_ptr_aliases: dict[str, str],
    public_engine: str,
    limit: int | None = None,
) -> dict[str, Any]:
    cli_engine = "legacy" if public_engine == "legacy" else "mlil_preview"
    raw_output_path = output_dir / f"{public_engine}_full.json"
    stdout_log_path = output_dir / f"{public_engine}_stdout.log"
    stderr_log_path = output_dir / f"{public_engine}_stderr.log"
    temp_output = tempfile.NamedTemporaryFile(prefix="fission-benchmark-", suffix=".json", delete=False)
    temp_output.close()

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
    if compiler_id:
        cmd.extend(["--compiler-id", compiler_id])
    if limit is not None:
        cmd.extend(["--decomp-limit", str(limit)])

    env = os.environ.copy()
    bin_dir = str(fission_bin.parent)
    add_library_search_path(env, "DYLD_LIBRARY_PATH", bin_dir)
    add_library_search_path(env, "LD_LIBRARY_PATH", bin_dir)

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
        stdout_log_path.write_text(completed.stdout, encoding="utf-8")
        stderr_log_path.write_text(completed.stderr, encoding="utf-8")
        if completed.returncode != 0:
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
        try:
            os.unlink(temp_output.name)
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
        }

    meta = dict(payload.get("_meta", {}))
    meta["wall_clock_sec"] = round(wall_clock_sec, 6)
    meta["raw_output_path"] = str(raw_output_path)
    meta["public_engine"] = public_engine
    meta["cache_mode"] = "warm"
    if resources:
        meta["resources"] = resources

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
) -> dict[str, Any]:
    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)

    try:
        import pyghidra
    except ImportError as exc:
        raise RuntimeError("pyghidra is not installed. Run `pip install pyghidra`.") from exc

    pyghidra.start()

    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    monitor = ConsoleTaskMonitor()
    raw_output_path = output_dir / "ghidra_full.json"
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

    if HAS_PSUTIL and res_stop is not None and res_thread is not None:
        res_stop.set()
        res_thread.join(timeout=3.0)

    ghidra_resources = res_holder if res_holder else {}

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

    return {
        "meta": payload["_meta"],
        "entries": entries,
        "raw_output_path": str(raw_output_path),
    }


def summarize_engine_failures(entries: dict[str, dict[str, Any]]) -> dict[str, Any]:
    return {
        "reported_success_count": sum(1 for entry in entries.values() if entry.get("reported_success", False)),
        "success_count": sum(1 for entry in entries.values() if entry.get("success")),
        "timeout_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "timeout"),
        "explicit_error_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "explicit_error"),
        "synthetic_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "synthetic_failure"),
        "unknown_failure_count": sum(1 for entry in entries.values() if entry.get("failure_kind") == "unknown_failure"),
    }


def summarize_engine_quality(entries: dict[str, dict[str, Any]], *, preview: bool = False) -> dict[str, Any]:
    success_entries = [entry for entry in entries.values() if entry.get("success")]
    goto_values = [int((entry.get("metrics") or {}).get("goto_count", 0)) for entry in success_entries]
    label_values = [int((entry.get("metrics") or {}).get("top_level_label_count", 0)) for entry in success_entries]
    return {
        "goto_total": sum(goto_values),
        "goto_median": round(statistics.median(goto_values), 3) if goto_values else 0.0,
        "top_level_label_total": sum(label_values),
        "top_level_label_median": round(statistics.median(label_values), 3) if label_values else 0.0,
        "empty_if_total": sum(int((entry.get("metrics") or {}).get("empty_if_count", 0)) for entry in success_entries),
        "constant_if_total": sum(int((entry.get("metrics") or {}).get("constant_if_count", 0)) for entry in success_entries),
        "preview_direct_success_count": sum(1 for entry in success_entries if preview and entry.get("engine_used") == "mlil_preview" and not entry.get("fell_back")),
        "legacy_fallback_count": sum(1 for entry in entries.values() if preview and entry.get("fell_back")),
    }


def build_pairwise_engine_comparison(
    left_label: str,
    left: dict[str, Any],
    right_label: str,
    right: dict[str, Any],
) -> dict[str, Any]:
    left_entries = left["entries"]
    right_entries = right["entries"]
    left_addrs = set(left_entries)
    right_addrs = set(right_entries)
    shared_addrs = sorted(left_addrs & right_addrs, key=lambda addr: int(addr, 16))
    left_only = sorted(left_addrs - right_addrs, key=lambda addr: int(addr, 16))
    right_only = sorted(right_addrs - left_addrs, key=lambda addr: int(addr, 16))
    rows: list[dict[str, Any]] = []
    successful_rows: list[dict[str, Any]] = []
    aggregate_left_parts: list[str] = []
    aggregate_right_parts: list[str] = []

    for address in shared_addrs:
        left_entry = left_entries[address]
        right_entry = right_entries[address]
        raw_similarity = similarity_percent(left_entry.get("code", ""), right_entry.get("code", ""))
        normalized_similarity = similarity_percent(left_entry.get("normalized_code", ""), right_entry.get("normalized_code", ""))
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
        }
        rows.append(row)
        if left_entry.get("success") and right_entry.get("success"):
            successful_rows.append(row)
            aggregate_left_parts.append(left_entry.get("normalized_code", ""))
            aggregate_right_parts.append(right_entry.get("normalized_code", ""))

    normalized_scores = [row["normalized_similarity"] for row in successful_rows]
    raw_scores = [row["raw_similarity"] for row in successful_rows]
    return {
        "left_label": left_label,
        "right_label": right_label,
        "comparisons": rows,
        "left_only": left_only,
        "right_only": right_only,
        "summary": {
            "shared_count": len(shared_addrs),
            "left_only_count": len(left_only),
            "right_only_count": len(right_only),
            "both_success_count": len(successful_rows),
            "aggregate_normalized_similarity": similarity_percent("\n".join(aggregate_left_parts), "\n".join(aggregate_right_parts)),
            "avg_normalized_similarity": round(statistics.fmean(normalized_scores), 2) if normalized_scores else 0.0,
            "median_normalized_similarity": round(statistics.median(normalized_scores), 2) if normalized_scores else 0.0,
            "avg_raw_similarity": round(statistics.fmean(raw_scores), 2) if raw_scores else 0.0,
        },
    }


def build_comparison(
    binary_path: Path,
    pyghidra: dict[str, Any],
    legacy: dict[str, Any],
    preview: dict[str, Any],
) -> dict[str, Any]:
    pair_py_legacy = build_pairwise_engine_comparison("pyghidra", pyghidra, "legacy", legacy)
    pair_legacy_preview = build_pairwise_engine_comparison("legacy", legacy, "preview", preview)
    pair_py_preview = build_pairwise_engine_comparison("pyghidra", pyghidra, "preview", preview)

    summary = {
        "binary": str(binary_path),
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "cache_mode": "warm",
        "engines": {
            "pyghidra": {
                **pyghidra["meta"],
                "function_count": len(pyghidra["entries"]),
                **summarize_engine_failures(pyghidra["entries"]),
                **summarize_engine_quality(pyghidra["entries"]),
            },
            "legacy": {
                **legacy["meta"],
                "function_count": len(legacy["entries"]),
                **summarize_engine_failures(legacy["entries"]),
                **summarize_engine_quality(legacy["entries"]),
            },
            "preview": {
                **preview["meta"],
                "function_count": len(preview["entries"]),
                **summarize_engine_failures(preview["entries"]),
                **summarize_engine_quality(preview["entries"], preview=True),
            },
        },
        "coverage": {
            "pyghidra_vs_legacy": pair_py_legacy["summary"],
            "legacy_vs_preview": pair_legacy_preview["summary"],
            "pyghidra_vs_preview": pair_py_preview["summary"],
        },
        "quality": {
            "pyghidra_vs_legacy": pair_py_legacy["summary"],
            "legacy_vs_preview": pair_legacy_preview["summary"],
            "pyghidra_vs_preview": pair_py_preview["summary"],
        },
        "resources": {
            "pyghidra": pyghidra["meta"].get("resources", {}),
            "legacy": legacy["meta"].get("resources", {}),
            "preview": preview["meta"].get("resources", {}),
        },
        "speed": {
            "pyghidra": {
                "init_sec": round(float(pyghidra["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(pyghidra["meta"].get("total_decomp_sec", 0.0)), 6),
                "wall_sec": round(float(pyghidra["meta"].get("wall_clock_sec", 0.0)), 6),
            },
            "legacy": {
                "init_sec": round(float(legacy["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(legacy["meta"].get("total_decomp_sec", 0.0)), 6),
                "postprocess_sec": round(float(legacy["meta"].get("total_postprocess_sec", 0.0)), 6),
                "wall_sec": round(float(legacy["meta"].get("wall_clock_sec", 0.0)), 6),
                "wall_speedup_vs_pyghidra": round(float(pyghidra["meta"].get("wall_clock_sec", 0.0)) / max(float(legacy["meta"].get("wall_clock_sec", 0.0)), 1e-9), 3),
            },
            "preview": {
                "init_sec": round(float(preview["meta"].get("init_sec", 0.0)), 6),
                "total_decomp_sec": round(float(preview["meta"].get("total_decomp_sec", 0.0)), 6),
                "postprocess_sec": round(float(preview["meta"].get("total_postprocess_sec", 0.0)), 6),
                "wall_sec": round(float(preview["meta"].get("wall_clock_sec", 0.0)), 6),
                "wall_speedup_vs_legacy": round(float(legacy["meta"].get("wall_clock_sec", 0.0)) / max(float(preview["meta"].get("wall_clock_sec", 0.0)), 1e-9), 3),
            },
        },
        "samples": {
            "pyghidra_vs_legacy_lowest_similarity": sorted(pair_py_legacy["comparisons"], key=lambda row: row["normalized_similarity"])[:20],
            "legacy_vs_preview_lowest_similarity": sorted(pair_legacy_preview["comparisons"], key=lambda row: row["normalized_similarity"])[:20],
            "pyghidra_vs_preview_lowest_similarity": sorted(pair_py_preview["comparisons"], key=lambda row: row["normalized_similarity"])[:20],
            "legacy_hot_path_phases": summarize_native_hot_paths(legacy["entries"]),
            "preview_hot_path_phases": summarize_native_hot_paths(preview["entries"]),
        },
        "public_summary_line": "",
    }

    summary["public_summary_line"] = (
        f"legacy vs pyghidra wall speedup {summary['speed']['legacy']['wall_speedup_vs_pyghidra']}x; "
        f"preview vs legacy wall speedup {summary['speed']['preview']['wall_speedup_vs_legacy']}x; "
        f"preview direct-success {summary['engines']['preview']['preview_direct_success_count']}/{summary['engines']['preview']['function_count']}"
    )

    return {
        "summary": summary,
        "pairwise": {
            "pyghidra_vs_legacy": pair_py_legacy,
            "legacy_vs_preview": pair_legacy_preview,
            "pyghidra_vs_preview": pair_py_preview,
        },
        "engines": {
            "pyghidra": pyghidra,
            "legacy": legacy,
            "preview": preview,
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
    low_rows = summary["samples"]["legacy_vs_preview_lowest_similarity"]
    hot_rows = summary["samples"].get("preview_hot_path_phases", [])
    lines = [
        f"# Whole Decomp Benchmark: {Path(summary['binary']).name}",
        "",
        "## Why 3-Way",
        "",
        "- `pyghidra`: Python-host baseline",
        "- `legacy`: native FFI baseline",
        "- `preview`: Rust preview pipeline",
        "",
        "## Summary",
        "",
        f"- Generated: {summary['generated_at']}",
        f"- Cache mode: `{summary['cache_mode']}`",
        f"- Public summary: {summary['public_summary_line']}",
        "",
        "## Speed",
        "",
        f"- pyghidra wall: {summary['speed']['pyghidra']['wall_sec']:.3f}s",
        f"- legacy wall: {summary['speed']['legacy']['wall_sec']:.3f}s",
        f"- preview wall: {summary['speed']['preview']['wall_sec']:.3f}s",
        f"- legacy vs pyghidra speedup: {summary['speed']['legacy']['wall_speedup_vs_pyghidra']:.3f}x",
        f"- preview vs legacy speedup: {summary['speed']['preview']['wall_speedup_vs_legacy']:.3f}x",
        "",
        "## Engine Coverage / Quality",
        "",
        "| Engine | Success | Failure | Timeout | goto total | label total | empty if | constant if |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for label, engine in summary["engines"].items():
        lines.append(
            f"| `{label}` | {engine.get('success_count', 0)} | {engine.get('failure_count', 0)} | "
            f"{engine.get('timeout_count', 0)} | {engine.get('goto_total', 0)} | "
            f"{engine.get('top_level_label_total', 0)} | {engine.get('empty_if_total', 0)} | "
            f"{engine.get('constant_if_total', 0)} |"
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
    lines.extend(["", "## Resources (requires psutil)", ""])
    resources = summary.get("resources", {})
    if any(resources.get(label, {}).get("max_rss_mb", 0.0) for label in ("pyghidra", "legacy", "preview")):
        for label in ("pyghidra", "legacy", "preview"):
            res = resources.get(label, {})
            lines.append(
                f"- {label}: max RSS {res.get('max_rss_mb', 0):.2f} MB, avg CPU {res.get('avg_cpu_pct', 0):.2f}%"
            )
    else:
        lines.append("- Install `pip install psutil` for resource usage metrics.")
    lines.extend(["", "## Representative Lowest Similarity (`legacy_vs_preview`)", ""])

    if low_rows:
        lines.append("| Address | Legacy | Preview | Norm Similarity |")
        lines.append("|---|---|---|---:|")
        for row in low_rows[:10]:
            lines.append(
                f"| `{row['address']}` | `{row['legacy_name']}` | `{row['preview_name']}` | "
                f"{row['normalized_similarity']:.2f}% |"
            )
    else:
        lines.append("- No shared successful functions to compare.")

    lines.extend(["", "## Preview Native Hot Paths", ""])
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

    lines.extend(
        [
            "",
            "## Artifacts",
            "",
            "- `legacy_full.json`: raw native FFI legacy output",
            "- `preview_full.json`: raw Rust preview output",
            "- `ghidra_full.json`: raw pyghidra whole-decomp output",
            "- `benchmark_summary.json`: merged metrics and per-function comparison",
        ]
    )

    with summary_md_path.open("w", encoding="utf-8") as handle:
        handle.write("\n".join(lines) + "\n")

    return summary_json_path, summary_md_path


def print_console_summary(summary: dict[str, Any], output_dir: Path) -> None:
    print("\n=== Whole Decomp Benchmark Summary ===")
    print(f"Binary: {summary['binary']}")
    print(summary["public_summary_line"])
    print(
        f"pyghidra wall={summary['speed']['pyghidra']['wall_sec']:.3f}s | "
        f"legacy wall={summary['speed']['legacy']['wall_sec']:.3f}s | "
        f"preview wall={summary['speed']['preview']['wall_sec']:.3f}s"
    )
    print(
        f"legacy vs pyghidra similarity={summary['quality']['pyghidra_vs_legacy']['avg_normalized_similarity']:.2f}% | "
        f"legacy vs preview similarity={summary['quality']['legacy_vs_preview']['avg_normalized_similarity']:.2f}%"
    )
    res = summary.get("resources", {})
    if any(res.get(label, {}).get("max_rss_mb", 0.0) for label in ("pyghidra", "legacy", "preview")):
        print(
            f"Resources: pyghidra max_rss={res.get('pyghidra', {}).get('max_rss_mb', 0):.2f}MB | "
            f"legacy max_rss={res.get('legacy', {}).get('max_rss_mb', 0):.2f}MB | "
            f"preview max_rss={res.get('preview', {}).get('max_rss_mb', 0):.2f}MB"
        )
    elif not HAS_PSUTIL:
        print("Resources: (install psutil for metrics)")
    hot_rows = summary["samples"].get("preview_hot_path_phases", [])
    if hot_rows:
        top = hot_rows[0]
        phases = ", ".join(
            f"{phase['phase']}={phase['ms']:.3f}ms" for phase in top["top_native_phases"]
        )
        print(
            f"Top preview hot path: {top['address']} {top['name']} "
            f"(decomp={top['decomp_sec']:.6f}s, {phases}, "
            f"callee={top['callee_preanalysis_count']}, "
            f"callgraph={top['callgraph_reanalysis_count']})"
        )
    print(f"Artifacts: {output_dir}")


def main() -> int:
    args = parse_args()
    binary_path = resolve_binary(args.binary)
    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    fission_bin = resolve_fission_bin(args.fission_bin)

    timestamp = time.strftime("%Y%m%d-%H%M%S")
    output_dir = ensure_dir(
        args.output_dir.resolve()
        if args.output_dir
        else DEFAULT_RESULTS_DIR / f"{binary_path.stem}-{timestamp}"
    )

    print(f"[*] Binary: {binary_path}")
    print(f"[*] Fission CLI: {fission_bin}")
    print(f"[*] Ghidra dir: {ghidra_dir}")
    print(f"[*] Output dir: {output_dir}")

    if args.limit:
        print(f"[*] Limit: first {args.limit} functions only")

    struct_ptr_aliases = load_struct_pointer_aliases(BASE_TYPES_JSON)

    legacy = run_fission_full(
        binary_path=binary_path,
        fission_bin=fission_bin,
        output_dir=output_dir,
        timeout_sec=args.timeout,
        profile=args.profile,
        compiler_id=args.compiler_id,
        struct_ptr_aliases=struct_ptr_aliases,
        public_engine="legacy",
        limit=args.limit,
    )
    print(
        f"[*] Legacy complete: functions={len(legacy['entries'])}, "
        f"wall={float(legacy['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    preview = run_fission_full(
        binary_path=binary_path,
        fission_bin=fission_bin,
        output_dir=output_dir,
        timeout_sec=args.timeout,
        profile=args.profile,
        compiler_id=args.compiler_id,
        struct_ptr_aliases=struct_ptr_aliases,
        public_engine="preview",
        limit=args.limit,
    )
    print(
        f"[*] Preview complete: functions={len(preview['entries'])}, "
        f"wall={float(preview['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    ghidra = run_ghidra_full(
        binary_path=binary_path,
        ghidra_dir=ghidra_dir,
        output_dir=output_dir,
        per_function_timeout_sec=args.ghidra_func_timeout,
        skip_thunks=args.skip_thunks,
        struct_ptr_aliases=struct_ptr_aliases,
        limit=args.limit,
    )
    print(
        f"[*] Ghidra complete: functions={len(ghidra['entries'])}, "
        f"wall={float(ghidra['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    benchmark = build_comparison(binary_path, ghidra, legacy, preview)
    write_summary_files(output_dir, benchmark)
    print_console_summary(benchmark["summary"], output_dir)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[-] {exc}", file=sys.stderr)
        raise SystemExit(1)
