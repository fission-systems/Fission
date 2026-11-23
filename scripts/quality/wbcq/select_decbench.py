#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Select step-shaped functions from the DecBench binaries for the WBCQ P3
evaluation (RFC #1093), part (c): real code, not written for the method.

A function is step-shaped when its DWARF signature is exactly
`<integer> f(<struct> *ctx, <integer> in)` -- a context it can keep state in, one
input, an observable integer result -- the boundary a behavioural quotient is
defined over. The selection reads only the signature: nothing about the body
(calls, globals, loops) is filtered, so the measured status distribution is
the method's on that population, not on a hand-picked part of it.

x86-64 ELF binaries of the `unoptimized`, `optimized` and `inlined` configs are
read (the evaluation runs one ISA; ADR 0009 parity is the corpus's job).
Writes a JSON list of candidates; --limit samples evenly over (project, opt)
with a fixed seed.

  python3 scripts/quality/wbcq/select_decbench.py ~/fission-benchmark/decbench-data \\
      --out ~/fission-benchmark/wbcq-p3/decbench_candidates.json
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

from elftools.elf.elffile import ELFFile

CONFIGS = ("unoptimized", "optimized", "inlined")
INTEGER_ENCODINGS = {0x02, 0x05, 0x06, 0x07, 0x08, 0x10}  # boolean, signed, signed_char, unsigned, unsigned_char, UTF


def strip(die):
    """Follow typedefs and qualifiers to the type they name."""
    while die is not None and die.tag in (
        "DW_TAG_typedef", "DW_TAG_const_type", "DW_TAG_volatile_type", "DW_TAG_restrict_type",
        "DW_TAG_atomic_type",
    ):
        if "DW_AT_type" not in die.attributes:
            return None
        die = die.get_DIE_from_attribute("DW_AT_type")
    return die


def type_of(die):
    if "DW_AT_type" not in die.attributes:
        return None
    return strip(die.get_DIE_from_attribute("DW_AT_type"))


def integer_bytes(die) -> int | None:
    if die is None:
        return None
    if die.tag == "DW_TAG_enumeration_type":
        size = die.attributes.get("DW_AT_byte_size")
        return size.value if size else 4
    if die.tag == "DW_TAG_base_type":
        encoding = die.attributes.get("DW_AT_encoding")
        size = die.attributes.get("DW_AT_byte_size")
        if encoding and encoding.value in INTEGER_ENCODINGS and size and size.value <= 8:
            return size.value
    return None


def struct_bytes(pointer) -> int | None:
    if pointer is None or pointer.tag != "DW_TAG_pointer_type":
        return None
    target = type_of(pointer)
    if target is None or target.tag not in ("DW_TAG_structure_type", "DW_TAG_union_type"):
        return None
    size = target.attributes.get("DW_AT_byte_size")
    return size.value if size and 0 < size.value <= 4096 else None


def candidates(binary: Path) -> list[dict]:
    found = []
    with binary.open("rb") as handle:
        elf = ELFFile(handle)
        if elf["e_machine"] != "EM_X86_64" or not elf.has_dwarf_info():
            return found
        dwarf = elf.get_dwarf_info()
        for unit in dwarf.iter_CUs():
            for die in unit.iter_DIEs():
                if die.tag != "DW_TAG_subprogram" or "DW_AT_low_pc" not in die.attributes:
                    continue
                name = die.attributes.get("DW_AT_name")
                if name is None:
                    continue
                params = [child for child in die.iter_children()
                          if child.tag == "DW_TAG_formal_parameter"]
                if len(params) != 2:
                    continue
                result = integer_bytes(type_of(die))
                context = struct_bytes(type_of(params[0]))
                value = integer_bytes(type_of(params[1]))
                if result is None or context is None or value is None:
                    continue
                found.append({
                    "function": name.value.decode(errors="replace"),
                    "address": hex(die.attributes["DW_AT_low_pc"].value),
                    "context_bytes": context,
                    "input_bits": value * 8,
                    "output_bits": result * 8,
                })
    return found


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("data", type=Path, help="DecBench dataset root")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=300)
    parser.add_argument("--seed", type=int, default=1093)
    args = parser.parse_args()
    data = args.data.expanduser()
    population = []
    seen = set()
    for config in CONFIGS:
        manifest = json.loads((data / "configs" / config / "manifest.json").read_text())
        for entry in manifest["binaries"]:
            path = data / entry["binary_path"]
            if not path.exists() or path in seen:
                continue
            seen.add(path)
            scored = set(entry.get("functions", []))
            try:
                rows = candidates(path)
            except Exception as error:  # a binary pyelftools cannot read
                print(f"skip {path}: {error}")
                continue
            for row in rows:
                if scored and row["function"] not in scored:
                    continue
                population.append({"config": config, "project": entry["project"],
                                   "opt": entry["opt"], "binary": str(path), **row})
    # One copy of a function per (project, opt): the same source compiled into
    # several programs of a project is one data point, not several.
    unique = {}
    for row in population:
        unique.setdefault((row["project"], row["opt"], row["function"]), row)
    population = list(unique.values())
    rng = random.Random(args.seed)
    groups: dict[tuple[str, str], list[dict]] = {}
    for row in population:
        groups.setdefault((row["project"], row["opt"]), []).append(row)
    for rows in groups.values():
        rng.shuffle(rows)
    sample = []
    while len(sample) < args.limit and any(groups.values()):
        for key in sorted(groups):
            if groups[key] and len(sample) < args.limit:
                sample.append(groups[key].pop())
    args.out.write_text(json.dumps({"population": len(population), "sample": sample}, indent=2) + "\n")
    print(f"{len(population)} step-shaped functions; sampled {len(sample)}")


if __name__ == "__main__":
    main()
