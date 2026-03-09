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
import threading
import time
from pathlib import Path
from typing import Any

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_RESULTS_DIR = ROOT_DIR / "artifacts" / "batch_benchmark"
DEFAULT_GHIDRA_DIRS = (
    ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC",
    ROOT_DIR / "ghidra_11.4.2_PUBLIC",
)

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


def _collect_process_resources(
    pid: int,
    interval_sec: float,
    result_holder: dict[str, Any],
) -> None:
    """Background thread: sample process until it exits. Writes into result_holder."""
    rss_list: list[float] = []
    cpu_list: list[float] = []
    try:
        proc = psutil.Process(pid)
        proc.cpu_percent()
        while True:
            try:
                if not proc.is_running():
                    break
            except psutil.NoSuchProcess:
                break
            try:
                rss_list.append(proc.memory_info().rss / (1024 * 1024))
                cpu_list.append(proc.cpu_percent(interval=interval_sec))
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                break
    except psutil.NoSuchProcess:
        pass
    result_holder["max_rss_mb"] = round(max(rss_list), 2) if rss_list else 0.0
    result_holder["avg_rss_mb"] = round(statistics.fmean(rss_list), 2) if rss_list else 0.0
    result_holder["avg_cpu_pct"] = round(statistics.fmean(cpu_list), 2) if cpu_list else 0.0
    result_holder["max_cpu_pct"] = round(max(cpu_list), 2) if cpu_list else 0.0
    result_holder["sample_count"] = len(rss_list)


def run_popen_with_resource_monitor(
    popen: subprocess.Popen[Any],
    timeout_sec: float,
    interval_sec: float = 0.5,
) -> tuple[subprocess.CompletedProcess[Any], dict[str, Any]]:
    """Run popen, monitor resources in background, wait for exit. Returns (completed, resources)."""
    result_holder: dict[str, Any] = {}
    t = threading.Thread(
        target=_collect_process_resources,
        args=(popen.pid, interval_sec, result_holder),
        daemon=True,
    )
    t.start()
    try:
        returncode = popen.wait(timeout=timeout_sec)
        t.join(timeout=5.0)
        stdout = popen.stdout.read() if popen.stdout else ""
        stderr = popen.stderr.read() if popen.stderr else ""
        completed = subprocess.CompletedProcess(
            args=popen.args, returncode=returncode, stdout=stdout, stderr=stderr
        )
        return completed, result_holder
    except subprocess.TimeoutExpired:
        t.join(timeout=1.0)
        popen.kill()
        try:
            stdout = popen.stdout.read() if popen.stdout else ""
            stderr = popen.stderr.read() if popen.stderr else ""
        except Exception:
            stdout, stderr = "", ""
        popen.wait(timeout=5)
        raise subprocess.TimeoutExpired(popen.args, timeout_sec, stdout, stderr)


def start_self_resource_monitor(
    interval_sec: float = 0.5,
) -> tuple[threading.Thread, dict[str, Any], threading.Event]:
    """Start background thread sampling current process. Returns (thread, result_holder, stop_event)."""
    result_holder: dict[str, Any] = {}
    stop_event = threading.Event()

    def collect() -> None:
        rss_list: list[float] = []
        cpu_list: list[float] = []
        try:
            proc = psutil.Process(os.getpid())
            proc.cpu_percent()
            while not stop_event.is_set():
                try:
                    rss_list.append(proc.memory_info().rss / (1024 * 1024))
                    cpu_list.append(proc.cpu_percent(interval=interval_sec))
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    break
        except Exception:
            pass
        result_holder["max_rss_mb"] = round(max(rss_list), 2) if rss_list else 0.0
        result_holder["avg_rss_mb"] = round(statistics.fmean(rss_list), 2) if rss_list else 0.0
        result_holder["avg_cpu_pct"] = round(statistics.fmean(cpu_list), 2) if cpu_list else 0.0
        result_holder["max_cpu_pct"] = round(max(cpu_list), 2) if cpu_list else 0.0
        result_holder["sample_count"] = len(rss_list)

    t = threading.Thread(target=collect, daemon=True)
    t.start()
    return t, result_holder, stop_event


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
    limit: int | None = None,
) -> dict[str, Any]:
    raw_output_path = output_dir / "fission_full.json"
    stdout_log_path = output_dir / "fission_stdout.log"
    stderr_log_path = output_dir / "fission_stderr.log"
    temp_output = tempfile.NamedTemporaryFile(prefix="fission-benchmark-", suffix=".json", delete=False)
    temp_output.close()

    cmd = [
        str(fission_bin),
        str(binary_path),
        "--decomp-all",
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
        }

    meta = dict(payload.get("_meta", {}))
    meta["wall_clock_sec"] = round(wall_clock_sec, 6)
    meta["raw_output_path"] = str(raw_output_path)
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


