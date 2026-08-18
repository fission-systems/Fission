#!/usr/bin/env python3
"""
pack_ordinals.py
────────────────
Pack the DLL ordinal tables into one `.fpk`.

`x86_ordinals.json` and `arm_ordinals.json` are 21.9M of JSON that
`OrdinalDatabase` parsed in full at startup -- 78ms per process -- to build a
map that a binary consults for the handful of DLLs it imports from. One record
per DLL, keyed by its name, lets a lookup decode one block instead.

    <dll>|<ordinal>:<name>,<ordinal>:<name>,...

A record per (dll, ordinal) was measured first and came out larger than the
JSON in the bundle -- 4.34M against 3.99M -- because it repeats the DLL name
429,126 times. Per DLL it is 3.92M.

The two inputs overlap on four DLLs, and the JSON loader merges them with the
first file winning per ordinal. That has to be reproduced here: emitting a
record per file instead left `commctrl.dll` with two records, and a lookup
found whichever came first, losing ordinals the merged table has.

Usage:
    python3 scripts/pack_ordinals.py [--output utils/signatures/ordinals/ordinals.fpk]
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ORDINALS = os.path.join(ROOT, "utils", "signatures", "ordinals")
# Order matters: the first file to define an ordinal wins, as in `OrdinalDatabase::load`.
SOURCES = ["x86_ordinals.json", "arm_ordinals.json"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default=os.path.join(ORDINALS, "ordinals.fpk"))
    args = parser.parse_args()

    merged: dict[str, dict[str, str]] = {}
    for name in SOURCES:
        path = os.path.join(ORDINALS, name)
        if not os.path.exists(path):
            print(f"[!] missing {path}", file=sys.stderr)
            continue
        with open(path) as fh:
            for dll, table in json.load(fh).items():
                target = merged.setdefault(dll, {})
                for ordinal, export in table.items():
                    target.setdefault(ordinal, export)

    records = []
    for dll, table in merged.items():
        body = ",".join(
            f"{ordinal}:{export}"
            for ordinal, export in sorted(table.items(), key=lambda kv: int(kv[0]))
        )
        records.append(f"{dll}|{body}")
    print(f"[+] {len(records)} DLLs", file=sys.stderr)

    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as fh:
        fh.write("\n".join(records))
        staging = fh.name
    try:
        subprocess.run(
            [
                sys.executable,
                os.path.join(ROOT, "scripts", "fpk_pack.py"),
                staging,
                "--kind",
                "pipe-text",
                "--output",
                args.output,
            ],
            check=True,
        )
    finally:
        os.unlink(staging)


if __name__ == "__main__":
    main()
