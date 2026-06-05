#!/usr/bin/env python3
"""Dump Ghidra Reference/Symbol-based CFG input facts for coverage comparison."""

from __future__ import annotations

import argparse
import json
import os
import time
from pathlib import Path
from typing import Any

import pyghidra


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_GHIDRA_DIRS = (
    ROOT / "vendor" / "ghidra" / "ghidra_12.0.4_PUBLIC",
    ROOT / "vendor" / "ghidra" / "ghidra-Ghidra_12.0.4_build",
    ROOT / "ghidra_12.0.4_PUBLIC",
    ROOT / "ghidra-Ghidra_12.0.4_build",
)


def parse_int(value: str) -> int:
    return int(value, 0)


def normalize_ghidra_install_dir(path: Path) -> Path:
    candidate = path.expanduser().resolve()
    if (candidate / "Ghidra" / "application.properties").exists():
        return candidate
    if candidate.name == "Ghidra" and (candidate / "application.properties").exists():
        return candidate.parent
    return candidate


def is_launchable_ghidra_install_dir(path: Path) -> bool:
    install_dir = normalize_ghidra_install_dir(path)
    return (
        (install_dir / "Ghidra" / "application.properties").exists()
        and (install_dir / "Ghidra" / "Features" / "PyGhidra" / "lib" / "PyGhidra.jar").exists()
        and (install_dir / "Ghidra" / "Framework" / "Utility" / "lib" / "Utility.jar").exists()
        and (install_dir / "support").exists()
    )


def resolve_ghidra_dir(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value)
    for env_name in ("FISSION_GHIDRA_DIR", "GHIDRA_INSTALL_DIR"):
        env_dir = os.environ.get(env_name)
        if env_dir:
            candidates.append(Path(env_dir))
    candidates.extend(DEFAULT_GHIDRA_DIRS)
    for candidate in candidates:
        normalized = normalize_ghidra_install_dir(candidate)
        if is_launchable_ghidra_install_dir(normalized):
            return normalized
    checked = ", ".join(str(normalize_ghidra_install_dir(path)) for path in candidates if path)
    raise FileNotFoundError(
        "Launchable Ghidra installation directory not found for CFG fact benchmark. "
        f"Checked: {checked if checked else '(none)'}"
    )


def _flow_type_name(ref: Any) -> str:
    return str(ref.getFlowType())


def _is_call_flow(ref: Any) -> bool:
    return "CALL" in _flow_type_name(ref)


def snapshot_function_facts(func: Any, flat: Any) -> dict[str, Any]:
    program = func.getProgram()
    listing = program.getListing()
    body = func.getBody()
    sym_table = program.getSymbolTable()
    ref_mgr = program.getReferenceManager()

    labels = {int(func.getEntryPoint().getOffset())}
    for sym in sym_table.getSymbols(body):
        labels.add(int(sym.getAddress().getOffset()))

    flow_edges: set[tuple[int, int]] = set()
    current = body.getMinAddress()
    while current is not None and body.contains(current):
        instr = listing.getInstructionAt(current)
        if instr is None:
            break
        from_addr = int(instr.getAddress().getOffset())
        for ref in ref_mgr.getReferencesFrom(instr.getAddress()):
            if not ref.isFlow() or _is_call_flow(ref):
                continue
            dest = ref.getToAddress()
            if dest is None or not body.contains(dest):
                continue
            flow_edges.add((from_addr, int(dest.getOffset())))
        for flow in instr.getFlows():
            dest = int(flow.getOffset())
            if body.contains(flow):
                flow_edges.add((from_addr, dest))
        current = instr.getFallThrough()

    edge_list = [{"from": src, "to": dst} for src, dst in sorted(flow_edges)]
    return {
        "function_address": int(func.getEntryPoint().getOffset()),
        "function_name": str(func.getName()),
        "labels": sorted(labels),
        "flow_edges": edge_list,
        "has_no_return": bool(func.hasNoReturn()),
    }


def snapshot_all_functions(program: Any, flat: Any) -> dict[str, Any]:
    from ghidra.program.model.listing import FunctionIterator

    functions: dict[str, Any] = {}
    func_manager = program.getFunctionManager()
    func_iter: FunctionIterator = func_manager.getFunctions(True)
    while func_iter.hasNext():
        func = func_iter.next()
        if func.isThunk():
            continue
        entry = int(func.getEntryPoint().getOffset())
        facts = snapshot_function_facts(func, flat)
        functions[hex(entry)] = {
            "function_address": entry,
            "function_name": str(func.getName()),
            "label_count": len(facts["labels"]),
            "flow_edge_count": len(facts["flow_edges"]),
            "facts": facts,
        }
    return functions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", type=parse_int, default=None)
    parser.add_argument("--all-functions", action="store_true")
    parser.add_argument("--language", default=None)
    parser.add_argument("--compiler", default=None)
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    if not args.all_functions and args.addr is None:
        parser.error("one of --addr or --all-functions is required")

    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    started_at = time.perf_counter()

    with pyghidra.open_program(
        args.binary,
        project_location=ghidra_dir / "support",
        analyze=True,
        language=args.language,
        compiler=args.compiler,
    ) as flat:
        program = flat.getCurrentProgram()
        if args.all_functions:
            functions = snapshot_all_functions(program, flat)
            report = {
                "tool": "ghidra",
                "binary": str(args.binary),
                "function_count": len(functions),
                "functions": functions,
            }
        else:
            func_manager = program.getFunctionManager()
            target = flat.toAddr(args.addr)
            func = func_manager.getFunctionContaining(target)
            if func is None:
                func = func_manager.getFunctionAt(target)
            if func is None:
                raise SystemExit(f"no function found at {hex(args.addr)}")
            facts = snapshot_function_facts(func, flat)
            report = {
                "tool": "ghidra",
                "binary": str(args.binary),
                "function_address": facts["function_address"],
                "function_name": facts["function_name"],
                "facts": facts,
            }

    report["timing"] = {"wall_clock_sec": time.perf_counter() - started_at}
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
