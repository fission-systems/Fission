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
    normalize_address,
)


def run_command_json(
    cmd: list[str],
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout: int = 90,
) -> tuple[dict[str, Any] | None, str | None, float]:
    start = time.perf_counter()
    try:
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
    except subprocess.TimeoutExpired:
        return None, "timeout", time.perf_counter() - start
    except subprocess.CalledProcessError as exc:
        error = exc.stderr.strip() or exc.stdout.strip() or "command_failed"
        return None, error, time.perf_counter() - start

    try:
        return json.loads(res.stdout), None, time.perf_counter() - start
    except json.JSONDecodeError:
        return None, "invalid_json", time.perf_counter() - start


def list_functions_with_fission(root_dir: Path, binary_path: Path, fission_bin: Path) -> list[tuple[str, str]]:
    cmd = [str(fission_bin), str(binary_path), "--list"]
    res = subprocess.run(
        cmd,
        cwd=root_dir,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
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
    payload, error, wall_sec = run_command_json(cmd, cwd=root_dir, timeout=timeout_sec)
    if payload is None:
        return {
            "success": False,
            "failure_kind": classify_failure_kind(error),
            "failure_detail": error,
            "wall_sec": round(wall_sec, 6),
        }

    func = payload.get("functions", [{}])[0]
    code = func.get("code", "")
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
    }
    if failure := detect_embedded_failure(code):
        entry["success"] = False
        entry["failure_kind"] = failure[0]
        entry["failure_detail"] = failure[1]
        if code.lstrip().startswith("// Assembly fallback:"):
            entry["fallback_counts"] = {"assembly_fallback": 1}
        return entry
    entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
    return entry


def run_ghidra_binary(
    binary_path: Path,
    functions: list[tuple[str, str]],
    ghidra_dir: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
) -> tuple[float, dict[str, dict[str, Any]]]:
    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)
    import pyghidra

    pyghidra.start()
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    results: dict[str, dict[str, Any]] = {}
    load_start = time.perf_counter()
    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        monitor = ConsoleTaskMonitor()
        decomp = DecompInterface()
        decomp.openProgram(program)
        init_sec = time.perf_counter() - load_start

        function_manager = program.getFunctionManager()
        addr_factory = program.getAddressFactory()

        for addr_str, name in functions:
            start = time.perf_counter()
            clean_addr = normalize_address(addr_str)
            entry: dict[str, Any] = {
                "address": addr_str,
                "name": name,
                "success": False,
            }
            try:
                addr = addr_factory.getAddress(clean_addr)
                target = None
                if addr:
                    target = function_manager.getFunctionContaining(addr)
                    if not target:
                        target = function_manager.getFunctionAt(addr)
                if target is None:
                    for func in list(function_manager.getFunctions(True)):
                        if func.getName() == name or func.getName() == f"_{name}":
                            target = func
                            break
                if target is None:
                    entry["failure_kind"] = "missing_function"
                else:
                    result = decomp.decompileFunction(target, timeout_sec, monitor)
                    if result and result.decompileCompleted() and result.getDecompiledFunction():
                        code = result.getDecompiledFunction().getC()
                        entry["success"] = True
                        entry["code"] = code
                        entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
                    else:
                        entry["failure_kind"] = "other"
                        entry["failure_detail"] = "decompile_incomplete"
            except Exception as exc:  # noqa: BLE001
                entry["failure_kind"] = classify_failure_kind(str(exc))
                entry["error"] = str(exc)
            entry["decomp_sec"] = round(time.perf_counter() - start, 6)
            results[normalize_address(addr_str)] = entry
    return init_sec, results
