#!/usr/bin/env python3
"""
pack_ghidra_exports.py
──────────────────────
Pack Ghidra's `.exports` symbol files into `ghidra_exports.fpk`.

`utils/ghidra-data/.../symbols/{win32,win64}/*.exports` is 27.4M of XML listing
25 DLLs' export tables -- ordinal, name, and a stack-purge byte count. Nothing
in Fission opened them: `ghidra_no_return.rs` walks the same directories but
reads only `.hints`, of which there are four totalling 0.03M.

They are worth having. `OrdinalDatabase` resolves ordinal imports from RetDec's
tables, and these carry 30,929 (dll, ordinal) pairs those tables do not,
including six DLLs missing entirely -- mfc140, mfc140u, and the 16-bit
kernel.exe, krnl386.exe, gdi.exe, user.exe.

They are NOT merged over the existing tables. The two sources disagree on 91,555
pairs, almost all in MFC, where ordinals are reassigned between builds:

    mfc80u.dll #6323   ghidra: ?wndTopMost@CWnd@@2V1@B
                       retdec: ??1CDaoException@@UAE@XZ

Nothing here says which build each source described, so this is a second table
consulted only when the first has no answer -- a name the primary table lacks is
new information, a name it already has is a disagreement this cannot settle.

    <dll>|<ordinal>:<name>,<ordinal>:<name>,...

Usage:
    python3 scripts/pack_ghidra_exports.py
"""
from __future__ import annotations

import argparse
import glob
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The XML is a packer input, so it lives under `utils/source/` and is not
# shipped. Subdirectories are kept because win16, win32 and win64 each carry a
# `mfc140.exports`, and flattening them silently overwrote 11 of the 37 files.
SYMBOLS = os.path.join(ROOT, "utils", "source", "ghidra-exports")
LIBRARY_RE = re.compile(r'<LIBRARY\s+NAME="([^"]+)"')
EXPORT_RE = re.compile(r'ORDINAL="(-?\d+)"\s+NAME="([^"]*)"')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        default=os.path.join(ROOT, "utils", "ghidra-data", "ghidra_exports.fpk"),
    )
    args = parser.parse_args()

    tables: dict[str, dict[int, str]] = {}
    files = sorted(glob.glob(os.path.join(SYMBOLS, "*", "*.exports")))
    for path in files:
        text = open(path, errors="ignore").read()
        library = LIBRARY_RE.search(text)
        if not library:
            print(f"[!] no LIBRARY element in {path}", file=sys.stderr)
            continue
        table = tables.setdefault(library.group(1).lower(), {})
        for match in EXPORT_RE.finditer(text):
            ordinal = int(match.group(1))
            name = match.group(2)
            # Ordinal -1 means "exported by name only"; there is nothing to key on.
            if ordinal < 0 or not name:
                continue
            # `|` and `,` are the record separators, and `:` splits a pair.
            if any(c in name for c in "|,:"):
                continue
            table.setdefault(ordinal, name)

    records = []
    for dll, table in tables.items():
        body = ",".join(f"{o}:{n}" for o, n in sorted(table.items()))
        records.append(f"{dll}|{body}")
    total = sum(len(t) for t in tables.values())
    print(f"[+] {len(files)} files, {len(records)} DLLs, {total} exports", file=sys.stderr)

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
