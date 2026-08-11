#!/usr/bin/env python3
"""
gdt_extract_structs.py
───────────────────────
Extract struct/composite-type layouts from a Ghidra Data Type (.gdt) archive
and emit them in Fission's `structures.json` shape:

    [{"name": "String", "size_32": 24, "size_64": 24,
      "fields": [{"name": "data_ptr", "type_name": "u8*",
                  "offset_32": 0, "offset_64": 0, "size_32": 8, "size_64": 8}, ...]}]

Usage:
    python3 scripts/gdt_extract_structs.py \
        utils/signatures/typeinfo/rust/rust-common.gdt \
        --output utils/signatures/typeinfo/rust/rust_structures.json

How it works (companion to gdt_extract_signatures.py, same BufferFile format):
  Every Ghidra "Data Type ID" is a 64-bit value whose top byte encodes which
  DB table it lives in (see DataTypeManagerDB.createKey/DATA_TYPE_KIND_SHIFT):
    0=BUILT_IN 1=COMPOSITE 2=COMPONENT 3=ARRAY 4=POINTER 5=TYPEDEF
    6=FUNCTION_DEF 7=PARAMETER 8=ENUM
  Composite (struct) records live in a LONGKEY_VAR_REC table (Name, Comment,
  IsUnion, CategoryID, Length, Alignment, NumComponents, ...). Component
  (field) records live in another LONGKEY_VAR_REC table keyed by their own
  id, referencing their owning struct via a Parent field that equals the
  struct's key. Pointer records are fixed-length (17 bytes: DataTypeID,
  CategoryID, Length) and live in a LONGKEY_FIXED_REC table, which is why
  the sibling function-signature extractor (which only reads VAR_REC nodes)
  never surfaces them.
"""

import argparse
import json
import re
import struct
import sys
from typing import Dict, List, Optional, Tuple

sys.path.insert(0, __import__("os").path.dirname(__import__("os").path.abspath(__file__)))
from gdt_extract_signatures import (
    read_gdt_buffer_file, BufferFile, walk_all_leaves,
    NODE_LONGKEY_FIXED_REC, _i32, _i64, read_string,
)

# ─── Data type "kind" encoding (DataTypeManagerDB.DATA_TYPE_KIND_SHIFT=56) ────
KIND_BUILT_IN    = 0
KIND_COMPOSITE   = 1
KIND_COMPONENT   = 2
KIND_ARRAY       = 3
KIND_POINTER     = 4
KIND_TYPEDEF     = 5
KIND_FUNCTION_DEF= 6
KIND_PARAMETER   = 7
KIND_ENUM        = 8

POINTER_RECORD_LENGTH = 17  # DataTypeID(8) + CategoryID(8) + Length(1), fixed


def split_kind(type_id: int) -> Tuple[int, int]:
    """Split a 64-bit Data Type ID into (kind, table_key)."""
    return (type_id >> 56) & 0xFF, type_id & 0x00FFFFFFFFFFFFFF


def _i16(buf: bytes, off: int) -> int:
    return struct.unpack(">h", buf[off:off + 2])[0]


def keyint(key_bytes: bytes) -> int:
    return _i64(key_bytes, 0) if len(key_bytes) == 8 else int.from_bytes(key_bytes, "big")


# ─── Record parsers (byte layouts confirmed against Ghidra's own DB adapter
#     source: CompositeDBAdapterV5V6, ComponentDBAdapterV0, TypedefDBAdapterV2,
#     BuiltinDBAdapterV0, PointerDBAdapter) ──────────────────────────────────

def parse_composite(rec: bytes) -> Optional[dict]:
    pos = 0
    name, pos = read_string(rec, pos)
    if not name or not re.match(r"^[A-Za-z_][A-Za-z0-9_<>:, ]*$", name):
        return None
    _comment, pos = read_string(rec, pos)
    if pos + 1 + 8 + 4 + 4 + 4 > len(rec):
        return None
    is_union = rec[pos]; pos += 1
    _cat_id = _i64(rec, pos); pos += 8
    length = _i32(rec, pos); pos += 4
    _alignment = _i32(rec, pos); pos += 4
    num_components = _i32(rec, pos); pos += 4
    return {"name": name, "is_union": bool(is_union), "length": length,
            "num_components": num_components}


