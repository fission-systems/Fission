#!/usr/bin/env python3
"""
retdec_ordinals_extract.py
────────────────────────────
Merge RetDec's per-DLL ordinal->export-name tables
(vendor/retdec-5.0/support/ordinals/{x86,arm}/*.ord, one file per DLL, lines
of "<ordinal> <export_name>") into a single JSON lookup table Fission can
ship under utils/signatures/ordinals/.

Why this exists: PE imports made by ordinal (no name, just a numeric index
into the exporting DLL's export table -- common in older DLLs and some
obfuscated/packed binaries) currently resolve to a synthetic placeholder
like "USER32.dll:Ordinal_17" in fission-loader's PE import parser, which is
useless for downstream signature/hint lookups. This table lets the loader
resolve ordinal imports back to their real exported names.

Usage:
    python3 scripts/retdec_ordinals_extract.py \
        vendor/retdec-5.0/support/ordinals/x86 \
        --output utils/signatures/ordinals/x86_ordinals.json
"""

import argparse
import json
import os
import sys
from typing import Dict


def parse_ord_file(path: str) -> Dict[str, str]:
    entries: Dict[str, str] = {}
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split(" ", 1)
            if len(parts) != 2:
                continue
            ordinal_str, name = parts
            if not ordinal_str.isdigit():
                continue
            name = name.strip()
            if not name:
                continue
            entries[ordinal_str] = name
    return entries


def main():
    ap = argparse.ArgumentParser(description="Merge RetDec .ord ordinal tables into one JSON file")
    ap.add_argument("ord_dir", help="Directory of .ord files (one per DLL)")
    ap.add_argument("--output", "-o", default="-", help="Output .json path (default: stdout)")
    ap.add_argument("--verbose", "-v", action="store_true")
    args = ap.parse_args()

    merged: Dict[str, Dict[str, str]] = {}
    n_files = 0
    n_entries = 0
    for fname in sorted(os.listdir(args.ord_dir)):
        if not fname.endswith(".ord"):
            continue
        dll_name = os.path.splitext(fname)[0].lower() + ".dll"
        entries = parse_ord_file(os.path.join(args.ord_dir, fname))
        if not entries:
            continue
        merged[dll_name] = entries
        n_files += 1
        n_entries += len(entries)

    if args.verbose:
        print(f"[+] {n_files} DLLs, {n_entries} ordinal entries", file=sys.stderr)

    out_text = json.dumps(merged, sort_keys=True, separators=(",", ":"))
    if args.output == "-":
        print(out_text)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out_text)
        print(f"Wrote {n_files} DLLs / {n_entries} entries -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
