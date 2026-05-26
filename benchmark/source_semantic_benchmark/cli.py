import json
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
from collections import Counter
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.config import (
    FISSION_LIST_LINE_RE,
    ROOT_DIR,
    TRAILING_DECORATION_RE,
)
from benchmark.source_semantic_benchmark.models import FissionFunction
from benchmark.source_semantic_benchmark.utils import (
    canonical_address,
    dump_json_pretty,
    load_json,
    rel,
    resolve_path,
)
from benchmark.source_semantic_benchmark.cache import (
    decomp_cache_key,
    list_cache_key,
    load_decomp_cache,
    save_decomp_cache,
    load_list_cache,
    save_list_cache,
)


def run_fission_list(binary_path: Path, fission_bin: Path, timeout_sec: int) -> tuple[list[FissionFunction], str | None]:
    cmd = [str(fission_bin), "list", str(binary_path)]
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
        return [], detail.strip() or "list_failed"

    funcs: list[FissionFunction] = []
    for line in res.stdout.splitlines():
        m = FISSION_LIST_LINE_RE.search(line)
        if not m:
            continue
        name = TRAILING_DECORATION_RE.sub("", m.group(2).strip()).strip()
        funcs.append(FissionFunction(address=canonical_address(m.group(1)), name=name))
    return funcs, None


