from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any

from .metrics import (
    classify_failure_kind,
    collect_code_metrics,
    detect_embedded_failure,
    extract_quality_metrics,
    extract_fallback_kind,
    normalize_address,
)
from .resource_monitor import (
    HAS_PSUTIL,
    run_popen_with_resource_monitor,
    start_self_resource_monitor,
)


def _canonical_address_int(value: str | int) -> int:
    if isinstance(value, int):
        return value
    text = str(value).strip()
    if not text:
        return 0
    return int(text, 16)


def _canonical_address_str(value: str | int) -> str:
    return f"0x{_canonical_address_int(value):x}"


def _build_ghidra_function_identity_maps(functions: list[Any], image_base_offset: int) -> dict[str, Any]:
    exact_by_offset: dict[int, Any] = {}
    normalized_by_offset: dict[int, list[Any]] = {}
    unique_name_matches: dict[str, Any] = {}
    name_collisions: set[str] = set()

    for func in functions:
        try:
            offset = int(func.getEntryPoint().getOffset())
        except Exception:
            continue
        exact_by_offset[offset] = func
        normalized = offset - image_base_offset if offset >= image_base_offset else offset
        normalized_by_offset.setdefault(normalized, []).append(func)
        try:
            name = str(func.getName())
        except Exception:
            name = ""
        if not name:
            continue
        lowered = name.casefold()
        if lowered in name_collisions:
            continue
        if lowered in unique_name_matches:
            unique_name_matches.pop(lowered, None)
            name_collisions.add(lowered)
            continue
        unique_name_matches[lowered] = func

    return {
        "exact_by_offset": exact_by_offset,
        "normalized_by_offset": normalized_by_offset,
        "unique_name_matches": unique_name_matches,
        "image_base_offset": image_base_offset,
    }


def _resolve_ghidra_seed_target(
    requested_address: str,
    requested_name: str,
    addr_factory: Any,
    function_manager: Any,
    identity_maps: dict[str, Any],
) -> tuple[Any | None, str, dict[str, Any]]:
    requested_offset = _canonical_address_int(requested_address)
    image_base_offset = int(identity_maps.get("image_base_offset", 0) or 0)
    evidence = "Unstable"
    details: dict[str, Any] = {
        "requested_address": _canonical_address_str(requested_offset),
        "requested_normalized_entry": _canonical_address_str(requested_offset),
        "image_base": _canonical_address_str(image_base_offset),
    }

    def _lookup_exact(offset: int) -> Any | None:
        addr = addr_factory.getAddress(_canonical_address_str(offset))
        if not addr:
            return None
        target = function_manager.getFunctionContaining(addr)
        if target is not None:
            return target
        return function_manager.getFunctionAt(addr)

    # Exact raw identity first.
    target = _lookup_exact(requested_offset)
    if target is not None:
        details["matched_entry"] = _canonical_address_str(int(target.getEntryPoint().getOffset()))
        details["matched_normalized_entry"] = _canonical_address_str(requested_offset)
        return target, "ExactNormalizedAddress", details

    # PIE / rebased binaries: try image-base-adjusted entry identity.
    if image_base_offset > 0 and requested_offset < image_base_offset:
        adjusted_offset = image_base_offset + requested_offset
        target = _lookup_exact(adjusted_offset)
        if target is not None:
            details["matched_entry"] = _canonical_address_str(adjusted_offset)
            details["matched_normalized_entry"] = _canonical_address_str(requested_offset)
            return target, "ExactNormalizedAddress", details

    normalized_matches = identity_maps.get("normalized_by_offset", {}).get(requested_offset, [])
    if len(normalized_matches) == 1:
        target = normalized_matches[0]
        details["matched_entry"] = _canonical_address_str(int(target.getEntryPoint().getOffset()))
        details["matched_normalized_entry"] = _canonical_address_str(requested_offset)
        return target, "ExactNormalizedAddress", details
    if len(normalized_matches) > 1:
        details["candidate_count"] = len(normalized_matches)
        return None, "Ambiguous", details

    lowered_name = str(requested_name or "").casefold().strip()
    if lowered_name:
        target = identity_maps.get("unique_name_matches", {}).get(lowered_name)
        if target is not None:
            try:
                matched_offset = int(target.getEntryPoint().getOffset())
            except Exception:
                matched_offset = 0
            normalized = (
                matched_offset - image_base_offset
                if matched_offset >= image_base_offset
                else matched_offset
            )
            details["matched_entry"] = _canonical_address_str(matched_offset)
            details["matched_normalized_entry"] = _canonical_address_str(normalized)
            return target, "StructuralUniqueMatch", details

    return None, evidence, details


