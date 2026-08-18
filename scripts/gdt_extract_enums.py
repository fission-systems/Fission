#!/usr/bin/env python3
"""
gdt_extract_enums.py
────────────────────
Extract enumeration constants from a Ghidra Data Type (.gdt) archive and emit
them as `{"NAME": value}` JSON.

Why this exists: `windows_vs12_*.gdt.types.json` already ships an `enum_values`
list, and its values are wrong. `WAIT_OBJECT_0` is recorded 2048 where it is 0,
`PAGE_EXECUTE_READWRITE` 16392 where it is 0x40, and `PAGE_NOACCESS` and
`FILE_SHARE_READ` are both 264 where both are 1 -- two distinct constants
cannot share a value, which is what shows the wrong field was read. Naming a
decompiled literal from that table would invent a wrong answer, so it is not
usable.

How this reads them instead (Ghidra 11.4.2 `EnumDBAdapterV1` /
`EnumValueDBAdapterV1`, and `DBRecord.write`, which lays fields out in schema
order with no record header):

    Enum        String Name | String Comment | Long CategoryID | Byte Size | Long x4
    Enum value  String Name | Long Value | Long EnumID | String Comment

Enum definitions are keyed by a Data Type ID of kind ENUM (8); enum values live
in their own table and point back through `EnumID`. Requiring that pointer to
be an ENUM-kind key is what separates real value records from every other
variable-length record in the archive.

Verified on windows_vs12_64.gdt two ways. Nine documented constants
(PAGE_EXECUTE_READWRITE, PAGE_NOACCESS, WAIT_TIMEOUT, WAIT_ABANDONED,
FILE_SHARE_READ, MEM_COMMIT, ERROR_ACCESS_DENIED, ERROR_SUCCESS, KEY_READ) all
match, and cross-checking every extracted name against `#define`s in the MinGW
headers agrees on 18,826 of 18,873 -- 99.75%. The shipped
`win_api_constants.json` manages 94.03% on the 469 names it shares with those
headers, and of the 29 places the two tables disagree the headers side with
this one 28 times and with the shipped table never.

Usage:
    python3 scripts/gdt_extract_enums.py <archive.gdt> --output <out.json>
    python3 scripts/gdt_extract_enums.py <archive.gdt> --self-test
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import sys
from typing import Any, Dict, List, Optional

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gdt_extract_signatures import (  # noqa: E402
    BufferFile, _i64, read_gdt_buffer_file, read_string, walk_all_leaves,
)
from gdt_extract_structs import KIND_ENUM, keyint, split_kind  # noqa: E402

# Values documented by the Windows SDK; a mis-read field will not match them.
KNOWN_VALUES: Dict[str, int] = {
    "PAGE_EXECUTE_READWRITE": 0x40,
    "PAGE_NOACCESS": 0x01,
    "WAIT_TIMEOUT": 0x102,
    "WAIT_ABANDONED": 0x80,
    "FILE_SHARE_READ": 0x01,
    "MEM_COMMIT": 0x1000,
    "ERROR_ACCESS_DENIED": 5,
    "ERROR_SUCCESS": 0,
    "KEY_READ": 0x20019,
}


def _byte(buf: bytes, off: int) -> int:
    value = buf[off]
    return value - 256 if value >= 128 else value


def parse_enum_def(rec: bytes) -> Optional[dict]:
    """`String Name | String Comment | Long CategoryID | Byte Size | ...`"""
    name, pos = read_string(rec, 0)
    if not name:
        return None
    _comment, pos = read_string(rec, pos)
    if pos + 9 > len(rec):
        return None
    size = _byte(rec, pos + 8)
    # An enum is 1, 2, 4 or 8 bytes wide; anything else means this record is
    # not an enum definition and was matched by coincidence.
    if size not in (1, 2, 4, 8):
        return None
    return {"name": name, "size": size, "category": _i64(rec, pos)}


def parse_enum_value(rec: bytes) -> Optional[dict]:
    """`String Name | Long Value | Long EnumID | String Comment`"""
    name, pos = read_string(rec, 0)
    if not name:
        return None
    if pos + 16 > len(rec):
        return None
    return {"name": name, "value": _i64(rec, pos), "enum_id": _i64(rec, pos + 8)}


def extract_enums(bf: BufferFile) -> Dict[str, List[dict]]:
    """Enum group name -> [{name, value}], for every enum in the archive."""
    definitions: Dict[int, dict] = {}
    values: List[dict] = []
    for _buf_idx, _node_type, key_bytes, rec in walk_all_leaves(bf):
        key = keyint(key_bytes)
        if split_kind(key)[0] == KIND_ENUM:
            parsed = parse_enum_def(rec)
            if parsed:
                definitions[key] = parsed
            continue
        parsed = parse_enum_value(rec)
        if parsed and split_kind(parsed["enum_id"])[0] == KIND_ENUM:
            values.append(parsed)

    groups: Dict[str, List[dict]] = collections.defaultdict(list)
    for value in values:
        definition = definitions.get(value["enum_id"])
        if definition is None:
            continue
        groups[definition["name"]].append(
            {"name": value["name"], "value": value["value"]}
        )
    return dict(groups)


def flatten(groups: Dict[str, List[dict]]) -> Dict[str, int]:
    """Constant name -> value, first definition wins on a duplicate name."""
    flat: Dict[str, int] = {}
    for members in groups.values():
        for member in members:
            flat.setdefault(member["name"], member["value"])
    return flat


def self_test(flat: Dict[str, int]) -> int:
    """Compare against `KNOWN_VALUES`; returns the number of failures."""
    failures = 0
    for name, expected in KNOWN_VALUES.items():
        got = flat.get(name)
        if got is None:
            print(f"  {name:26} ABSENT", file=sys.stderr)
            failures += 1
        elif (got & 0xFFFFFFFF) != (expected & 0xFFFFFFFF):
            print(f"  {name:26} {got:#x} != {expected:#x}", file=sys.stderr)
            failures += 1
        else:
            print(f"  {name:26} {got:#x} ok", file=sys.stderr)
    return failures


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gdt_file")
    parser.add_argument("--output", help="write flat {name: value} JSON here")
    parser.add_argument(
        "--groups-output", help="write {group: [{name, value}]} JSON here"
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="check documented constants and exit non-zero on any mismatch",
    )
    args = parser.parse_args()

    bf = BufferFile(read_gdt_buffer_file(args.gdt_file))
    groups = extract_enums(bf)
    flat = flatten(groups)
    multi = sum(1 for members in groups.values() if len(members) > 1)
    print(
        f"[+] {len(groups)} enums ({multi} with more than one member), "
        f"{len(flat)} distinct constants",
        file=sys.stderr,
    )

    failures = 0
    if args.self_test or args.output or args.groups_output:
        failures = self_test(flat)

    if failures:
        print(f"[!] {failures} documented constants did not match", file=sys.stderr)
        sys.exit(1)

    if args.output:
        with open(args.output, "w") as fh:
            json.dump(flat, fh, indent=1, sort_keys=True)
        print(f"[+] Wrote {len(flat)} constants -> {args.output}", file=sys.stderr)
    if args.groups_output:
        with open(args.groups_output, "w") as fh:
            json.dump(groups, fh, indent=1, sort_keys=True)
        print(f"[+] Wrote {len(groups)} groups -> {args.groups_output}", file=sys.stderr)


if __name__ == "__main__":
    main()
