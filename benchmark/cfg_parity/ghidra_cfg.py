#!/usr/bin/env python3
"""Dump Ghidra address-keyed CFG snapshots for parity against Fission."""

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
        "Launchable Ghidra installation directory not found for CFG parity benchmark. "
        f"Checked: {checked if checked else '(none)'}"
    )


def snapshot_from_high_pcode(func: Any, flat: Any, timeout_sec: int) -> dict[str, Any]:
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    decomp = DecompInterface()
    decomp.openProgram(func.getProgram())
    result = decomp.decompileFunction(func, timeout_sec, ConsoleTaskMonitor())
    if not result.decompileCompleted():
        raise RuntimeError(result.getErrorMessage() or "decompile failed")

    high_func = result.getHighFunction()
    blocks = list(high_func.getBasicBlocks())
    block_starts = sorted({int(bb.getStart().getOffset()) for bb in blocks})
    start_to_index = {start: idx for idx, start in enumerate(block_starts)}

    edges: list[dict[str, int]] = []
    exit_blocks: list[int] = []
    for bb in blocks:
        src = int(bb.getStart().getOffset())
        outs: set[int] = set()
        for i in range(bb.getOutSize()):
            outs.add(int(bb.getOut(i).getStart().getOffset()))
        if not outs:
            exit_blocks.append(src)
        for dst in sorted(outs):
            edges.append({"from": src, "to": dst})

    edges.sort(key=lambda edge: (edge["from"], edge["to"]))
    exit_blocks = sorted(set(exit_blocks))

    return {
        "model": "ghidra_high_pcode",
        "function_address": int(func.getEntryPoint().getOffset()),
        "block_starts": block_starts,
        "edges": edges,
        "exit_blocks": exit_blocks,
        "block_count": len(block_starts),
        "edge_count": len(edges),
        "high_block_count": len(blocks),
        "start_index_map": start_to_index,
    }


def _flow_type_name(ref: Any) -> str:
    return str(ref.getFlowType())


def _is_call_flow(ref: Any) -> bool:
    return "CALL" in _flow_type_name(ref)


def snapshot_from_basic_block_model(func: Any, flat: Any) -> dict[str, Any]:
    program = func.getProgram()
    listing = program.getListing()
    address_space = program.getAddressFactory().getDefaultAddressSpace()
    from ghidra.program.model.block import BasicBlockModel

    bbm = BasicBlockModel(program)
    blocks = list(bbm.getCodeBlocksContaining(func.getBody(), flat.monitor))
    block_starts = sorted(int(bb.getMinAddress().getOffset()) for bb in blocks)

    edges: list[dict[str, int]] = []
    exit_blocks: list[int] = []
    for bb in blocks:
        src = int(bb.getMinAddress().getOffset())
        it = bb.getDestinations(flat.monitor)
        out_degree = 0
        while it.hasNext():
            ref = it.next()
            if _is_call_flow(ref):
                continue
            dest_block = ref.getDestinationBlock()
            if dest_block is None:
                continue
            dst = int(dest_block.getMinAddress().getOffset())
            if listing.getFunctionContaining(address_space.getAddress(dst)) != func:
                continue
            edges.append({"from": src, "to": dst})
            out_degree += 1
        if out_degree == 0:
            exit_blocks.append(src)

    edges.sort(key=lambda edge: (edge["from"], edge["to"]))
    exit_blocks = sorted(set(exit_blocks))

    return {
        "model": "ghidra_basic_block_model",
        "function_address": int(func.getEntryPoint().getOffset()),
        "block_starts": block_starts,
        "edges": edges,
        "exit_blocks": exit_blocks,
        "block_count": len(block_starts),
        "edge_count": len(edges),
    }


def snapshot_from_instruction_flow(func: Any, flat: Any) -> dict[str, Any]:
    program = func.getProgram()
    listing = program.getListing()
    body = func.getBody()
    address_space = program.getAddressFactory().getDefaultAddressSpace()

    instructions = []
    current = body.getMinAddress()
    while current is not None and body.contains(current):
        instr = listing.getInstructionAt(current)
        if instr is None:
            break
        instructions.append(instr)
        current = instr.getFallThrough()

    leaders = {int(func.getEntryPoint().getOffset())}
    for instr in instructions:
        for flow in instr.getFlows():
            leaders.add(int(flow.getOffset()))

    leader_list = sorted(leaders)
    leader_set = set(leader_list)
    blocks: dict[int, list[Any]] = {}
    for idx, leader in enumerate(leader_list):
        start_addr = address_space.getAddress(leader)
        instr = listing.getInstructionAt(start_addr)
        if instr is None:
            continue
        block_instrs = [instr]
        current = instr.getFallThrough()
        next_leader = leader_list[idx + 1] if idx + 1 < len(leader_list) else None
        while current is not None and body.contains(current):
            offset = int(current.getOffset())
            if next_leader is not None and offset >= next_leader:
                break
            if offset in leader_set and offset != leader:
                break
            next_instr = listing.getInstructionAt(current)
            if next_instr is None:
                break
            block_instrs.append(next_instr)
            current = next_instr.getFallThrough()
        blocks[leader] = block_instrs

    edges: set[tuple[int, int]] = set()
    exit_blocks: list[int] = []
    for leader, block_instrs in blocks.items():
        last = block_instrs[-1]
        flows = [int(flow.getOffset()) for flow in last.getFlows()]
        if not flows:
            exit_blocks.append(leader)
        for flow in flows:
            if flow in blocks:
                edges.add((leader, flow))

    edge_list = [{"from": src, "to": dst} for src, dst in sorted(edges)]
    return {
        "model": "ghidra_instruction_flow",
        "function_address": int(func.getEntryPoint().getOffset()),
        "block_starts": sorted(blocks.keys()),
        "edges": edge_list,
        "exit_blocks": sorted(set(exit_blocks)),
        "block_count": len(blocks),
        "edge_count": len(edge_list),
    }