def run_command_json(
    cmd: list[str],
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout: int = 90,
) -> tuple[dict[str, Any] | None, str | None, float, dict[str, Any] | None]:
    start = time.perf_counter()
    try:
        if HAS_PSUTIL:
            popen = subprocess.Popen(
                cmd,
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env,
            )
            res, resources = run_popen_with_resource_monitor(popen, timeout_sec=timeout)
            if res.returncode != 0:
                raise subprocess.CalledProcessError(
                    res.returncode,
                    res.args,
                    output=res.stdout,
                    stderr=res.stderr,
                )
        else:
            res = subprocess.run(
                cmd,
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=timeout,
                check=True,
                env=env,
            )
            resources = None
    except subprocess.TimeoutExpired:
        return None, "timeout", time.perf_counter() - start, None
    except subprocess.CalledProcessError as exc:
        error = exc.stderr.strip() or exc.stdout.strip() or "command_failed"
        return None, error, time.perf_counter() - start, None

    try:
        return json.loads(res.stdout), None, time.perf_counter() - start, resources
    except json.JSONDecodeError:
        return None, "invalid_json", time.perf_counter() - start, resources


def list_functions_with_fission(
    root_dir: Path,
    binary_path: Path,
    fission_bin: Path,
    timeout_sec: int | None = None,
) -> list[tuple[str, str]]:
    cmd = [str(fission_bin), str(binary_path), "--list"]
    res = subprocess.run(
        cmd,
        cwd=root_dir,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
        timeout=timeout_sec,
    )
    functions: list[tuple[str, str]] = []
    for line in res.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[0].startswith("0x"):
            functions.append((parts[0], parts[-1]))
    return functions


def sample_functions(
    binary_name: str,
    functions: list[tuple[str, str]],
    limit: int,
    mandatory_sample_addresses: dict[str, list[str]],
) -> list[tuple[str, str]]:
    if limit <= 0 or len(functions) <= limit:
        return functions
    selected: list[tuple[str, str]] = []
    seen: set[str] = set()
    mandatory = {normalize_address(addr) for addr in mandatory_sample_addresses.get(binary_name, [])}

    for address, name in functions:
        normalized = normalize_address(address)
        if normalized in mandatory and normalized not in seen:
            selected.append((address, name))
            seen.add(normalized)

    for address, name in functions:
        normalized = normalize_address(address)
        if normalized in seen:
            continue
        selected.append((address, name))
        seen.add(normalized)
        if len(selected) >= limit:
            break

    return selected[:limit]


def run_fission_function(
    root_dir: Path,
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
    engine: str = "auto",
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        str(binary_path),
        "--decomp",
        address,
        "--engine",
        engine,
        "--json",
        "--benchmark",
        "--ghidra-compat",
        "--no-header",
        "--no-warnings",
    ]
    payload, error, wall_sec, resources = run_command_json(cmd, cwd=root_dir, timeout=timeout_sec)
    if payload is None:
        return {
            "success": False,
            "failure_kind": classify_failure_kind(error),
            "failure_detail": error,
            "wall_sec": round(wall_sec, 6),
            "resources": resources,
        }

    func = payload.get("functions", [{}])[0]
    code = func.get("code", "")
    if error_text := func.get("error"):
        return {
            "success": False,
            "address": func.get("address", address),
            "name": func.get("name", ""),
            "failure_kind": classify_failure_kind(error_text),
            "failure_detail": error_text,
            "wall_sec": round(wall_sec, 6),
            "engine_used": func.get("engine_used", engine),
            "fell_back": bool(func.get("fell_back", False)),
            "fallback_reason": func.get("fallback_reason"),
            "fallback_kind": extract_fallback_kind(func.get("fallback_reason")),
            "resources": resources,
        }
    entry = {
        "success": True,
        "address": func.get("address", address),
        "name": func.get("name", ""),
        "decomp_sec": round(float(func.get("decomp_sec", 0.0)), 6),
        "postprocess_sec": round(float(func.get("postprocess_sec", 0.0)), 6),
        "wall_sec": round(wall_sec, 6),
        "code": code,
        "engine_used": func.get("engine_used", engine),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "fallback_kind": extract_fallback_kind(func.get("fallback_reason")),
        "preview_build_stats": func.get("preview_build_stats"),
        "preview_hint_stats": func.get("preview_hint_stats"),
        "resources": resources,
    }
    if failure := detect_embedded_failure(code):
        entry["success"] = False
        entry["failure_kind"] = failure[0]
        entry["failure_detail"] = failure[1]
        if code.lstrip().startswith("// Assembly fallback:"):
            entry["fallback_counts"] = {"assembly_fallback": 1}
        return entry
    entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
    entry["quality_metrics"] = extract_quality_metrics(entry["metrics"])
    return entry


