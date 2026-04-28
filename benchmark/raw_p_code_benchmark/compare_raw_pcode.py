#!/usr/bin/env python3
"""Compare Ghidra raw SLEIGH p-code with Fission raw p-code."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any

# Opcodes where input[0] is a constant encoding an address space reference.
# The raw integer value differs between Ghidra (packed spaceID) and Fission
# (SLA index). We normalize by resolving to the semantic space name.
LOAD_STORE_OPCODES = {"LOAD", "STORE"}


def build_space_resolver(report: dict[str, Any]) -> dict[int, str]:
    """Build a resolver from opaque space-id integer → semantic space name.

    Both Ghidra and Fission identify address spaces with integers, but they
    use different encoding schemes:

    - **Ghidra**: packed spaceID = ``(sla_index << 7) | (logsize << 4) | type``
    - **Fission**: raw SLA index (the ``unique`` field in Ghidra parlance)

    The resolver maps BOTH encodings to the space name so that LOAD/STORE
    constant inputs compare equal regardless of which encoding is used.

    For Ghidra reports: scan non-constant varnodes for ``(space_id → name)``
    pairs. Also extract the SLA index via ``packed >> 7`` and register that.

    For Fission reports: use the explicit ``space_map`` (name → SLA index).
    """
    resolver: dict[int, str] = {}

    # Ghidra packed-spaceID shift constant (from AddressSpace.java)
    ID_UNIQUE_SHIFT = 7

    # Fission includes an explicit space_map {name: index}.
    space_map = report.get("space_map", {})
    for name, index in space_map.items():
        resolver[int(index)] = str(name).lower()

    # Scan all instruction varnodes to harvest (space_id → space name).
    for instruction in report.get("instructions", []):
        for op in instruction.get("pcode", []):
            for vn in [op.get("output")] + op.get("inputs", []):
                if vn is None:
                    continue
                if bool(vn.get("is_constant")):
                    continue
                space = vn.get("space")
                space_id = vn.get("space_id", vn.get("spaceId"))
                if space is not None and space_id is not None:
                    sid = int(space_id)
                    name = str(space).lower()
                    # Register the packed spaceID itself.
                    resolver[sid] = name
                    # Also register the SLA index extracted from the packed
                    # spaceID, so that Fission's raw SLA index values also
                    # resolve. (No-op when sid < 128, i.e. already an index.)
                    sla_index = sid >> ID_UNIQUE_SHIFT
                    if sla_index not in resolver:
                        resolver[sla_index] = name
    return resolver


def normalize_opcode(opcode: str | None) -> str:
    if not opcode:
        return ""
    return "".join(ch for ch in opcode.upper() if ch.isalnum())


def normalize_varnode(
    raw: dict[str, Any] | None,
    unique_map: dict[tuple[Any, Any], int],
    *,
    is_load_store_space: bool = False,
    space_resolver: dict[int, str] | None = None,
) -> dict[str, Any] | None:
    if raw is None:
        return None
    is_constant = bool(raw.get("is_constant"))
    size = int(raw.get("size", 0))
    if is_constant:
        value = raw.get("constant_val", raw.get("offset", 0))
        int_value = int(value)
        # For LOAD/STORE input[0], the constant encodes an address space.
        # Ghidra uses a packed spaceID; Fission uses the SLA index.
        # Normalize to the semantic space name so both sides compare equal.
        if is_load_store_space and space_resolver:
            space_name = space_resolver.get(int_value)
            if space_name is None:
                # Ghidra packed spaceID: extract the SLA index and retry.
                sla_index = int_value >> 7  # ID_UNIQUE_SHIFT
                space_name = space_resolver.get(sla_index)
            if space_name is not None:
                return {"space": "const", "value": space_name, "size": size}
        return {"space": "const", "value": int_value, "size": size}

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


def normalize_ops(
    instruction: dict[str, Any],
    space_resolver: dict[int, str] | None = None,
) -> list[dict[str, Any]]:
    unique_map: dict[tuple[Any, Any], int] = {}
    ops = []
    for op in instruction.get("pcode", []):
        opcode = normalize_opcode(op.get("opcode"))
        is_load_store = opcode in LOAD_STORE_OPCODES
        inputs = []
        for idx, v in enumerate(op.get("inputs", [])):
            inputs.append(
                normalize_varnode(
                    v,
                    unique_map,
                    is_load_store_space=(is_load_store and idx == 0),
                    space_resolver=space_resolver,
                )
            )
        ops.append(
            {
                "opcode": opcode,
                "output": normalize_varnode(op.get("output"), unique_map),
                "inputs": inputs,
            }
        )
    return ops


def op_delta_payload(
    *,
    kind: str,
    index: int,
    ghidra: dict[str, Any] | None,
    fission: dict[str, Any] | None,
    input_index: int | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "kind": kind,
        "index": index,
        "ghidra_op": ghidra,
        "fission_op": fission,
    }
    if input_index is not None:
        payload["input_index"] = input_index
    return payload


def owner_hints_for(buckets: list[str], detail: dict[str, Any]) -> list[str]:
    hints: set[str] = set()
    bucket_set = set(buckets)
    if detail.get("compat_emitter_used"):
        hints.add("compat_emitter")
    if bucket_set & {"pcode_opcode_mismatch", "pcode_op_count_mismatch", "pcode_arity_mismatch"}:
        hints.add("template_opcode_sequence")
    if bucket_set & {
        "input_varnode_mismatch",
        "output_varnode_mismatch",
        "varnode_space_mismatch",
        "varnode_size_mismatch",
        "label_target_mismatch",
    }:
        mismatch = detail.get("first_mismatch") or {}
        ghidra_op = mismatch.get("ghidra_op") or {}
        fission_op = mismatch.get("fission_op") or {}
        opcode = normalize_opcode(ghidra_op.get("opcode") or fission_op.get("opcode"))
        if opcode in {"LOAD", "STORE"}:
            hints.add("dynamic_pointer_identity")
        elif opcode in {"COPY", "PIECE", "SUBPIECE"}:
            hints.add("handle_selector_resolution")
        else:
            hints.add("varnode_identity")
    if "temp_space_mismatch" in bucket_set:
        hints.add("temp_allocation")
    if bucket_set & {
        "length_mismatch",
        "missing_ghidra_instruction",
        "missing_fission_instruction",
        "decode_no_match",
        "ghidra_decode_error",
        "fission_decode_error",
    }:
        hints.add("decode_length")
    if bucket_set & {"unsupported_template", "invalid_pcode_shape"}:
        hints.add("unsupported_template")
    if "mnemonic_mismatch" in bucket_set and not (
        bucket_set & {"pcode_opcode_mismatch", "pcode_op_count_mismatch", "input_varnode_mismatch", "output_varnode_mismatch"}
    ):
        hints.add("display_only_mnemonic")
    if "both_decode_error_or_padding" in bucket_set:
        hints.add("padding_or_no_instruction")
    if detail.get("template_source") in {"NativeFission", "CompatibilityLowered"}:
        hints.add("compat_emitter")
    return sorted(hints)


BRANCH_LIKE_OPCODES = {
    "BRANCH",
    "CBRANCH",
    "CALL",
    "BRANCHIND",
    "CALLIND",
    "RETURN",
}


def safe_ratio(numerator: float, denominator: float) -> float:
    if denominator <= 0:
        return 1.0 if numerator <= 0 else 0.0
    return max(0.0, min(1.0, numerator / denominator))


def sequence_similarity(left: list[Any], right: list[Any]) -> float:
    import difflib

    if not left and not right:
        return 1.0
    return difflib.SequenceMatcher(None, left, right).ratio()


def scalar_similarity(left: Any, right: Any) -> float:
    return 1.0 if left == right else 0.0


def varnode_similarity(ghidra: dict[str, Any] | None, fission: dict[str, Any] | None) -> float:
    if ghidra is None and fission is None:
        return 1.0
    if ghidra is None or fission is None:
        return 0.0

    score = 0.0
    score += 0.40 * scalar_similarity(ghidra.get("space"), fission.get("space"))
    score += 0.20 * scalar_similarity(ghidra.get("size"), fission.get("size"))

    g_value = ghidra.get("value", ghidra.get("offset", ghidra.get("index")))
    f_value = fission.get("value", fission.get("offset", fission.get("index")))
    score += 0.40 * scalar_similarity(g_value, f_value)
    return score


def op_similarity(ghidra: dict[str, Any], fission: dict[str, Any]) -> float:
    opcode_score = scalar_similarity(ghidra.get("opcode"), fission.get("opcode"))
    output_score = varnode_similarity(ghidra.get("output"), fission.get("output"))

    g_inputs = ghidra.get("inputs", [])
    f_inputs = fission.get("inputs", [])
    aligned = max(len(g_inputs), len(f_inputs))
    if aligned == 0:
        input_score = 1.0
    else:
        input_score = sum(
            varnode_similarity(
                g_inputs[idx] if idx < len(g_inputs) else None,
                f_inputs[idx] if idx < len(f_inputs) else None,
            )
            for idx in range(aligned)
        ) / aligned

    return (0.40 * opcode_score) + (0.20 * output_score) + (0.40 * input_score)


def pcode_structural_similarity(
    ghidra_ops: list[dict[str, Any]],
    fission_ops: list[dict[str, Any]],
) -> float:
    aligned = max(len(ghidra_ops), len(fission_ops))
    if aligned == 0:
        return 1.0
    return sum(
        op_similarity(ghidra_ops[idx], fission_ops[idx])
        if idx < len(ghidra_ops) and idx < len(fission_ops)
        else 0.0
        for idx in range(aligned)
    ) / aligned


def instruction_similarity_components(
    ghidra: dict[str, Any] | None,
    fission: dict[str, Any] | None,
    ghidra_ops: list[dict[str, Any]],
    fission_ops: list[dict[str, Any]],
) -> dict[str, float]:
    if ghidra is None or fission is None:
        return {
            "opcode_sequence_similarity": 0.0,
            "pcode_structural_similarity": 0.0,
            "length_similarity": 0.0,
            "mnemonic_similarity": 0.0,
            "weighted_similarity_score": 0.0,
        }

    g_opcodes = [op["opcode"] for op in ghidra_ops]
    f_opcodes = [op["opcode"] for op in fission_ops]
    opcode_score = sequence_similarity(g_opcodes, f_opcodes)
    pcode_score = pcode_structural_similarity(ghidra_ops, fission_ops)

    g_len = int(ghidra.get("length", -1))
    f_len = int(fission.get("length", -2))
    length_score = scalar_similarity(g_len, f_len)

    g_mnemonic = str(ghidra.get("mnemonic", "")).lower()
    decoded = fission.get("decoded") or {}
    f_mnemonic = str(decoded.get("mnemonic", "")).lower()
    mnemonic_score = 1.0 if not g_mnemonic or not f_mnemonic else scalar_similarity(g_mnemonic, f_mnemonic)

    weighted = (
        0.65 * pcode_score
        + 0.15 * opcode_score
        + 0.10 * length_score
        + 0.10 * mnemonic_score
    )
    return {
        "opcode_sequence_similarity": opcode_score,
        "pcode_structural_similarity": pcode_score,
        "length_similarity": length_score,
        "mnemonic_similarity": mnemonic_score,
        "weighted_similarity_score": weighted,
    }


def performance_from_report(report: dict[str, Any]) -> dict[str, Any]:
    timing = report.get("timing") or {}
    wall_clock_sec = timing.get("wall_clock_sec")
    instruction_count = int(timing.get("instruction_count", 0))
    pcode_op_count = int(timing.get("pcode_op_count", 0))
    return {
        "wall_clock_sec": wall_clock_sec,
        "instruction_count": instruction_count,
        "pcode_op_count": pcode_op_count,
        "instructions_per_sec": timing.get("instructions_per_sec"),
        "pcode_ops_per_sec": timing.get("pcode_ops_per_sec"),
    }


def performance_delta(ghidra: dict[str, Any], fission: dict[str, Any]) -> dict[str, Any]:
    ghidra_wall = ghidra.get("wall_clock_sec")
    fission_wall = fission.get("wall_clock_sec")
    wall_clock_delta_sec = None
    wall_clock_speedup_fission_over_ghidra = None
    if ghidra_wall is not None and fission_wall is not None:
        wall_clock_delta_sec = fission_wall - ghidra_wall
        if fission_wall > 0:
            wall_clock_speedup_fission_over_ghidra = ghidra_wall / fission_wall

    ghidra_ips = ghidra.get("instructions_per_sec")
    fission_ips = fission.get("instructions_per_sec")
    instruction_throughput_ratio = None
    if ghidra_ips not in (None, 0) and fission_ips is not None:
        instruction_throughput_ratio = fission_ips / ghidra_ips

    ghidra_ops = ghidra.get("pcode_ops_per_sec")
    fission_ops = fission.get("pcode_ops_per_sec")
    pcode_throughput_ratio = None
    if ghidra_ops not in (None, 0) and fission_ops is not None:
        pcode_throughput_ratio = fission_ops / ghidra_ops

    return {
        "wall_clock_delta_sec": wall_clock_delta_sec,
        "wall_clock_speedup_fission_over_ghidra": wall_clock_speedup_fission_over_ghidra,
        "instruction_throughput_ratio_fission_over_ghidra": instruction_throughput_ratio,
        "pcode_throughput_ratio_fission_over_ghidra": pcode_throughput_ratio,
    }


def classify_varnode_delta(
    *,
    opcode: str,
    side: str,
    index: int | None,
    ghidra: dict[str, Any] | None,
    fission: dict[str, Any] | None,
) -> tuple[str, str]:
    g_space = ghidra.get("space") if ghidra else None
    f_space = fission.get("space") if fission else None
    if g_space != f_space:
        if "unique" in {g_space, f_space}:
            return "temp_space_mismatch", "space"
        return "varnode_space_mismatch", "space"

    g_size = ghidra.get("size") if ghidra else None
    f_size = fission.get("size") if fission else None
    if g_size != f_size:
        return "varnode_size_mismatch", "size"

    if opcode in BRANCH_LIKE_OPCODES and side == "input" and index == 0:
        g_value = ghidra.get("value", ghidra.get("offset")) if ghidra else None
        f_value = fission.get("value", fission.get("offset")) if fission else None
        if g_value != f_value:
            return "label_target_mismatch", "value"

    if (ghidra or {}).get("space") == "unique":
        if ghidra != fission:
            return "temp_space_mismatch", "value"

    if side == "output":
        return "output_varnode_mismatch", "value"
    return "input_varnode_mismatch", "value"


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
    template_source = fission.get("template_source")
    detail["template_source"] = (
        "sla_construct_tpl" if template_source == "SpecDerived" else template_source
    )
    detail["raw_template_source"] = template_source

    ghidra_error = str(ghidra.get("error") or "")
    if (
        ghidra.get("status") != "ok"
        and "no instruction" in ghidra_error.lower()
        and fission.get("status") == "ok"
        and not fission.get("pcode", [])
    ):
        buckets.extend(["ghidra_decode_error", "both_decode_error_or_padding"])
        detail["ghidra_error"] = ghidra.get("error")
        detail["ghidra_opcodes"] = []
        detail["fission_opcodes"] = []
        return buckets, detail

    if ghidra.get("status") != "ok":
        buckets.append("ghidra_decode_error")
        detail["ghidra_error"] = ghidra.get("error")
    if fission.get("status") != "ok":
        err = str(fission.get("error") or "")
        if "DecodeNoMatch" in err:
            buckets.append("decode_no_match")
        elif "UnsupportedPcodeTemplate" in err:
            buckets.append("unsupported_template")
            if ghidra.get("status") != "ok" and "spec_derived_construct_tpl_has_no_ops" in err:
                buckets.append("both_decode_error_or_padding")
        elif "InvalidPcodeShape" in err:
            buckets.append("invalid_pcode_shape")
        else:
            buckets.append("fission_decode_error")
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

    gops = normalize_ops(ghidra, space_resolver=ghidra.get("_space_resolver"))
    fops = normalize_ops(fission, space_resolver=fission.get("_space_resolver"))
    detail["ghidra_opcodes"] = [op["opcode"] for op in gops]
    detail["fission_opcodes"] = [op["opcode"] for op in fops]

    if len(gops) != len(fops):
        buckets.append("pcode_op_count_mismatch")
        detail["ghidra_pcode_count"] = len(gops)
        detail["fission_pcode_count"] = len(fops)
        detail.setdefault(
            "first_mismatch",
            op_delta_payload(
                kind="op_count",
                index=min(len(gops), len(fops)),
                ghidra=gops[min(len(gops), len(fops))] if len(gops) > len(fops) else None,
                fission=fops[min(len(gops), len(fops))] if len(fops) > len(gops) else None,
            ),
        )

    for idx, (gop, fop) in enumerate(zip(gops, fops)):
        if gop["opcode"] != fop["opcode"]:
            buckets.append("pcode_opcode_mismatch")
            detail.setdefault("first_opcode_mismatch", {"index": idx, "ghidra": gop["opcode"], "fission": fop["opcode"]})
            detail.setdefault(
                "first_mismatch",
                op_delta_payload(kind="opcode", index=idx, ghidra=gop, fission=fop),
            )
            break
        if len(gop["inputs"]) != len(fop["inputs"]) or bool(gop["output"]) != bool(fop["output"]):
            buckets.append("pcode_arity_mismatch")
            detail.setdefault("first_arity_mismatch", {"index": idx, "ghidra": gop, "fission": fop})
            detail.setdefault(
                "first_mismatch",
                op_delta_payload(kind="arity", index=idx, ghidra=gop, fission=fop),
            )
            break
        for input_index, (g_in, f_in) in enumerate(zip(gop["inputs"], fop["inputs"])):
            if g_in != f_in:
                bucket, mismatch_kind = classify_varnode_delta(
                    opcode=gop["opcode"],
                    side="input",
                    index=input_index,
                    ghidra=g_in,
                    fission=f_in,
                )
                buckets.append(bucket)
                detail.setdefault(
                    f"first_{mismatch_kind}_mismatch",
                    {"index": idx, "operand_index": input_index, "ghidra": gop, "fission": fop},
                )
                detail.setdefault(
                    "first_mismatch",
                    op_delta_payload(
                        kind="input_varnode",
                        index=idx,
                        input_index=input_index,
                        ghidra=gop,
                        fission=fop,
                    ),
                )
                break
        else:
            if gop["output"] != fop["output"]:
                bucket, mismatch_kind = classify_varnode_delta(
                    opcode=gop["opcode"],
                    side="output",
                    index=None,
                    ghidra=gop["output"],
                    fission=fop["output"],
                )
                buckets.append(bucket)
                detail.setdefault(
                    f"first_{mismatch_kind}_mismatch",
                    {"index": idx, "ghidra": gop, "fission": fop},
                )
                detail.setdefault(
                    "first_mismatch",
                    op_delta_payload(kind="output_varnode", index=idx, ghidra=gop, fission=fop),
                )
                break

    if not buckets:
        buckets.append("full_match")

    if ghidra is None or fission is None:
        components = instruction_similarity_components(ghidra, fission, [], [])
    else:
        components = instruction_similarity_components(ghidra, fission, gops, fops)
    detail["similarity_components"] = components
    detail["similarity_score"] = components["weighted_similarity_score"]

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

    # Build a merged space resolver from both reports. Both describe the same
    # binary and thus the same address spaces, so merging is safe. Fission's
    # space_map gives SLA indices and Ghidra gives packed spaceIDs; the merged
    # resolver covers both encoding schemes.
    merged_resolver = build_space_resolver(ghidra)
    merged_resolver.update(build_space_resolver(fission))

    totals: Counter[str] = Counter()
    owner_hint_totals: Counter[str] = Counter()
    rows = []
    total_similarity = 0.0
    similarity_component_totals: Counter[str] = Counter()
    for idx in range(max(len(g_instrs), len(f_instrs))):
        fission_instruction = f_instrs[idx] if idx < len(f_instrs) else None
        # Inject the merged resolver into both instructions.
        g_instr = g_instrs[idx] if idx < len(g_instrs) else None
        if g_instr is not None:
            g_instr = {**g_instr, "_space_resolver": merged_resolver}
        f_instr = fission_instruction
        if f_instr is not None:
            f_instr = {**f_instr, "_space_resolver": merged_resolver}
        buckets, detail = bucket_instruction(g_instr, f_instr)
        total_similarity += detail.get("similarity_score", 0.0)
        for name, value in detail.get("similarity_components", {}).items():
            similarity_component_totals[name] += float(value)
        for bucket in set(buckets):
            totals[bucket] += 1
        if fission_instruction and fission_instruction.get("compat_emitter_used"):
            totals["compat_emitter_used"] += 1
        hints = owner_hints_for(buckets, detail)
        for hint in hints:
            owner_hint_totals[hint] += 1
        detail["index"] = idx
        detail["buckets"] = buckets
        detail["owner_hints"] = hints
        rows.append(detail)
        if "both_decode_error_or_padding" in buckets and "ghidra_decode_error" in buckets:
            break

    average_similarity = total_similarity / len(rows) if rows else 0.0
    average_similarity_components = {
        name: value / len(rows) if rows else 0.0
        for name, value in sorted(similarity_component_totals.items())
    }

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
        "owner_hint_totals": dict(sorted(owner_hint_totals.items())),
        "similarity_summary": {
            "average_similarity_score": average_similarity,
            "average_components": average_similarity_components,
            "parity_ratio": totals.get("full_match", 0) / len(rows) if rows else 0.0,
        },
        "performance": {
            "ghidra": performance_from_report(ghidra),
            "fission": performance_from_report(fission),
            "delta": performance_delta(
                performance_from_report(ghidra),
                performance_from_report(fission),
            ),
        },
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
