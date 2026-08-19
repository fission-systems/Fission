#!/usr/bin/env python3
"""Pack the merged struct corpus into `.fpk`.

`WindowsStructures::load` reads six JSON files and merges them into one map:

    structures.json                (base)
    rust_structures.json           (overwrite: a Rust layout wins)
    phnt_structures.json           (additive)
    windows_vs12_structures.json   (additive)
    generic_clib_structures.json   (additive)
    mac_osx_structures.json        (additive)

28.5 MB of JSON re-parsed into a 26k-entry HashMap on first use. The merge is
deterministic, so it happens here instead and only the merged result ships.

Two tables, because there are two access patterns:

  <out>/structures.fpk         name -> the struct, for `get(name)`
  <out>/structures.bysize.fpk  "<width>:<size>" -> names, for the reverse
                               lookup in `infer_struct_name_from_offsets`,
                               which today scans all 26k entries filtering on
                               an exact size match.

Usage:
  scripts/pack_structures.py utils/source/typeinfo utils/signatures/typeinfo/win32
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fpk_pack import build  # noqa: E402

# (relative path under the source root, overwrite?) in `load`'s exact order.
SOURCES = [
    ("win32/structures.json", True),
    ("rust/rust_structures.json", True),
    ("win32/phnt_structures.json", False),
    ("win32/windows_vs12_structures.json", False),
    ("generic/generic_clib_structures.json", False),
    ("mac_10.9/mac_osx_structures.json", False),
]


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        raise SystemExit(2)
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)

    merged: dict[str, dict] = {}
    for rel, overwrite in SOURCES:
        path = src / rel
        if not path.exists():
            print(f"  skip (absent): {rel}")
            continue
        items = json.loads(path.read_text())
        added = kept = 0
        for item in items:
            name = item.get("name")
            if not name:
                continue
            if name in merged and not overwrite:
                kept += 1
                continue
            merged[name] = item
            added += 1
        print(f"  {rel}: {len(items)} entries, {added} written, {kept} kept from earlier")

    # Name table. One compact JSON object per line, keyed by "name" -- the
    # `json-lines` kind the packer already understands.
    records = [
        json.dumps(merged[name], separators=(",", ":"), sort_keys=True)
        for name in sorted(merged)
    ]
    blob = build(records, "json-lines")
    (out / "structures.fpk").write_bytes(blob)
    print(f"structures.fpk: {len(records)} structs, {len(blob)/1048576:.1f} MB")

    # Size index. Key is "<width>:<zero-padded size>" so it sorts numerically
    # within a width and a single block holds neighbouring sizes.
    by_size: dict[str, list[str]] = {}
    for name, item in merged.items():
        for width, field in (("32", "size_32"), ("64", "size_64")):
            size = item.get(field) or 0
            if size <= 0:
                continue  # 0 means "unknown"; the scan already never matches it
            by_size.setdefault(f"{width}:{size:010d}", []).append(name)
    size_records = [
        f"{key}|{','.join(sorted(names))}" for key, names in sorted(by_size.items())
    ]
    size_blob = build(size_records, "pipe-text")
    (out / "structures.bysize.fpk").write_bytes(size_blob)
    print(f"structures.bysize.fpk: {len(size_records)} sizes, {len(size_blob)/1048576:.1f} MB")

    total = sum((src / rel).stat().st_size for rel, _ in SOURCES if (src / rel).exists())
    packed = len(blob) + len(size_blob)
    print(f"\n{total/1048576:.1f} MB of JSON -> {packed/1048576:.1f} MB packed "
          f"({packed/total*100:.1f}%)")


if __name__ == "__main__":
    main()