def build_comparison(
    binary_path: Path,
    fission: dict[str, Any],
    ghidra: dict[str, Any],
) -> dict[str, Any]:
    fission_entries = fission["entries"]
    ghidra_entries = ghidra["entries"]

    fission_addrs = set(fission_entries)
    ghidra_addrs = set(ghidra_entries)
    shared_addrs = sorted(fission_addrs & ghidra_addrs, key=lambda addr: int(addr, 16))
    fission_only = sorted(fission_addrs - ghidra_addrs, key=lambda addr: int(addr, 16))
    ghidra_only = sorted(ghidra_addrs - fission_addrs, key=lambda addr: int(addr, 16))

    comparisons: list[dict[str, Any]] = []
    successful_pairs: list[dict[str, Any]] = []
    aggregate_fission_parts: list[str] = []
    aggregate_ghidra_parts: list[str] = []

    for address in shared_addrs:
        f_entry = fission_entries[address]
        g_entry = ghidra_entries[address]

        raw_similarity = similarity_percent(f_entry.get("code", ""), g_entry.get("code", ""))
        normalized_similarity = similarity_percent(
            f_entry.get("normalized_code", ""),
            g_entry.get("normalized_code", ""),
        )

        row = {
            "address": address,
            "fission_name": f_entry.get("name", ""),
            "ghidra_name": g_entry.get("name", ""),
            "fission_success": f_entry.get("success", False),
            "ghidra_success": g_entry.get("success", False),
            "fission_failure_kind": f_entry.get("failure_kind"),
            "ghidra_failure_kind": g_entry.get("failure_kind"),
            "fission_decomp_sec": f_entry.get("decomp_sec", 0.0),
            "ghidra_decomp_sec": g_entry.get("decomp_sec", 0.0),
            "raw_similarity": raw_similarity,
            "normalized_similarity": normalized_similarity,
            "fission_error": f_entry.get("error"),
            "ghidra_error": g_entry.get("error"),
            "fission_native_timing": f_entry.get("native_timing"),
        }
        comparisons.append(row)

        if f_entry.get("success") and g_entry.get("success"):
            successful_pairs.append(row)
            aggregate_fission_parts.append(f_entry.get("normalized_code", ""))
            aggregate_ghidra_parts.append(g_entry.get("normalized_code", ""))

    normalized_scores = [row["normalized_similarity"] for row in successful_pairs]
    raw_scores = [row["raw_similarity"] for row in successful_pairs]

    fission_failure_breakdown = {
        "reported_success_count": sum(
            1 for entry in fission_entries.values() if entry.get("reported_success", False)
        ),
        "success_count": sum(1 for entry in fission_entries.values() if entry["success"]),
        "explicit_error_count": sum(
            1 for entry in fission_entries.values() if entry.get("failure_kind") == "explicit_error"
        ),
        "synthetic_failure_count": sum(
            1
            for entry in fission_entries.values()
            if entry.get("failure_kind") == "synthetic_failure"
        ),
        "unknown_failure_count": sum(
            1 for entry in fission_entries.values() if entry.get("failure_kind") == "unknown_failure"
        ),
    }
    ghidra_failure_breakdown = {
        "success_count": sum(1 for entry in ghidra_entries.values() if entry["success"]),
        "explicit_error_count": sum(
            1 for entry in ghidra_entries.values() if entry.get("failure_kind") == "explicit_error"
        ),
        "synthetic_failure_count": sum(
            1
            for entry in ghidra_entries.values()
            if entry.get("failure_kind") == "synthetic_failure"
        ),
        "unknown_failure_count": sum(
            1 for entry in ghidra_entries.values() if entry.get("failure_kind") == "unknown_failure"
        ),
    }

    aggregate_similarity = similarity_percent(
        "\n".join(aggregate_fission_parts),
        "\n".join(aggregate_ghidra_parts),
    )

    summary = {
        "binary": str(binary_path),
        "fission": {
            **fission["meta"],
            "function_count": len(fission_entries),
            **fission_failure_breakdown,
        },
        "ghidra": {
            **ghidra["meta"],
            "function_count": len(ghidra_entries),
            **ghidra_failure_breakdown,
        },
        "matching": {
            "shared_count": len(shared_addrs),
            "fission_only_count": len(fission_only),
            "ghidra_only_count": len(ghidra_only),
            "both_success_count": len(successful_pairs),
            "coverage_vs_fission_pct": round(
                len(shared_addrs) * 100.0 / len(fission_entries), 2
            )
            if fission_entries
            else 0.0,
            "coverage_vs_ghidra_pct": round(
                len(shared_addrs) * 100.0 / len(ghidra_entries), 2
            )
            if ghidra_entries
            else 0.0,
        },
        "quality": {
            "aggregate_normalized_similarity": aggregate_similarity,
            "avg_normalized_similarity": round(statistics.fmean(normalized_scores), 2)
            if normalized_scores
            else 0.0,
            "median_normalized_similarity": round(statistics.median(normalized_scores), 2)
            if normalized_scores
            else 0.0,
            "min_normalized_similarity": round(min(normalized_scores), 2)
            if normalized_scores
            else 0.0,
            "max_normalized_similarity": round(max(normalized_scores), 2)
            if normalized_scores
            else 0.0,
            "avg_raw_similarity": round(statistics.fmean(raw_scores), 2)
            if raw_scores
            else 0.0,
        },
        "resources": {
            "fission_max_rss_mb": round(
                float(fission["meta"].get("resources", {}).get("max_rss_mb", 0.0)), 2
            ),
            "fission_avg_cpu_pct": round(
                float(fission["meta"].get("resources", {}).get("avg_cpu_pct", 0.0)), 2
            ),
            "ghidra_max_rss_mb": round(
                float(ghidra["meta"].get("resources", {}).get("max_rss_mb", 0.0)), 2
            ),
            "ghidra_avg_cpu_pct": round(
                float(ghidra["meta"].get("resources", {}).get("avg_cpu_pct", 0.0)), 2
            ),
        },
        "speed": {
            "fission_total_sec": round(float(fission["meta"].get("total_decomp_sec", 0.0)), 6),
            "fission_postprocess_sec": round(
                float(fission["meta"].get("total_postprocess_sec", 0.0)), 6
            ),
            "ghidra_total_sec": round(float(ghidra["meta"].get("total_decomp_sec", 0.0)), 6),
            "fission_init_sec": round(float(fission["meta"].get("init_sec", 0.0)), 6),
            "ghidra_init_sec": round(float(ghidra["meta"].get("init_sec", 0.0)), 6),
            "fission_wall_sec": round(float(fission["meta"].get("wall_clock_sec", 0.0)), 6),
            "ghidra_wall_sec": round(float(ghidra["meta"].get("wall_clock_sec", 0.0)), 6),
            "wall_speedup_vs_ghidra": round(
                float(ghidra["meta"].get("wall_clock_sec", 0.0))
                / max(float(fission["meta"].get("wall_clock_sec", 0.0)), 1e-9),
                3,
            ),
        },
        "samples": {
            "lowest_similarity": sorted(
                successful_pairs, key=lambda row: row["normalized_similarity"]
            )[:20],
            "fission_only_addresses": fission_only[:20],
            "ghidra_only_addresses": ghidra_only[:20],
            "fission_hot_path_phases": summarize_native_hot_paths(fission_entries),
        },
    }

    return {
        "summary": summary,
        "comparisons": comparisons,
        "fission_only": fission_only,
        "ghidra_only": ghidra_only,
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
    low_rows = summary["samples"]["lowest_similarity"]
    hot_rows = summary["samples"].get("fission_hot_path_phases", [])
    lines = [
        f"# Whole Decomp Benchmark: {Path(summary['binary']).name}",
        "",
        "## Summary",
        "",
        f"- Shared functions: {summary['matching']['shared_count']}",
        f"- Both decompiled successfully: {summary['matching']['both_success_count']}",
        f"- Aggregate normalized similarity: {summary['quality']['aggregate_normalized_similarity']:.2f}%",
        f"- Average normalized similarity: {summary['quality']['avg_normalized_similarity']:.2f}%",
        f"- Fission wall clock: {summary['speed']['fission_wall_sec']:.3f}s",
        f"- Ghidra wall clock: {summary['speed']['ghidra_wall_sec']:.3f}s",
        f"- Wall speedup vs Ghidra: {summary['speed']['wall_speedup_vs_ghidra']:.3f}x",
        "",
        "## Resources (requires psutil)",
        "",
    ]
    res = summary.get("resources", {})
    if res.get("fission_max_rss_mb") or res.get("ghidra_max_rss_mb"):
        lines.extend(
            [
                f"- Fission max RSS: {res.get('fission_max_rss_mb', 0):.2f} MB, avg CPU: {res.get('fission_avg_cpu_pct', 0):.2f}%",
                f"- Ghidra max RSS: {res.get('ghidra_max_rss_mb', 0):.2f} MB, avg CPU: {res.get('ghidra_avg_cpu_pct', 0):.2f}%",
            ]
        )
    else:
        lines.append("- Install `pip install psutil` for resource usage metrics.")
    lines.extend(
        [
            "",
            "## Coverage",
            "",
            f"- Fission functions: {summary['fission']['function_count']} (success {summary['fission']['success_count']})",
            f"- Fission reported-success before cleanup: {summary['fission']['reported_success_count']}",
            f"- Fission explicit errors: {summary['fission']['explicit_error_count']}",
            f"- Fission synthetic failures: {summary['fission']['synthetic_failure_count']}",
            f"- Ghidra functions: {summary['ghidra']['function_count']} (success {summary['ghidra']['success_count']})",
            f"- Fission-only addresses: {summary['matching']['fission_only_count']}",
            f"- Ghidra-only addresses: {summary['matching']['ghidra_only_count']}",
            "",
            "## Speed Breakdown",
            "",
            f"- Fission init: {summary['speed']['fission_init_sec']:.3f}s",
            f"- Fission pure decomp: {summary['speed']['fission_total_sec']:.3f}s",
            f"- Fission postprocess: {summary['speed']['fission_postprocess_sec']:.3f}s",
            f"- Ghidra pure decomp: {summary['speed']['ghidra_total_sec']:.3f}s",
            "",
            "## Lowest Similarity Samples",
            "",
        ],
    )

    if low_rows:
        lines.append("| Address | Fission | Ghidra | Norm Similarity |")
        lines.append("|---|---|---|---:|")
        for row in low_rows[:10]:
            lines.append(
                f"| `{row['address']}` | `{row['fission_name']}` | `{row['ghidra_name']}` | "
                f"{row['normalized_similarity']:.2f}% |"
            )
    else:
        lines.append("- No shared successful functions to compare.")

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

    lines.extend(
        [
            "",
            "## Artifacts",
            "",
            "- `fission_full.json`: raw Fission whole-decomp output",
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
    print(
        f"Shared={summary['matching']['shared_count']} "
        f"BothSuccess={summary['matching']['both_success_count']} "
        f"FissionOnly={summary['matching']['fission_only_count']} "
        f"GhidraOnly={summary['matching']['ghidra_only_count']}"
    )
    print(
        f"Quality: aggregate={summary['quality']['aggregate_normalized_similarity']:.2f}% "
        f"avg={summary['quality']['avg_normalized_similarity']:.2f}% "
        f"median={summary['quality']['median_normalized_similarity']:.2f}%"
    )
    print(
        f"Fission: init={summary['speed']['fission_init_sec']:.3f}s "
        f"decomp={summary['speed']['fission_total_sec']:.3f}s "
        f"post={summary['speed']['fission_postprocess_sec']:.3f}s "
        f"wall={summary['speed']['fission_wall_sec']:.3f}s"
    )
    print(
        f"Ghidra:  init={summary['speed']['ghidra_init_sec']:.3f}s "
        f"decomp={summary['speed']['ghidra_total_sec']:.3f}s "
        f"wall={summary['speed']['ghidra_wall_sec']:.3f}s"
    )
    print(
        f"Fission failures: explicit={summary['fission']['explicit_error_count']} "
        f"synthetic={summary['fission']['synthetic_failure_count']} "
        f"reported_success={summary['fission']['reported_success_count']}"
    )
    print(
        f"Wall speedup vs Ghidra: {summary['speed']['wall_speedup_vs_ghidra']:.3f}x"
    )
    res = summary.get("resources", {})
    if res.get("fission_max_rss_mb") or res.get("ghidra_max_rss_mb"):
        print(
            f"Resources: Fission max_rss={res.get('fission_max_rss_mb', 0):.2f}MB avg_cpu={res.get('fission_avg_cpu_pct', 0):.2f}% | "
            f"Ghidra max_rss={res.get('ghidra_max_rss_mb', 0):.2f}MB avg_cpu={res.get('ghidra_avg_cpu_pct', 0):.2f}%"
        )
    elif not HAS_PSUTIL:
        print("Resources: (install psutil for metrics)")
    hot_rows = summary["samples"].get("fission_hot_path_phases", [])
    if hot_rows:
        top = hot_rows[0]
        phases = ", ".join(
            f"{phase['phase']}={phase['ms']:.3f}ms" for phase in top["top_native_phases"]
        )
        print(
            f"Top Fission hot path: {top['address']} {top['name']} "
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

    fission = run_fission_full(
        binary_path=binary_path,
        fission_bin=fission_bin,
        output_dir=output_dir,
        timeout_sec=args.timeout,
        profile=args.profile,
        compiler_id=args.compiler_id,
        limit=args.limit,
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
        limit=args.limit,
    )
    print(
        f"[*] Ghidra complete: functions={len(ghidra['entries'])}, "
        f"wall={float(ghidra['meta'].get('wall_clock_sec', 0.0)):.3f}s"
    )

    benchmark = build_comparison(binary_path, fission, ghidra)
    write_summary_files(output_dir, benchmark)
    print_console_summary(benchmark["summary"], output_dir)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[-] {exc}", file=sys.stderr)
        raise SystemExit(1)
