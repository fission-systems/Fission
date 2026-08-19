#!/usr/bin/env python3
"""
pack_golang_typeinfo.py
───────────────────────
Pack the Go API snapshots into `.fpk`.

`go1.X.json` is 4-10M of nested JSON per Go release, parsed in full whenever a
Go binary is analysed -- 42ms for go1.20, 80ms for go1.25 -- to build a flat map
the decompiler then queries by symbol name.

Splitting by build tag does not help: the platform-independent `all` tag is 8.1M
of go1.25's 10.2M, and a windows/amd64 binary needs 8.44M of it. What does help
is per-symbol records, so a lookup decodes one block.

Records keep the build tag, because the merge is part of the query. `from_raw`
overlays tags in order -- all, goarch, goos, unix when the OS is one, then
goos-goarch -- with the first definition winning, and that has to happen at
lookup time now rather than once at load. The tag leads the key so blocks group
by tag, which is also what compresses.

    <tag>\\x1f<symbol>|<params>|<results>          funcs
    <tag>\\x1f<symbol>|<kind>|<target>|<fields>    types

Only the fields the runtime deserialises are kept: `JsonFuncSig` reads Params
and Results, `JsonTypeEntry` reads Kind, Target and Fields. Position, Flags and
TypeParams are in the JSON and are not read by anything.

Usage:
    python3 scripts/pack_golang_typeinfo.py [--dir utils/signatures/typeinfo/golang]
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TAG_SEP = "\x1f"


def esc(text: str | None) -> str:
    """`|` and `;` separate fields; `\\x1f` separates tag from symbol."""
    return (
        (text or "")
        .replace("\\", "\\\\")
        .replace("|", "\\p")
        .replace(";", "\\s")
        .replace(TAG_SEP, "\\u")
        .replace("\n", "\\n")
    )


def pack(records: list[str], output: str) -> None:
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
                output,
            ],
            check=True,
        )
    finally:
        os.unlink(staging)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dir", default=os.path.join(ROOT, "utils", "signatures", "typeinfo", "golang")
    )
    args = parser.parse_args()

    for path in sorted(glob.glob(os.path.join(args.dir, "go1.*.json"))):
        stem = os.path.basename(path)[: -len(".json")]
        with open(path) as fh:
            raw = json.load(fh)

        funcs: list[str] = []
        types: list[str] = []
        for tag, entry in raw.items():
            for name, sig in (entry.get("Funcs") or {}).items():
                params = ";".join(
                    f"{esc(p.get('Name'))}:{esc(p.get('DataType'))}"
                    for p in (sig.get("Params") or [])
                )
                results = ";".join(
                    f"{esc(r.get('Name'))}:{esc(r.get('DataType'))}"
                    for r in (sig.get("Results") or [])
                )
                funcs.append(f"{tag}{TAG_SEP}{esc(name)}|{params}|{results}")
            for name, entry_type in (entry.get("Types") or {}).items():
                fields = ";".join(
                    f"{esc(f.get('Name'))}:{esc(f.get('DataType'))}"
                    for f in (entry_type.get("Fields") or [])
                )
                types.append(
                    f"{tag}{TAG_SEP}{esc(name)}|{esc(entry_type.get('Kind'))}"
                    f"|{esc(entry_type.get('Target'))}|{fields}"
                )

        pack(funcs, os.path.join(args.dir, f"{stem}.fn.fpk"))
        pack(types, os.path.join(args.dir, f"{stem}.ty.fpk"))
        print(f"[+] {stem}: {len(funcs)} funcs, {len(types)} types", file=sys.stderr)


if __name__ == "__main__":
    main()
