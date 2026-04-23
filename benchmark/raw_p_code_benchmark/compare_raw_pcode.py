#!/usr/bin/env python3
"""Compare Ghidra raw SLEIGH p-code with Fission raw p-code."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any


def normalize_opcode(opcode: str | None) -> str:
    if not opcode:
        return ""
    return "".join(ch for ch in opcode.upper() if ch.isalnum())


def normalize_varnode(raw: dict[str, Any] | None, unique_map: dict[tuple[Any, Any], int]) -> dict[str, Any] | None:
    if raw is None:
        return None
    is_constant = bool(raw.get("is_constant"))
    size = int(raw.get("size", 0))
    if is_constant:
        value = raw.get("constant_val", raw.get("offset", 0))
        return {"space": "const", "value": int(value), "size": size}

    space = raw.get("space")
    if space is None:
        space_id = int(raw.get("space_id", raw.get("spaceId", -1)))
        space = f"space_{space_id}"
    offset = int(raw.get("offset", 0))
    normalized_space = str(space).lower()
    if normalized_space in {"unique", "space_1"}:
        key = (normalized_space, offset)
        if key not in unique_map:
            unique_map[key] = len(unique_map)
        return {"space": "unique", "index": unique_map[key], "size": size}
    return {"space": normalized_space, "offset": offset, "size": size}


def normalize_ops(instruction: dict[str, Any]) -> list[dict[str, Any]]:
    unique_map: dict[tuple[Any, Any], int] = {}
    ops = []
    for op in instruction.get("pcode", []):
        ops.append(
            {
                "opcode": normalize_opcode(op.get("opcode")),
                "output": normalize_varnode(op.get("output"), unique_map),
                "inputs": [normalize_varnode(v, unique_map) for v in op.get("inputs", [])],
            }
        )
    return ops


def bucket_instruction(ghidra: dict[str, Any] | None, fission: dict[str, Any] | None) -> tuple[list[str], dict[str, Any]]:
    buckets: list[str] = []
    detail: dict[str, Any] = {}

    if ghidra is None:
        buckets.append("missing_ghidra_instruction")
        return buckets, detail
    if fission is None:
        buckets.append("missing_fission_instruction")
        return buckets, detail

    address = ghidra.get("address", fission.get("address"))
    detail["address"] = address
    detail["ghidra_status"] = ghidra.get("status")
    detail["fission_status"] = fission.get("status")
    detail["compat_emitter_used"] = bool(fission.get("compat_emitter_used"))

    if ghidra.get("status") != "ok":
        buckets.append("ghidra_decode_error")
        detail["ghidra_error"] = ghidra.get("error")
    if fission.get("status") != "ok":
        err = str(fission.get("error") or "")
        buckets.append("decode_no_match" if "DecodeNoMatch" in err else "fission_decode_error")
        detail["fission_error"] = err
        return buckets, detail

    if int(ghidra.get("length", -1)) != int(fission.get("length", -2)):
        buckets.append("length_mismatch")
        detail["ghidra_length"] = ghidra.get("length")
        detail["fission_length"] = fission.get("length")

    ghidra_mnemonic = str(ghidra.get("mnemonic", "")).lower()
    decoded = fission.get("decoded") or {}
    fission_mnemonic = str(decoded.get("mnemonic", "")).lower()
    if ghidra_mnemonic and fission_mnemonic and ghidra_mnemonic != fission_mnemonic:
        buckets.append("mnemonic_mismatch")
        detail["ghidra_mnemonic"] = ghidra_mnemonic
        detail["fission_mnemonic"] = fission_mnemonic

    gops = normalize_ops(ghidra)
    fops = normalize_ops(fission)
    detail["ghidra_opcodes"] = [op["opcode"] for op in gops]
    detail["fission_opcodes"] = [op["opcode"] for op in fops]

    if len(gops) != len(fops):
        buckets.append("pcode_op_count_mismatch")
        detail["ghidra_pcode_count"] = len(gops)
        detail["fission_pcode_count"] = len(fops)

    for idx, (gop, fop) in enumerate(zip(gops, fops)):
        if gop["opcode"] != fop["opcode"]:
            buckets.append("pcode_opcode_mismatch")
            detail.setdefault("first_opcode_mismatch", {"index": idx, "ghidra": gop["opcode"], "fission": fop["opcode"]})
            break
        if len(gop["inputs"]) != len(fop["inputs"]) or bool(gop["output"]) != bool(fop["output"]):
            buckets.append("pcode_arity_mismatch")
            detail.setdefault("first_arity_mismatch", {"index": idx, "ghidra": gop, "fission": fop})
            break
        if [v.get("size") if v else None for v in gop["inputs"]] != [
            v.get("size") if v else None for v in fop["inputs"]
        ]:
            buckets.append("varnode_size_mismatch")
            detail.setdefault("first_size_mismatch", {"index": idx, "ghidra": gop, "fission": fop})
            break
        g_out_size = gop["output"].get("size") if gop["output"] else None
        f_out_size = fop["output"].get("size") if fop["output"] else None
        if g_out_size != f_out_size:
            buckets.append("varnode_size_mismatch")
            detail.setdefault("first_size_mismatch", {"index": idx, "ghidra": gop, "fission": fop})
            break
        if [v.get("space") if v else None for v in gop["inputs"]] != [
            v.get("space") if v else None for v in fop["inputs"]
        ]:
            buckets.append("varnode_space_mismatch")
            detail.setdefault("first_space_mismatch", {"index": idx, "ghidra": gop, "fission": fop})
            break

    if not buckets:
        buckets.append("full_match")
    return buckets, detail


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ghidra", required=True, type=Path)
    parser.add_argument("--fission", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    ghidra = json.loads(args.ghidra.read_text())
    fission = json.loads(args.fission.read_text())
    g_instrs = ghidra.get("instructions", [])
    f_instrs = fission.get("instructions", [])

    totals: Counter[str] = Counter()
    rows = []
    for idx in range(max(len(g_instrs), len(f_instrs))):
        fission_instruction = f_instrs[idx] if idx < len(f_instrs) else None
        buckets, detail = bucket_instruction(
            g_instrs[idx] if idx < len(g_instrs) else None,
            fission_instruction,
        )
        for bucket in set(buckets):
            totals[bucket] += 1
        if fission_instruction and fission_instruction.get("compat_emitter_used"):
            totals["compat_emitter_used"] += 1
        detail["index"] = idx
        detail["buckets"] = buckets
        rows.append(detail)

    report = {
        "binary": ghidra.get("binary") or fission.get("binary"),
        "start_address": ghidra.get("start_address") or fission.get("start_address"),
        "ghidra_language_id": ghidra.get("language_id"),
        "ghidra_compiler_spec_id": ghidra.get("compiler_spec_id"),
        "fission_language_id": fission.get("language_id"),
        "fission_entry_id": fission.get("entry_id"),
        "fission_execution_mode": fission.get("execution_mode"),
        "total_instructions": len(rows),
        "bucket_totals": dict(sorted(totals.items())),
        "rows": rows,
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
