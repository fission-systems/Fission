#!/usr/bin/env python3
"""Dump raw Ghidra instruction p-code with PyGhidra.

This intentionally uses Instruction.getPcode(), not decompiler HighFunction
p-code. The output is the oracle layer for SLEIGH runtime parity.
"""

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


def java_bytes_to_list(raw: Any) -> list[int]:
    return [int(byte) & 0xFF for byte in raw]


def varnode_to_json(varnode: Any | None) -> dict[str, Any] | None:
    if varnode is None:
        return None
    address = varnode.getAddress()
    space = address.getAddressSpace()
    return {
        "space": space.getName(),
        "space_id": int(space.getSpaceID()),
        "offset": int(address.getOffset()),
        "size": int(varnode.getSize()),
        "is_constant": bool(varnode.isConstant()),
        "repr": str(varnode),
    }


def pcode_op_to_json(op: Any) -> dict[str, Any]:
    return {
        "seq_num": int(op.getSeqnum().getOrder()),
        "opcode": str(op.getMnemonic()),
        "opcode_id": int(op.getOpcode()),
        "address": int(op.getSeqnum().getTarget().getOffset()),
        "output": varnode_to_json(op.getOutput()),
        "inputs": [varnode_to_json(op.getInput(i)) for i in range(op.getNumInputs())],
        "repr": str(op),
    }


def instruction_to_json(instr: Any) -> dict[str, Any]:
    flows = instr.getFlows()
    return {
        "address": int(instr.getAddress().getOffset()),
        "status": "ok",
        "error": None,
        "bytes": java_bytes_to_list(instr.getBytes()),
        "length": int(instr.getLength()),
        "mnemonic": str(instr.getMnemonicString()),
        "flow_type": str(instr.getFlowType()),
        "flows": [int(flow.getOffset()) for flow in flows],
        "pcode": [pcode_op_to_json(op) for op in instr.getPcode()],
    }


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
        "Launchable Ghidra installation directory not found for raw p-code benchmark. "
        "Pass --ghidra-dir or set GHIDRA_INSTALL_DIR to a packaged Ghidra install root. "
        f"Checked: {checked if checked else '(none)'}"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", required=True, type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--language", default=None)
    parser.add_argument("--compiler", default=None)
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--project-location", type=Path, default=None)
    parser.add_argument("--project-name", default=None)
    parser.add_argument("--no-analyze", action="store_true")
    parser.add_argument(
        "--disassemble-missing",
        action="store_true",
        help="If no instruction exists at the requested address, ask Ghidra to disassemble there before reading raw p-code.",
    )
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    started_at = time.perf_counter()
    pyghidra.start(install_dir=ghidra_dir)

    with pyghidra.open_program(
        args.binary,
        project_location=args.project_location,
        project_name=args.project_name,
        analyze=not args.no_analyze,
        language=args.language,
        compiler=args.compiler,
    ) as flat:
        program = flat.getCurrentProgram()
        address_space = program.getAddressFactory().getDefaultAddressSpace()
        listing = program.getListing()
        current = address_space.getAddress(args.addr)

        instructions: list[dict[str, Any]] = []
        for _ in range(args.count):
            instr = listing.getInstructionAt(current)
            if instr is None and args.disassemble_missing:
                flat.disassemble(current)
                instr = listing.getInstructionAt(current)
            if instr is None:
                instructions.append(
                    {
                        "address": int(current.getOffset()),
                        "status": "error",
                        "error": f"no instruction at 0x{int(current.getOffset()):x}",
                        "pcode": [],
                    }
                )
                break
            instructions.append(instruction_to_json(instr))
            current = instr.getAddress().add(instr.getLength())

        language = program.getLanguage()
        compiler = program.getCompilerSpec()
        report = {
            "tool": "ghidra",
            "binary": str(args.binary),
            "ghidra_dir": str(ghidra_dir),
            "language_id": str(language.getLanguageID()),
            "compiler_spec_id": str(compiler.getCompilerSpecID()),
            "start_address": args.addr,
            "requested_count": args.count,
            "instructions": instructions,
        }

    elapsed_sec = time.perf_counter() - started_at
    instruction_count = sum(1 for instruction in instructions if instruction.get("status") == "ok")
    pcode_op_count = sum(len(instruction.get("pcode", [])) for instruction in instructions)
    report["timing"] = {
        "wall_clock_sec": elapsed_sec,
        "instruction_count": instruction_count,
        "pcode_op_count": pcode_op_count,
        "instructions_per_sec": instruction_count / elapsed_sec if elapsed_sec > 0 else None,
        "pcode_ops_per_sec": pcode_op_count / elapsed_sec if elapsed_sec > 0 else None,
    }

    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