def run_ghidra_binary_with_meta(
    binary_path: Path,
    functions: list[tuple[str, str]],
    ghidra_dir: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)
    import pyghidra

    pyghidra.start()
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    results: dict[str, dict[str, Any]] = {}
    load_start = time.perf_counter()
    res_thread = None
    res_holder: dict[str, Any] = {}
    res_stop = None
    if HAS_PSUTIL:
        res_thread, res_holder, res_stop = start_self_resource_monitor(interval_sec=0.5)
    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        monitor = ConsoleTaskMonitor()
        decomp = DecompInterface()
        decomp.openProgram(program)
        init_sec = time.perf_counter() - load_start

        function_manager = program.getFunctionManager()
        addr_factory = program.getAddressFactory()
        image_base_offset = int(program.getImageBase().getOffset())
        identity_maps = _build_ghidra_function_identity_maps(
            list(function_manager.getFunctions(True)),
            image_base_offset,
        )

        for addr_str, name in functions:
            start = time.perf_counter()
            clean_addr = normalize_address(addr_str)
            entry: dict[str, Any] = {
                "address": addr_str,
                "name": name,
                "success": False,
            }
            try:
                target, match_evidence, identity_details = _resolve_ghidra_seed_target(
                    clean_addr,
                    name,
                    addr_factory,
                    function_manager,
                    identity_maps,
                )
                entry["match_evidence"] = match_evidence
                entry["canonical_identity"] = identity_details
                if target is None:
                    entry["failure_kind"] = "missing_function"
                    if match_evidence in {"Ambiguous", "Unstable"}:
                        entry["failure_detail"] = match_evidence.casefold()
                else:
                    entry["resolved_address"] = _canonical_address_str(
                        int(target.getEntryPoint().getOffset())
                    )
                    result = decomp.decompileFunction(target, timeout_sec, monitor)
                    if result and result.decompileCompleted() and result.getDecompiledFunction():
                        code = result.getDecompiledFunction().getC()
                        entry["success"] = True
                        entry["code"] = code
                        entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
                        entry["quality_metrics"] = extract_quality_metrics(entry["metrics"])
                    else:
                        entry["failure_kind"] = "other"
                        entry["failure_detail"] = "decompile_incomplete"
            except Exception as exc:  # noqa: BLE001
                entry["failure_kind"] = classify_failure_kind(str(exc))
                entry["error"] = str(exc)
            entry["decomp_sec"] = round(time.perf_counter() - start, 6)
            results[normalize_address(addr_str)] = entry
        try:
            decomp.dispose()
        except Exception:
            pass

    wall_sec = time.perf_counter() - load_start
    if HAS_PSUTIL and res_stop is not None and res_thread is not None:
        res_stop.set()
        res_thread.join(timeout=3.0)

    meta: dict[str, Any] = {
        "backend": "pyghidra",
        "init_sec": round(init_sec, 6),
        "wall_sec": round(wall_sec, 6),
        "image_base_address": _canonical_address_str(image_base_offset),
        "canonical_identity_mode": "image_base_normalized_entry",
        "resources": res_holder if res_holder else {},
    }
    return meta, results


def run_ghidra_binary(
    binary_path: Path,
    functions: list[tuple[str, str]],
    ghidra_dir: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
) -> tuple[float, dict[str, dict[str, Any]]]:
    meta, results = run_ghidra_binary_with_meta(
        binary_path,
        functions,
        ghidra_dir,
        timeout_sec,
        struct_ptr_aliases,
    )
    return float(meta.get("init_sec", 0.0)), results
