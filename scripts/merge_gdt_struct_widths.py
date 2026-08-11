#!/usr/bin/env python3
"""
merge_gdt_struct_widths.py
─────────────────────────────
Merge a pair of single-arch struct extractions (from
scripts/gdt_extract_structs.py, run once against a 32-bit-target .gdt and
once against its 64-bit-target sibling, e.g. generic_clib.gdt +
generic_clib_64.gdt or windows_vs12_32.gdt + windows_vs12_64.gdt) into one
dual-width structures.json-shaped file.

Why this exists: a single .gdt archive only ever describes ONE pointer
width. gdt_extract_structs.py necessarily reports the same (correct only
for that one width) size in both its size_32 and size_64 output fields
when run on a single archive -- confirmed wrong empirically (e.g. 32-bit
`struct stat` is 88 bytes, 64-bit is 144; a single-archive extraction of
either file claims BOTH widths are its own number). This script fixes that
by pulling size_32/offset_32 from the 32-bit extraction and size_64/
offset_64 from the 64-bit extraction for every struct name present in
BOTH -- typically ~95-98% of names given the two archives describe the
same headers compiled for different targets. A struct present in only one
extraction is skipped entirely rather than guessing its missing width.

Usage:
    python3 scripts/merge_gdt_struct_widths.py \
        generic_clib_32.json generic_clib_64.json \
        --output utils/signatures/typeinfo/generic/generic_clib_structures.json
"""

import argparse
import json
import sys
from typing import Dict


def merge(items32, items64) -> list:
    by_name_32 = {i["name"]: i for i in items32}
    by_name_64 = {i["name"]: i for i in items64}
    common_names = sorted(set(by_name_32) & set(by_name_64))

    out = []
    skipped_field_mismatch = 0
    for name in common_names:
        s32 = by_name_32[name]
        s64 = by_name_64[name]
        fields_32_by_name: Dict[str, dict] = {f["name"]: f for f in s32["fields"]}
        fields_64_by_name: Dict[str, dict] = {f["name"]: f for f in s64["fields"]}
        common_fields = [f for f in s64["fields"] if f["name"] in fields_32_by_name]
        if len(common_fields) != len(s64["fields"]) or len(common_fields) != len(s32["fields"]):
            # Field sets don't line up 1:1 between the two widths (e.g. a
            # union/anonymous-member naming quirk) -- don't guess, skip.
            skipped_field_mismatch += 1
            continue

        merged_fields = []
        for f64 in s64["fields"]:
            f32 = fields_32_by_name[f64["name"]]
            merged_fields.append({
                "name": f64["name"],
                "type_name": f64["type_name"],
                "offset_32": f32["offset_32"],
                "offset_64": f64["offset_64"],
                "size_32": f32["size_32"],
                "size_64": f64["size_64"],
            })

        out.append({
            "name": name,
            "size_32": s32["size_32"],
            "size_64": s64["size_64"],
            "fields": merged_fields,
        })

    if skipped_field_mismatch:
        print(f"[+] skipped {skipped_field_mismatch} structs with mismatched field sets between widths",
              file=sys.stderr)
    return out


def main():
    ap = argparse.ArgumentParser(description="Merge two single-arch struct extractions into one dual-width file")
    ap.add_argument("json_32bit")
    ap.add_argument("json_64bit")
    ap.add_argument("--output", "-o", default="-")
    ap.add_argument("--verbose", "-v", action="store_true")
    args = ap.parse_args()

    items32 = json.load(open(args.json_32bit, encoding="utf-8"))
    items64 = json.load(open(args.json_64bit, encoding="utf-8"))
    if args.verbose:
        print(f"[+] 32-bit: {len(items32)} structs, 64-bit: {len(items64)} structs", file=sys.stderr)

    merged = merge(items32, items64)
    merged.sort(key=lambda s: s["name"])

    out_text = json.dumps(merged, indent=2)
    if args.output == "-":
        print(out_text)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out_text)
        print(f"Wrote {len(merged)} dual-width structs -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
