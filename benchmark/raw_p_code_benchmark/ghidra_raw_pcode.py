#!/usr/bin/env python3
"""Dump raw Ghidra instruction p-code with PyGhidra.

This intentionally uses Instruction.getPcode(), not decompiler HighFunction
p-code. The output is the oracle layer for SLEIGH runtime parity.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import pyghidra


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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--addr", required=True, type=parse_int)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--language", default=None)
    parser.add_argument("--compiler", default=None)
    parser.add_argument("--project-location", type=Path, default=None)
    parser.add_argument("--project-name", default=None)
    parser.add_argument("--no-analyze", action="store_true")
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

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
            "language_id": str(language.getLanguageID()),
            "compiler_spec_id": str(compiler.getCompilerSpecID()),
            "start_address": args.addr,
            "requested_count": args.count,
            "instructions": instructions,
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