def snapshot_for_function(func: Any, flat: Any, model: str, timeout_sec: int) -> dict[str, Any]:
    if model == "ghidra_high_pcode":
        return snapshot_from_high_pcode(func, flat, timeout_sec)
    if model == "ghidra_basic_block_model":
        return snapshot_from_basic_block_model(func, flat)
    return snapshot_from_instruction_flow(func, flat)


def snapshot_all_functions(program: Any, flat: Any, model: str, timeout_sec: int) -> dict[str, Any]:
    from ghidra.program.model.listing import FunctionIterator

    functions: dict[str, Any] = {}
    func_manager = program.getFunctionManager()
    func_iter: FunctionIterator = func_manager.getFunctions(True)
    while func_iter.hasNext():
        func = func_iter.next()
        entry = int(func.getEntryPoint().getOffset())
        snapshot = snapshot_for_function(func, flat, model, timeout_sec)
        functions[hex(entry)] = {
            "function_address": entry,
            "function_name": str(func.getName()),
            "block_count": snapshot["block_count"],
            "edge_count": snapshot["edge_count"],
            "snapshot": {
                "model": snapshot["model"],
                "function_address": snapshot["function_address"],
                "block_starts": snapshot["block_starts"],
                "edges": snapshot["edges"],
                "exit_blocks": snapshot["exit_blocks"],
            },
        }
    return functions


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", type=parse_int, default=None)
    parser.add_argument(
        "--all-functions",
        action="store_true",
        help="Dump BBM snapshots for every function in the binary (single analyze session).",
    )
    parser.add_argument("--language", default=None)
    parser.add_argument("--compiler", default=None)
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument(
        "--model",
        choices=(
            "ghidra_high_pcode",
            "ghidra_basic_block_model",
            "ghidra_instruction_flow",
        ),
        default="ghidra_basic_block_model",
    )
    parser.add_argument("--decompile-timeout-sec", type=int, default=120)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    if not args.all_functions and args.addr is None:
        parser.error("one of --addr or --all-functions is required")

    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    started_at = time.perf_counter()
    pyghidra.start(install_dir=ghidra_dir)

    with pyghidra.open_program(
        args.binary,
        language=args.language,
        compiler=args.compiler,
        analyze=True,
    ) as flat:
        program = flat.getCurrentProgram()
        if args.all_functions:
            functions = snapshot_all_functions(
                program, flat, args.model, args.decompile_timeout_sec
            )
            report = {
                "tool": "ghidra",
                "binary": str(args.binary),
                "ghidra_dir": str(ghidra_dir),
                "model": args.model,
                "function_count": len(functions),
                "functions": functions,
            }
        else:
            address_space = program.getAddressFactory().getDefaultAddressSpace()
            listing = program.getListing()
            func = listing.getFunctionContaining(address_space.getAddress(args.addr))
            if func is None:
                raise SystemExit(f"no function found at 0x{args.addr:x}")

            snapshot = snapshot_for_function(func, flat, args.model, args.decompile_timeout_sec)
            report = {
                "tool": "ghidra",
                "binary": str(args.binary),
                "ghidra_dir": str(ghidra_dir),
                "function_name": str(func.getName()),
                "model": args.model,
                "snapshot": {
                    "model": snapshot["model"],
                    "function_address": snapshot["function_address"],
                    "block_starts": snapshot["block_starts"],
                    "edges": snapshot["edges"],
                    "exit_blocks": snapshot["exit_blocks"],
                },
                "block_count": snapshot["block_count"],
                "edge_count": snapshot["edge_count"],
            }

    elapsed_sec = time.perf_counter() - started_at
    timing: dict[str, Any] = {"wall_clock_sec": elapsed_sec}
    if args.all_functions:
        timing["function_count"] = report.get("function_count")
    else:
        timing["block_count"] = report.get("block_count")
        timing["edge_count"] = report.get("edge_count")
    report["timing"] = timing

    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
