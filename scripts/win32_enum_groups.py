#!/usr/bin/env python3
"""
win32_enum_groups.py
────────────────────
Build the Win32 enum-group table: which named group a constant belongs to, and
which API parameter carries that group.

Two halves have existed separately and never met.

`win_api_signatures.txt` names groups in parameter position -- `VirtualAlloc`'s
`flProtect:PAGE_PROTECTION_FLAGS`, `WaitForSingleObject`'s `WAIT_EVENT` return
-- but says nothing about which constants are in a group. Nothing under
`utils/` did: `win_api_constants.json` is a flat value->name table of error
codes, and the enum list in `windows_vs12_*.gdt.types.json` has wrong values.

Joining them by name prefix is not sound. `PAGE_PROTECTION_FLAGS` -> `PAGE_*`
happens to work, but the same rule turns `PRINTER_HANDLE`, `SC_HANDLE` and
`LSA_HANDLE` -- opaque handle types, not enums -- into groups of 137, 38 and 12
constants, and would name handle values after unrelated constants.

`vendor/win32metadata` (MIT, Microsoft) is the authority Microsoft generates
its own projections from, and it is already vendored. Its
`generation/WinSDK/enums.json` gives each group its members, whether the group
is a flag set, and which method parameters use it -- and, crucially, says
nothing about `PRINTER_HANDLE`, because that is not an enum.

Two member sources:
  * 745 groups list members and values outright
  * 309 declare `autoPopulate` -- a `|`-separated header prefix filter -- and
    are filled from the constant table `gdt_extract_enums.py` recovers, which
    agrees with the MinGW headers on 99.75% of names

`vendor/` is reference-only: no production path may read it at build or run
time. So this runs offline and writes JSON under `utils/`, exactly as the GDT
extractors do.

Usage:
    python3 scripts/win32_enum_groups.py \
        --constants <flat-constants.json> \
        --output utils/signatures/typeinfo/win32/enum_groups.json
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any, Dict

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
METADATA = os.path.join(
    REPO, "vendor", "win32metadata", "generation", "WinSDK", "enums.json"
)

# Groups whose membership is documented, used to catch a silently empty build.
KNOWN_MEMBERS: Dict[str, Dict[str, int]] = {
    "PAGE_PROTECTION_FLAGS": {"PAGE_NOACCESS": 0x01, "PAGE_EXECUTE_READWRITE": 0x40},
    "FILE_SHARE_MODE": {"FILE_SHARE_READ": 0x01, "FILE_SHARE_WRITE": 0x02},
    "WIN32_ERROR": {"ERROR_SUCCESS": 0, "ERROR_ACCESS_DENIED": 5},
}


def parse_value(raw: Any) -> int | None:
    if isinstance(raw, int):
        return raw
    if not isinstance(raw, str):
        return None
    text = raw.strip().rstrip("uUlL")
    try:
        return int(text, 16) if text.lower().startswith("0x") else int(text, 10)
    except ValueError:
        return None


def build(constants: Dict[str, int]) -> Dict[str, Any]:
    with open(METADATA) as fh:
        items = json.load(fh)["items"]

    groups: Dict[str, Dict[str, Any]] = {}
    # `addUsesTo` items carry no name of their own; they attach uses to another
    # group, so their uses are collected and merged after the named pass.
    pending_uses: list[tuple[str, list]] = []

    for item in items:
        name = item.get("name")
        if not name:
            target = item.get("addUsesTo")
            if target:
                pending_uses.append((target, item.get("uses") or []))
            continue

        members: Dict[str, int] = {}
        for member in item.get("members") or []:
            value = parse_value(member.get("value"))
            if value is not None and member.get("name"):
                members[member["name"]] = value

        auto = item.get("autoPopulate")
        if auto and auto.get("filter"):
            prefixes = tuple(p for p in auto["filter"].split("|") if p)
            for const_name, value in constants.items():
                if const_name.startswith(prefixes):
                    members.setdefault(const_name, value)

        if not members:
            continue

        groups[name] = {
            "flags": bool(item.get("flags")),
            "members": members,
            "uses": {},
        }
        pending_uses.append((name, item.get("uses") or []))

    for target, uses in pending_uses:
        group = groups.get(target)
        if group is None:
            continue
        for use in uses:
            method = use.get("method")
            parameter = use.get("parameter")
            if method and parameter:
                group["uses"].setdefault(method, {})[parameter] = target

    # Flatten uses into one method -> {parameter: group} index; callers look up
    # by call target, not by group.
    by_method: Dict[str, Dict[str, str]] = {}
    for name, group in groups.items():
        for method, params in group.pop("uses").items():
            by_method.setdefault(method, {}).update(params)

    return {"groups": groups, "uses": by_method}


def self_test(table: Dict[str, Any]) -> int:
    failures = 0
    groups = table["groups"]
    for group_name, members in KNOWN_MEMBERS.items():
        group = groups.get(group_name)
        if group is None:
            print(f"  {group_name:26} GROUP ABSENT", file=sys.stderr)
            failures += 1
            continue
        for member, expected in members.items():
            got = group["members"].get(member)
            if got is None:
                print(f"  {group_name}.{member:26} ABSENT", file=sys.stderr)
                failures += 1
            elif (got & 0xFFFFFFFF) != (expected & 0xFFFFFFFF):
                print(
                    f"  {group_name}.{member:26} {got:#x} != {expected:#x}",
                    file=sys.stderr,
                )
                failures += 1
            else:
                print(f"  {group_name}.{member:26} {got:#x} ok", file=sys.stderr)
    # A parameter mapping is the other half; without it nothing can be resolved.
    if table["uses"].get("VirtualAlloc", {}).get("flProtect") != "PAGE_PROTECTION_FLAGS":
        print("  VirtualAlloc.flProtect mapping ABSENT", file=sys.stderr)
        failures += 1
    else:
        print("  VirtualAlloc.flProtect -> PAGE_PROTECTION_FLAGS ok", file=sys.stderr)
    return failures


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--constants",
        help="flat {name: value} JSON from gdt_extract_enums.py; "
        "autoPopulate groups stay empty without it",
    )
    parser.add_argument("--output")
    args = parser.parse_args()

    constants: Dict[str, int] = {}
    if args.constants:
        with open(args.constants) as fh:
            constants = {k: int(v) for k, v in json.load(fh).items()}

    table = build(constants)
    members = sum(len(g["members"]) for g in table["groups"].values())
    flagged = sum(1 for g in table["groups"].values() if g["flags"])
    print(
        f"[+] {len(table['groups'])} groups ({flagged} flag sets), {members} members, "
        f"{len(table['uses'])} methods with parameter mappings",
        file=sys.stderr,
    )

    failures = self_test(table)
    if failures:
        print(f"[!] {failures} checks failed", file=sys.stderr)
        sys.exit(1)

    if args.output:
        with open(args.output, "w") as fh:
            json.dump(table, fh, indent=1, sort_keys=True)
        print(f"[+] Wrote {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