def parse_component(rec: bytes) -> Optional[dict]:
    pos = 0
    if pos + 8 + 4 + 8 > len(rec):
        return None
    parent = _i64(rec, pos); pos += 8
    offset = _i32(rec, pos); pos += 4
    dtid = _i64(rec, pos); pos += 8
    field_name, pos = read_string(rec, pos)
    _comment, pos = read_string(rec, pos)
    component_size = _i32(rec, pos) if pos + 4 <= len(rec) else 0
    return {"parent": parent, "offset": offset, "dtid": dtid,
            "name": field_name, "size": component_size}


def parse_typedef(rec: bytes) -> Optional[dict]:
    if len(rec) < 10:
        return None
    underlying_dtid = _i64(rec, 0)
    _flags = _i16(rec, 8)
    name, _pos = read_string(rec, 10)
    if not name:
        return None
    return {"name": name, "underlying_dtid": underlying_dtid}


def parse_builtin(rec: bytes) -> Optional[dict]:
    name, pos = read_string(rec, 0)
    classname, _pos = read_string(rec, pos)
    if not name:
        return None
    return {"name": name, "classname": classname or ""}


def parse_pointer_table(bf: BufferFile) -> Dict[int, dict]:
    """Pointer records are fixed-length and live in LONGKEY_FIXED_REC leaf
    nodes, which walk_all_leaves() (VAR_REC-only) never visits."""
    pointers: Dict[int, dict] = {}
    HEADER = 13
    entry_size = 8 + POINTER_RECORD_LENGTH
    for buf_idx in range(bf.num_user_buffers()):
        buf = bf.get_buffer(buf_idx)
        if buf is None or len(buf) < HEADER or buf[0] != NODE_LONGKEY_FIXED_REC:
            continue
        kc = _i32(buf, 1)
        if kc <= 0 or kc > 10000:
            continue
        for i in range(kc):
            base = HEADER + i * entry_size
            if base + entry_size > len(buf):
                break
            key = _i64(buf, base)
            rec = buf[base + 8:base + 8 + POINTER_RECORD_LENGTH]
            if len(rec) != POINTER_RECORD_LENGTH:
                continue
            pointee_dtid = _i64(rec, 0)
            length_byte = rec[16]
            length = length_byte - 256 if length_byte >= 128 else length_byte
            pointers[key] = {"pointee_dtid": pointee_dtid, "length": length}
    return pointers


# ─── Type-name resolution across tables ───────────────────────────────────────

class TypeResolver:
    def __init__(self, builtins: Dict[int, dict], typedefs: Dict[int, dict],
                 composites: Dict[int, dict], pointers: Dict[int, dict],
                 native_ptr_size: int = 8):
        self.builtins = builtins
        self.typedefs = typedefs
        self.composites = composites
        self.pointers = pointers
        self.native_ptr_size = native_ptr_size

    def resolve(self, type_id: int, depth: int = 0) -> str:
        if depth > 8:
            return "void"
        if type_id == 0:
            return "void"
        kind, _table_key = split_kind(type_id)
        if kind == KIND_BUILT_IN:
            b = self.builtins.get(type_id)
            return b["name"] if b else "int"
        if kind == KIND_TYPEDEF:
            t = self.typedefs.get(type_id)
            return t["name"] if t else "int"
        if kind == KIND_COMPOSITE:
            c = self.composites.get(type_id)
            return c["name"] if c else "void"
        if kind == KIND_POINTER:
            p = self.pointers.get(type_id)
            if not p:
                return "void*"
            return self.resolve(p["pointee_dtid"], depth + 1) + "*"
        if kind == KIND_ENUM:
            return "int"
        return "int"

    def pointer_size(self, type_id: int) -> int:
        kind, _ = split_kind(type_id)
        if kind == KIND_POINTER:
            p = self.pointers.get(type_id)
            if p and p["length"] > 0:
                return p["length"]
            return self.native_ptr_size
        return self.native_ptr_size


# ─── Main extraction ───────────────────────────────────────────────────────