def run_fission_list_cached(
    binary_path: Path,
    fission_bin: Path,
    timeout_sec: int,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> tuple[list[FissionFunction], str | None]:
    key = list_cache_key(binary_path, fission_bin)
    with cache_lock:
        cached = cache.get(key)
        if cached is not None:
            cache_stats["hit"] += 1
    if cached is not None:
        if cached.get("error"):
            return [], cached["error"]
        funcs = [FissionFunction(address=canonical_address(f["address"]), name=f["name"]) for f in cached.get("functions", [])]
        return funcs, None

    with cache_lock:
        cache_stats["miss"] += 1

    funcs, error = run_fission_list(binary_path, fission_bin, timeout_sec)
    payload: dict[str, Any] = {"format": "list_result_v1", "error": error}
    if not error:
        payload["functions"] = [{"address": f.address, "name": f.name} for f in funcs]

    with cache_lock:
        cache.setdefault(key, payload)
        cache_stats["stored"] += 1
    return funcs, error


def ghidra_headless_path(ghidra_home: Path) -> Path:
    candidates = ["support/analyzeHeadless", "support/analyzeHeadless.bat"]
    for rel_path in candidates:
        full = ghidra_home / rel_path
        if full.exists():
            return full
    raise FileNotFoundError(f"Headless analyzer not found in {ghidra_home}")


def run_ghidra_reference_export(
    binary_path: Path,
    ghidra_home: Path,
    timeout_sec: int,
    output_dir: Path,
    entry_id: str,
    script_dir: Path | None = None,
    export_script: str | None = None,
) -> dict[str, Any]:
    from benchmark.source_semantic_benchmark.config import (
        DEFAULT_GHIDRA_SCRIPT_DIR,
        DEFAULT_GHIDRA_EXPORT_SCRIPT,
    )
    if script_dir is None:
        script_dir = DEFAULT_GHIDRA_SCRIPT_DIR
    if export_script is None:
        export_script = DEFAULT_GHIDRA_EXPORT_SCRIPT

    headless = ghidra_headless_path(ghidra_home)
    start = time.perf_counter()
    with tempfile.TemporaryDirectory(prefix="ghidra-bench-proj-") as tmp:
        proj_dir = Path(tmp)
        out_json_file = proj_dir / "ghidra_export.json"
        cmd = [
            str(headless),
            str(proj_dir),
            "RefProj",
            "-import",
            str(binary_path),
            "-overwrite",
            "-scriptPath",
            str(script_dir),
            "-postScript",
            export_script,
            str(out_json_file),
            "-readOnly",
            "-deleteProject",
        ]
        try:
            res = subprocess.run(
                cmd,
                cwd=ROOT_DIR,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=timeout_sec,
                check=True,
            )
        except subprocess.TimeoutExpired as exc:
            detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
            return {
                "success": False,
                "failure_kind": "timeout",
                "failure_detail": f"ghidra_timeout: {detail[-2000:]}",
                "functions": [],
                "function_count": 0,
                "artifact_path": None,
                "wall_sec": round(time.perf_counter() - start, 6),
                "command": " ".join(shlex.quote(c) for c in cmd),
            }
        except subprocess.CalledProcessError as exc:
            detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
            return {
                "success": False,
                "failure_kind": "command_failed",
                "failure_detail": f"ghidra_failed: {detail[-2000:]}",
                "functions": [],
                "function_count": 0,
                "artifact_path": None,
                "wall_sec": round(time.perf_counter() - start, 6),
                "command": " ".join(shlex.quote(c) for c in cmd),
            }

        if not out_json_file.exists():
            return {
                "success": False,
                "failure_kind": "empty_output",
                "failure_detail": f"ghidra_no_output_file: {res.stderr[-2000:]}",
                "functions": [],
                "function_count": 0,
                "artifact_path": None,
                "wall_sec": round(time.perf_counter() - start, 6),
                "command": " ".join(shlex.quote(c) for c in cmd),
            }

        try:
            data = json.loads(out_json_file.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            return {
                "success": False,
                "failure_kind": "invalid_json",
                "failure_detail": f"ghidra_invalid_json: {exc}",
                "functions": [],
                "function_count": 0,
                "artifact_path": None,
                "wall_sec": round(time.perf_counter() - start, 6),
                "command": " ".join(shlex.quote(c) for c in cmd),
            }

        dest_json_file = output_dir / f"ghidra_export_{entry_id}.json"
        dest_json_file.parent.mkdir(parents=True, exist_ok=True)
        try:
            shutil.copy2(out_json_file, dest_json_file)
            artifact_path = rel(dest_json_file)
        except Exception:
            artifact_path = str(dest_json_file)

        funcs = data.get("functions") or []
        return {
            "success": True,
            "failure_kind": None,
            "failure_detail": None,
            "functions": funcs,
            "function_count": len(funcs),
            "artifact_path": artifact_path,
            "wall_sec": round(time.perf_counter() - start, 6),
            "command": " ".join(shlex.quote(c) for c in cmd),
        }



def ghidra_function_index(functions: list[dict[str, Any]]) -> tuple[list[FissionFunction], dict[str, dict[str, Any]]]:
    index: dict[str, dict[str, Any]] = {}
    fission_format: list[FissionFunction] = []
    for func in functions:
        addr = canonical_address(func.get("address") or 0)
        name = func.get("name") or "noname"
        index[addr] = func
        fission_format.append(FissionFunction(address=addr, name=name))
    return fission_format, index


def parse_json_loose(text: str) -> Any:
    text = text.strip()
    if not text:
        raise json.JSONDecodeError("empty", text, 0)
    starts = [idx for idx in (text.find("["), text.find("{")) if idx >= 0]
    if starts:
        text = text[min(starts) :]
    return json.loads(text)


def debug_decomp_summary(debug_decomp: Any) -> dict[str, Any] | None:
    if not isinstance(debug_decomp, dict):
        return None
    quality = debug_decomp.get("quality_evidence") if isinstance(debug_decomp.get("quality_evidence"), dict) else {}
    pipeline = (
        debug_decomp.get("rust_sleigh_pipeline")
        if isinstance(debug_decomp.get("rust_sleigh_pipeline"), dict)
        else {}
    )
    pcode_blocks = pipeline.get("raw_pcode_blocks") if isinstance(pipeline.get("raw_pcode_blocks"), list) else []
    sampled_pcode_blocks = pcode_blocks[:64]
    return {
        "stage_status": debug_decomp.get("stage_status"),
        "stage_metrics": debug_decomp.get("stage_metrics"),
        "owner_buckets": debug_decomp.get("owner_buckets") or [],
        "rust_sleigh_pipeline": {
            key: pipeline.get(key)
            for key in [
                "entry_address",
                "max_bytes",
                "instruction_limit",
                "decode_attempt_count",
                "decode_stop_reason",
                "template_source_counts",
                "raw_pcode_block_count",
                "raw_pcode_op_count",
                "raw_pcode_edge_count",
                "raw_pcode_terminal_opcode_counts",
                "raw_pcode_block_evidence_truncated",
                "strict_indirect_retry_attempted",
                "nir_fallback_kind",
                "nir_fallback_kind_refined",
                "nir_fallback_reason_summary",
                "pipeline_stage_status",
            ]
            if key in pipeline
        } | (
            {
                "raw_pcode_blocks_sampled_count": len(sampled_pcode_blocks),
                "raw_pcode_blocks": sampled_pcode_blocks,
            }
            if sampled_pcode_blocks
            else {}
        ),
        "quality_evidence": {
            key: quality.get(key)
            for key in [
                "validated_pcode_op_count",
                "invalid_pcode_shape_count",
                "replacement_plan_rejected_missing_merge_count",
                "replacement_plan_rejected_alias_unsafe_count",
                "forced_linear_structuring_count",
                "structuring_irreducible_scc_count",
                "region_emit_ready_failed_count",
                "call_target_unresolved_sub_fallback_count",
                "call_prototype_signature_missing_count",
                "typed_fact_conflict_count",
            ]
            if key in quality
        },
    }


def run_fission_decomp(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool = False,
    debug_decomp_bundle_path: Path | None = None,
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        "decomp",
        str(binary_path),
        "--addr",
        address,
        "--json",
        "--no-header",
        "--no-warnings",
        "--timeout-ms",
        str(max(1000, timeout_sec * 1000)),
    ]
    if include_debug_decomp:
        cmd.append("--debug-decomp")
    if debug_decomp_bundle_path is not None:
        debug_decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        cmd.extend(["--debug-decomp-bundle", str(debug_decomp_bundle_path)])
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.TimeoutExpired:
        return {"success": False, "failure_kind": "timeout", "wall_sec": round(time.perf_counter() - start, 6)}
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or str(exc)).strip()
        return {
            "success": False,
            "failure_kind": "command_failed",
            "failure_detail": detail[-4000:],
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    try:
        payload = parse_json_loose(res.stdout)
    except json.JSONDecodeError as exc:
        return {
            "success": False,
            "failure_kind": "invalid_json",
            "failure_detail": str(exc),
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    if isinstance(payload, list):
        func = payload[0] if payload else {}
    else:
        func = (payload.get("functions") or [{}])[0] if isinstance(payload, dict) else {}
    if func.get("error"):
        return {
            "success": False,
            "failure_kind": "decompile_error",
            "failure_detail": func.get("error"),
            "wall_sec": round(time.perf_counter() - start, 6),
            "engine_used": func.get("engine_used"),
            "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
        }
    code = func.get("code") or ""
    if not code.strip():
        return {"success": False, "failure_kind": "empty_output", "wall_sec": round(time.perf_counter() - start, 6)}
    return {
        "success": True,
        "code": code,
        "wall_sec": round(time.perf_counter() - start, 6),
        "engine_used": func.get("engine_used"),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "preview_build_stats": func.get("preview_build_stats"),
        "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
        "debug_decomp_bundle_path": rel(debug_decomp_bundle_path)
        if debug_decomp_bundle_path is not None
        else None,
    }


def run_fission_decomp_cached(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    debug_decomp_bundle_path: Path | None,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> dict[str, Any]:
    key = decomp_cache_key(binary_path, address, fission_bin, include_debug_decomp)
    with cache_lock:
        cached = cache.get(key)
        if cached is not None:
            cache_stats["hit"] += 1
    if cached is not None:
        cached_result = dict(cached)
        cached_result["decomp_cache_status"] = "hit"
        if (
            include_debug_decomp
            and debug_decomp_bundle_path is not None
            and not debug_decomp_bundle_path.exists()
        ):
            cached_result = run_fission_decomp(
                binary_path,
                address,
                fission_bin,
                timeout_sec,
                include_debug_decomp=include_debug_decomp,
                debug_decomp_bundle_path=debug_decomp_bundle_path,
            )
            cached_result["decomp_cache_status"] = "refreshed_debug_bundle"
            with cache_lock:
                cache[key] = cached_result
                cache_stats["stored"] += 1
        elif debug_decomp_bundle_path is not None:
            cached_result["debug_decomp_bundle_path"] = rel(debug_decomp_bundle_path)
        return cached_result
    with cache_lock:
        cache_stats["miss"] += 1
    decomp = run_fission_decomp(
        binary_path,
        address,
        fission_bin,
        timeout_sec,
        include_debug_decomp=include_debug_decomp,
        debug_decomp_bundle_path=debug_decomp_bundle_path,
    )
    decomp["decomp_cache_status"] = "miss"
    with cache_lock:
        cache.setdefault(key, decomp)
        cache_stats["stored"] += 1
    return dict(decomp)


def decomp_result_from_function_payload(
    func: dict[str, Any],
    wall_sec: float,
    debug_bundle: dict[str, Any] | None,
    debug_decomp_bundle_path: Path | None,
) -> dict[str, Any]:
    if func.get("error"):
        return {
            "success": False,
            "failure_kind": "decompile_error",
            "failure_detail": func.get("error"),
            "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
            "engine_used": func.get("engine_used"),
            "debug_decomp": debug_decomp_summary(debug_bundle or func.get("debug_decomp")),
        }
    code = func.get("code") or ""
    if not code.strip():
        return {
            "success": False,
            "failure_kind": "empty_output",
            "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
        }
    return {
        "success": True,
        "code": code,
        "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
        "engine_used": func.get("engine_used"),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "preview_build_stats": func.get("preview_build_stats"),
        "debug_decomp": debug_decomp_summary(debug_bundle or func.get("debug_decomp")),
        "debug_decomp_bundle_path": rel(debug_decomp_bundle_path)
        if debug_decomp_bundle_path is not None
        else None,
    }


def write_single_debug_bundle(path: Path, bundle: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        dump_json_pretty({"schema_version": 1, "functions": [bundle]}),
        encoding="utf-8",
    )


def run_fission_decomp_batch(
    binary_path: Path,
    address_paths: list[tuple[str, Path | None]],
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    output_dir: Path,
    entry_id: str,
) -> dict[str, dict[str, Any]]:
    if not address_paths:
        return {}
    batch_dir = output_dir / "batch_decomp"
    batch_dir.mkdir(parents=True, exist_ok=True)
    from benchmark.source_semantic_benchmark.utils import sanitize_id
    slug = sanitize_id(entry_id)
    address_file = batch_dir / f"{slug}-addresses.txt"
    address_file.write_text(
        "".join(f"{address}\n" for address, _path in address_paths),
        encoding="utf-8",
    )
    debug_bundle_path = batch_dir / f"{slug}-debug-decomp.json"
    cmd = [
        str(fission_bin),
        "decomp",
        str(binary_path),
        "--addresses-file",
        str(address_file),
        "--benchmark",
        "--no-header",
        "--no-warnings",
        "--timeout-ms",
        str(max(1000, timeout_sec * 1000)),
    ]
    if include_debug_decomp:
        cmd.append("--debug-decomp")
        cmd.extend(["--debug-decomp-bundle", str(debug_bundle_path)])
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=max(timeout_sec, timeout_sec * len(address_paths)),
            check=True,
        )
    except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return {}

    wall_sec = round(time.perf_counter() - start, 6)
    try:
        payload = parse_json_loose(res.stdout)
    except json.JSONDecodeError:
        return {}
    functions = payload.get("functions") if isinstance(payload, dict) else None
    if not isinstance(functions, list):
        return {}

    debug_by_address: dict[str, dict[str, Any]] = {}
    if include_debug_decomp and debug_bundle_path.exists():
        try:
            debug_payload = json.loads(debug_bundle_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            debug_payload = {}
        debug_functions = debug_payload.get("functions", []) if isinstance(debug_payload, dict) else []
        for bundle in debug_functions:
            if not isinstance(bundle, dict):
                continue
            function = bundle.get("function") if isinstance(bundle.get("function"), dict) else {}
            address = function.get("resolved_address") or function.get("requested_address")
            if isinstance(address, str):
                debug_by_address[canonical_address(address)] = bundle

    requested_paths = {
        canonical_address(address): path
        for address, path in address_paths
    }
    results: dict[str, dict[str, Any]] = {}
    for func in functions:
        if not isinstance(func, dict):
            continue
        address = func.get("address")
        if not isinstance(address, str):
            continue
        key = canonical_address(address)
        debug_bundle = debug_by_address.get(key)
        requested_path = requested_paths.get(key)
        if debug_bundle is not None and requested_path is not None:
            write_single_debug_bundle(requested_path, debug_bundle)
        results[key] = decomp_result_from_function_payload(
            func,
            wall_sec,
            debug_bundle,
            requested_path,
        )
    return results


def run_command_capture(cmd: list[Any], timeout_sec: int) -> dict[str, Any]:
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
        )
        return {
            "status": "ok" if res.returncode == 0 else "run_failed",
            "returncode": res.returncode,
            "stdout": res.stdout,
            "stderr": res.stderr,
            "run_sec": round(time.perf_counter() - start, 6),
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "status": "timeout",
            "returncode": -1,
            "stdout": getattr(exc, "stdout", "") or "",
            "stderr": getattr(exc, "stderr", "") or "",
            "run_sec": round(time.perf_counter() - start, 6),
        }
    except Exception as exc:
        return {
            "status": "exception",
            "returncode": -1,
            "stdout": "",
            "stderr": str(exc),
            "run_sec": round(time.perf_counter() - start, 6),
        }