def extract_structures(bf: BufferFile, verbose: bool = False) -> List[dict]:
    all_leaves = walk_all_leaves(bf)

    builtins: Dict[int, dict] = {}
    typedefs: Dict[int, dict] = {}
    composites: Dict[int, dict] = {}      # key -> parsed composite (name/is_union/length)
    components_by_parent: Dict[int, List[dict]] = {}

    for buf_idx, _ntb, key_bytes, rec in all_leaves:
        key = keyint(key_bytes)
        kind, table_key = split_kind(key)

        if kind == KIND_BUILT_IN:
            b = parse_builtin(rec)
            if b and "DataType" in b["classname"]:
                builtins[key] = b
        elif kind == KIND_TYPEDEF:
            t = parse_typedef(rec)
            if t:
                typedefs[key] = t
        elif kind == KIND_COMPOSITE:
            c = parse_composite(rec)
            if c:
                composites[key] = c
        elif kind == KIND_COMPONENT:
            comp = parse_component(rec)
            if comp:
                components_by_parent.setdefault(comp["parent"], []).append(comp)

    pointers = parse_pointer_table(bf)

    if verbose:
        print(f"  [scan] {len(builtins)} builtins, {len(typedefs)} typedefs, "
              f"{len(composites)} composites, {len(pointers)} pointers, "
              f"{sum(len(v) for v in components_by_parent.values())} components",
              file=sys.stderr)

    resolver = TypeResolver(builtins, typedefs, composites, pointers)

    # Reverse index: composite key -> typedef alias names pointing at it
    # (e.g. `typedef struct tagRECT {...} RECT;` registers "RECT" as an
    # alias of tagRECT's composite key), resolved through up to a couple of
    # chained typedef hops (`typedef RECT RECTL;`-style indirection is rare
    # but does happen) rather than assuming direct typedef->composite only.
    NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
    alias_names_by_composite: Dict[int, List[str]] = {}
    for t in typedefs.values():
        name = t.get("name")
        if not name or not NAME_RE.match(name):
            continue
        target = t.get("underlying_dtid")
        for _hop in range(4):
            if target is None:
                break
            kind, _ = split_kind(target)
            if kind == KIND_COMPOSITE:
                alias_names_by_composite.setdefault(target, []).append(name)
                break
            if kind == KIND_TYPEDEF:
                target = typedefs.get(target, {}).get("underlying_dtid")
                continue
            break

    structs: List[dict] = []
    for comp_key, cdef in sorted(composites.items()):
        if cdef["is_union"]:
            continue  # unions don't fit StructDef's flat-offset shape; skip for now
        fields_raw = sorted(components_by_parent.get(comp_key, []), key=lambda f: f["offset"])
        fields = []
        for f in fields_raw:
            if not f["name"] or not re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", f["name"]):
                continue
            type_name = resolver.resolve(f["dtid"])
            size = f["size"] if f["size"] > 0 else resolver.pointer_size(f["dtid"])
            fields.append({
                "name": f["name"],
                "type_name": type_name,
                "offset_32": f["offset"],
                "offset_64": f["offset"],
                "size_32": size,
                "size_64": size,
            })
        if not fields:
            continue

        names = [cdef["name"]]
        seen = {cdef["name"]}
        for alias in alias_names_by_composite.get(comp_key, []):
            if alias not in seen:
                names.append(alias)
                seen.add(alias)

        for name in names:
            structs.append({
                "name": name,
                "size_32": cdef["length"],
                "size_64": cdef["length"],
                "fields": fields,
            })

    return structs


def main():
    ap = argparse.ArgumentParser(
        description="Extract struct/composite type layouts from a Ghidra .gdt archive"
    )
    ap.add_argument("gdt_file", help="Path to the .gdt file")
    ap.add_argument("--output", "-o", default="-", help="Output .json file path (default: stdout)")
    ap.add_argument("--verbose", "-v", action="store_true", help="Print debug info to stderr")
    args = ap.parse_args()

    if args.verbose:
        print(f"[+] Reading: {args.gdt_file}", file=sys.stderr)
    raw = read_gdt_buffer_file(args.gdt_file)
    bf = BufferFile(raw)
    if args.verbose:
        print(f"[+] Block size: {bf.block_size}, user buffers: {bf.num_user_buffers()}", file=sys.stderr)

    structs = extract_structures(bf, verbose=args.verbose)

    out = json.dumps(structs, indent=2)
    if args.output == "-":
        print(out)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out + "\n")
        print(f"Wrote {len(structs)} structs → {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
