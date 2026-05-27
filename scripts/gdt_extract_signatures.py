#!/usr/bin/env python3
"""
gdt_extract_signatures.py
─────────────────────────
Extract function signatures from a Ghidra Data Type (.gdt) archive and emit
them in Fission's pipe-delimited format:

    FunctionName|ReturnType|param0:Type0,param1:Type1,...

Usage:
    python3 scripts/gdt_extract_signatures.py \
        utils/signatures/typeinfo/generic/generic_clib.gdt \
        --output utils/signatures/typeinfo/generic/generic_clib_signatures.txt

    python3 scripts/gdt_extract_signatures.py \
        utils/signatures/typeinfo/win/win_api_signatures.gdt \
        --output utils/signatures/typeinfo/win/win_api_signatures.txt

How it works:
  .gdt = [Java ObjectInputStream header] + [ZIP DEFLATE stream] + [Ghidra BufferFile DB]

  The Ghidra BufferFile DB stores B-tree tables. We:
  1. Decompress the buffer file from the .gdt archive
  2. Scan all buffers for LongKey / FixedKey var-record leaf nodes
  3. Extract name strings from function-definition and parameter records
  4. Build type-name map from built-in/typedef tables
  5. Assemble name|ret|params output
"""

import argparse
import re
import struct
import sys
import zlib
from collections import defaultdict
from typing import Dict, List, Optional, Tuple


# ─── Buffer file constants (from LocalBufferFile.java) ────────────────────────
GBF_MAGIC           = 0x2f30312c34292c2a
BUFFER_PREFIX_SIZE  = 5    # flags(1) + buf_id(4)

# Node type constants (from NodeMgr.java)
NODE_LONGKEY_INTERIOR   = 0
NODE_LONGKEY_VAR_REC    = 1
NODE_LONGKEY_FIXED_REC  = 2
NODE_VARKEY_INTERIOR    = 3
NODE_VARKEY_REC         = 4
NODE_FIXEDKEY_INTERIOR  = 5
NODE_FIXEDKEY_VAR_REC   = 6
NODE_FIXEDKEY_FIXED_REC = 7
NODE_CHAINED_INDEX      = 8
NODE_CHAINED_DATA       = 9

# All leaf record node types
LEAF_NODE_TYPES = {NODE_LONGKEY_VAR_REC, NODE_LONGKEY_FIXED_REC,
                   NODE_VARKEY_REC, NODE_FIXEDKEY_VAR_REC, NODE_FIXEDKEY_FIXED_REC}


# ─── GDT decompression ───────────────────────────────────────────────────────

def read_gdt_buffer_file(path: str) -> bytes:
    """Decompress a .gdt archive and return the raw Ghidra BufferFile bytes."""
    with open(path, "rb") as f:
        data = f.read()

    # Java ObjectInputStream: ac ed 00 05 77 <block_len>
    if data[:2] != b"\xac\xed":
        raise ValueError("Not a Java serialization stream")
    if data[4] != 0x77:
        raise ValueError("Expected TC_BLOCKDATA (0x77)")
    block_len = data[5]
    zip_start = 6 + block_len

    # ZIP local file header: PK\x03\x04
    if data[zip_start:zip_start+4] != b"PK\x03\x04":
        raise ValueError("Expected ZIP local file header after Java header")

    fname_len  = struct.unpack("<H", data[zip_start+26:zip_start+28])[0]
    extra_len  = struct.unpack("<H", data[zip_start+28:zip_start+30])[0]
    data_start = zip_start + 30 + fname_len + extra_len

    # Raw DEFLATE (no wbits wrapper)
    d = zlib.decompressobj(-15)
    chunks: List[bytes] = []
    pos, chunk = data_start, 65536
    while pos < len(data):
        try:
            out = d.decompress(data[pos:pos+chunk])
            if out:
                chunks.append(out)
            pos += chunk
        except zlib.error:
            break
    return b"".join(chunks)


# ─── BufferFile reader ───────────────────────────────────────────────────────

class BufferFile:
    def __init__(self, raw: bytes):
        self.raw = raw
        # Parse header fields
        magic = struct.unpack(">Q", raw[0:8])[0]
        if magic != GBF_MAGIC:
            raise ValueError(f"Bad GBF magic: 0x{magic:016x}")
        self.block_size  = struct.unpack(">i", raw[20:24])[0]
        self.buffer_size = self.block_size - BUFFER_PREFIX_SIZE
        self.num_blocks  = len(raw) // self.block_size

    def num_user_buffers(self) -> int:
        return self.num_blocks - 1  # block 0 = file header

    def get_buffer(self, buf_idx: int) -> Optional[bytes]:
        """Return raw buffer data (without prefix) or None if empty/missing."""
        block_idx = buf_idx + 1
        offset    = block_idx * self.block_size
        if offset + self.block_size > len(self.raw):
            return None
        flags = self.raw[offset]
        if flags & 0x01:    # EMPTY_BUFFER
            return None
        return self.raw[offset + BUFFER_PREFIX_SIZE : offset + self.block_size]


# ─── Record extraction ───────────────────────────────────────────────────────

def _i32(buf: bytes, off: int) -> int:
    return struct.unpack(">i", buf[off:off+4])[0]

def _u32(buf: bytes, off: int) -> int:
    return struct.unpack(">I", buf[off:off+4])[0]

def _i64(buf: bytes, off: int) -> int:
    return struct.unpack(">q", buf[off:off+8])[0]


def read_string(buf: bytes, off: int) -> Tuple[Optional[str], int]:
    """Read a Ghidra DB string (int length prefix + UTF-8 bytes).
    Returns (string_or_None, new_offset)."""
    if off + 4 > len(buf):
        return None, off
    length = _i32(buf, off)
    off += 4
    if length < 0:
        return None, off
    if off + length > len(buf):
        return None, off
    try:
        s = buf[off:off+length].decode("utf-8")
    except UnicodeDecodeError:
        s = buf[off:off+length].decode("latin-1")
    return s, off + length


def extract_longkey_var_rec_leaf(buf: bytes, key_size: int = 8) -> List[Tuple[bytes, bytes]]:
    """
    Extract (key_bytes, record_bytes) pairs from a LongKeyVarRecNode (VarRecNode).

    Layout (from VarRecNode.java):
      NodeType(1) | KeyCount(4) | PrevLeaf(4) | NextLeaf(4)  = 13 bytes header
      Per entry: key(8) + data_offset(4) + indirect_flag(1)  = 13 bytes  [grows upward]
      Record data                                             [grows downward from end]
    """
    HEADER    = 13
    KEY_SZ    = 8
    ENTRY     = KEY_SZ + 4 + 1   # key(8) + offset(4) + indflag(1) = 13

    kc = _i32(buf, 1)
    if kc <= 0 or kc > 10000:
        return []

    records = []
    for i in range(kc):
        base      = HEADER + i * ENTRY
        if base + ENTRY > len(buf):
            break
        key_bytes    = buf[base:base+KEY_SZ]
        data_off     = _i32(buf, base + KEY_SZ)
        indirect_flag= buf[base + KEY_SZ + 4]

        if indirect_flag:
            # Record stored in chained DBBuffer; data_off is buffer ID, not offset.
            # We skip these for now (they are rare for type/function definition tables).
            continue

        if not (0 <= data_off < len(buf)):
            continue

        # Record length: from data_off to the previous entry's offset (or buf end for i==0).
        if i == 0:
            rec_end = len(buf)
        else:
            prev_base = HEADER + (i - 1) * ENTRY
            rec_end   = _i32(buf, prev_base + KEY_SZ)
        if data_off >= rec_end:
            continue
        records.append((key_bytes, buf[data_off:rec_end]))
    return records


def extract_fixedkey_var_rec_leaf(buf: bytes, key_size: int) -> List[Tuple[bytes, bytes]]:
    """
    Extract (key_bytes, record_bytes) from a FixedKeyVarRecNode.

    Layout:
      NodeType(1) | KeyCount(4) | PrevLeaf(4) | NextLeaf(4)  = 13 bytes header
      Per entry: key(key_size) + data_offset(4) + indirect_flag(1)  [grows upward]
      Record data  [grows downward from end]
    """
    HEADER    = 13
    ENTRY     = key_size + 4 + 1   # +1 for indirect_flag byte

    kc = _i32(buf, 1)
    if kc <= 0 or kc > 10000:
        return []

    records = []
    for i in range(kc):
        base         = HEADER + i * ENTRY
        if base + ENTRY > len(buf):
            break
        key_bytes    = buf[base:base+key_size]
        data_off     = _i32(buf, base + key_size)
        indirect_flag= buf[base + key_size + 4]

        if indirect_flag:
            continue

        if not (0 <= data_off < len(buf)):
            continue
        if i == 0:
            rec_end = len(buf)
        else:
            prev_base = HEADER + (i - 1) * ENTRY
            rec_end   = _i32(buf, prev_base + key_size)
        if data_off >= rec_end:
            continue
        records.append((key_bytes, buf[data_off:rec_end]))
    return records


# ─── Walk all leaf nodes ──────────────────────────────────────────────────────

def walk_all_leaves(bf: BufferFile) -> List[Tuple[int, bytes, bytes, bytes]]:
    """
    Walk all user buffers and return leaf-node records as:
      (buf_idx, node_type_byte, key_bytes, record_bytes)
    This is a best-effort scan that doesn't require knowing the B-tree root.
    """
    results = []
    for buf_idx in range(bf.num_user_buffers()):
        buf = bf.get_buffer(buf_idx)
        if buf is None or len(buf) < 13:
            continue
        node_type = buf[0]
        if node_type not in (NODE_LONGKEY_VAR_REC, NODE_FIXEDKEY_VAR_REC):
            continue

        kc = _i32(buf, 1)
        if kc <= 0 or kc > 5000:
            continue

        if node_type == NODE_LONGKEY_VAR_REC:
            pairs = extract_longkey_var_rec_leaf(buf, key_size=8)
        else:
            # We need to know the key size for FixedKey tables.
            # Try key_size=16 (UniversalID) first, then 8.
            pairs = []
            for ks in (16, 8, 4):
                try:
                    p = extract_fixedkey_var_rec_leaf(buf, key_size=ks)
                    if p:
                        pairs = p
                        break
                except Exception:
                    pass

        for key_bytes, rec_bytes in pairs:
            results.append((buf_idx, bytes([node_type]), key_bytes, rec_bytes))
    return results


# ─── Function-record parsing ──────────────────────────────────────────────────

def try_parse_func_def(rec: bytes) -> Optional[dict]:
    """
    Try to parse a record as a Function Definition:
      Name(string) | Comment(string) | CategoryID(long) | ReturnTypeID(long) |
      Flags(byte) | CallConvID(int) | ...
    Returns dict or None if parsing fails / result looks implausible.
    """
    pos = 0
    name, pos = read_string(rec, pos)
    if not name or not re.match(r"^[A-Za-z_][A-Za-z0-9_@$?]*$", name):
        return None
    if len(name) > 256 or len(name) < 1:
        return None

    comment, pos = read_string(rec, pos)

    if pos + 8 > len(rec):
        return {"name": name}

    cat_id = _i64(rec, pos);  pos += 8
    if pos + 8 > len(rec):
        return {"name": name, "category_id": cat_id}

    ret_id = _i64(rec, pos);  pos += 8
    if pos + 1 > len(rec):
        return {"name": name, "category_id": cat_id, "ret_id": ret_id}

    flags = rec[pos]; pos += 1
    if pos + 4 > len(rec):
        return {"name": name, "category_id": cat_id, "ret_id": ret_id, "flags": flags}

    callconv_id = _i32(rec, pos); pos += 4
    return {
        "name":         name,
        "comment":      comment,
        "category_id":  cat_id,
        "ret_id":       ret_id,
        "flags":        flags,
        "callconv_id":  callconv_id,
    }


def try_parse_func_param(key_bytes: bytes, rec: bytes) -> Optional[dict]:
    """
    Try to parse a record as a Function Parameter:
      ParentID(long) | DataTypeID(long) | Name(string) | Comment(string) |
      Ordinal(int) | DataTypeLength(int)
    """
    pos = 0
    if pos + 8 > len(rec):
        return None
    parent_id = _i64(rec, pos);  pos += 8
    if pos + 8 > len(rec):
        return None
    dt_id = _i64(rec, pos);  pos += 8

    name, pos = read_string(rec, pos)
    comment, pos = read_string(rec, pos)

    ordinal = 0
    if pos + 4 <= len(rec):
        ordinal = _i32(rec, pos); pos += 4

    dt_len = 0
    if pos + 4 <= len(rec):
        dt_len = _i32(rec, pos)

    return {
        "parent_id":  parent_id,
        "dt_id":      dt_id,
        "name":       name or f"param{ordinal}",
        "ordinal":    ordinal,
        "dt_len":     dt_len,
    }


def try_parse_type_name(rec: bytes) -> Optional[Tuple[int, str]]:
    """
    Try to parse a record as a type-name entry (Built-in / Typedef / Composite).
    Returns (dt_id_key_as_int, type_name) or None.
    """
    pos = 0
    name, pos = read_string(rec, pos)
    if not name or not re.match(r"^[A-Za-z_][A-Za-z0-9_* ]*$", name):
        return None
    return name


# ─── Main extraction logic ────────────────────────────────────────────────────

_FUNC_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_@$?]*$")
_TYPE_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_ *\[\]]*$")


def extract_builtin_type_map(bf: BufferFile) -> Dict[int, str]:
    """
    Extract the Built-in datatypes table from the GDT database.

    Ghidra stores built-in types in a LONGKEY_VAR_REC table (type=1) whose
    records are: Name(string) | ClassName(string) | CategoryID(long).
    The DB key is the internal type ID (a positive long starting near 100).

    We scan all LONGKEY_VAR_REC leaf buffers and return a {type_id: name} map
    for records whose class name contains 'DataType' (a reliable heuristic).
    """
    type_map: Dict[int, str] = {}
    ENTRY = 8 + 4 + 1  # VarRecNode entry = key(8) + offset(4) + indflag(1)
    HEADER = 13

    for buf_idx in range(bf.num_user_buffers()):
        buf = bf.get_buffer(buf_idx)
        if buf is None or len(buf) < HEADER:
            continue
        if buf[0] != NODE_LONGKEY_VAR_REC:
            continue
        kc = _i32(buf, 1)
        if kc <= 0 or kc > 200:   # built-in table is small (14 entries typical)
            continue

        local: Dict[int, str] = {}
        for i in range(kc):
            base = HEADER + i * ENTRY
            if base + ENTRY > len(buf):
                break
            key_val  = _i64(buf, base)
            data_off = _i32(buf, base + 8)
            ind_flag = buf[base + 12]
            if ind_flag or not (0 <= data_off < len(buf)):
                continue
            if i == 0:
                rec_end = len(buf)
            else:
                prev_base = HEADER + (i - 1) * ENTRY
                rec_end = _i32(buf, prev_base + 8)
            if data_off >= rec_end:
                continue
            rec = buf[data_off:rec_end]
            name, pos = read_string(rec, 0)
            if not name:
                continue
            classname, _ = read_string(rec, pos)
            # Only accept genuine Ghidra data type class names
            if classname and 'DataType' in classname:
                local[key_val] = name

        # Accept this buffer if most names look like primitive C / Ghidra type names.
        # We use any() rather than all() because some GDT files include extra built-ins
        # like sbyte/byte that are not in the minimal PRIMITIVE set.
        PRIMITIVE = {"void","char","uchar","short","ushort","int","uint",
                     "long","ulong","longlong","ulonglong","float","double",
                     "wchar_t","bool","byte","sbyte","word","dword","qword",
                     "undefined","unicode","string","unicode32","pointer"}
        if local and any(v in PRIMITIVE for v in local.values()):
            type_map.update(local)

    return type_map


def extract_all(bf: BufferFile, verbose: bool = False) -> List[str]:
    """
    Main extraction: scan all leaf nodes, classify records, build signatures.

    Three-pass approach:
      1. Extract built-in type ID→name map (small table, 14 entries typical)
      2. Scan every LONGKEY_VAR_REC leaf for function definitions and parameters
      3. Resolve type IDs → type name strings, then assemble pipe-delimited sigs
    """
    # ── Pass 0: extract built-in type map ────────────────────────────────────
    builtin_type_map = extract_builtin_type_map(bf)
    if verbose:
        print(f"  [builtin] {len(builtin_type_map)} built-in types: "
              + ", ".join(f"{k}={v}" for k,v in sorted(builtin_type_map.items())[:6]),
              file=sys.stderr)

    all_leaves = walk_all_leaves(bf)
    if verbose:
        print(f"  [scan] {len(all_leaves)} raw records from {bf.num_user_buffers()} buffers",
              file=sys.stderr)

    # ── Pass 1: collect typedef / composite / enum type names ─────────────────
    # These supplement the built-in map with user-defined type names.
    type_name_by_id: Dict[int, str] = dict(builtin_type_map)  # seed from built-ins

    # ── Pass 2: collect function defs ────────────────────────────────────────
    func_defs:   Dict[int, dict] = {}    # key_int → parsed func def record
    func_params: List[dict]      = []    # all parsed param records

    for buf_idx, node_type_b, key_bytes, rec in all_leaves:
        key_int = _i64(key_bytes, 0) if len(key_bytes) == 8 else int.from_bytes(key_bytes, "big")

        # Try as func def first
        fd = try_parse_func_def(rec)
        if fd and "name" in fd:
            func_defs[id(rec)] = {"key": key_int, **fd}
            continue

        # Try as type-name record (typedef / composite / enum)
        name = try_parse_type_name(rec)
        if name and key_int > 0:
            type_name_by_id[key_int] = name

    # ── Pass 3: collect param records ────────────────────────────────────────
    for buf_idx, node_type_b, key_bytes, rec in all_leaves:
        key_int = _i64(key_bytes, 0) if len(key_bytes) == 8 else int.from_bytes(key_bytes, "big")

        p = try_parse_func_param(key_bytes, rec)
        if (p and p.get("parent_id") and p.get("name") and
                re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", p["name"] or "")):
            func_params.append({"key": key_int, **p})

    if verbose:
        print(f"  [parse] {len(func_defs)} function defs, "
              f"{len(func_params)} param records, "
              f"{len(type_name_by_id)} type names",
              file=sys.stderr)

    # ── Pass 4: resolve type IDs → names ────────────────────────────────────
    def resolve_type(type_id: int) -> str:
        if type_id in type_name_by_id:
            return type_name_by_id[type_id]
        if type_id == 0:
            return "void"
        if type_id < 0:
            # Negative IDs are Ghidra source-archive references; treat as int
            return "int"
        return "int"

    # ── Pass 5: group params by parent_id ────────────────────────────────────
    params_by_parent: Dict[int, List[dict]] = defaultdict(list)
    for p in func_params:
        params_by_parent[p["parent_id"]].append(p)
    for pid in params_by_parent:
        params_by_parent[pid].sort(key=lambda x: x.get("ordinal", 0))

    # ── Pass 6: assemble signatures ──────────────────────────────────────────
    sigs: List[str] = []
    seen_names: set = set()

    for rec_obj in func_defs.values():
        name = rec_obj.get("name", "")
        if not name or not _FUNC_NAME_RE.match(name):
            continue
        if name in seen_names:
            continue
        seen_names.add(name)

        ret_id   = rec_obj.get("ret_id", 0) or 0
        ret_type = resolve_type(ret_id)

        func_key = rec_obj.get("key", 0)
        params   = params_by_parent.get(func_key, [])

        if not params:
            sigs.append(f"{name}|{ret_type}|void")
        else:
            param_strs = []
            for p in params:
                pname  = re.sub(r"[^A-Za-z0-9_]", "_", p.get("name") or "param")
                pt_id  = p.get("dt_id", 0) or 0
                ptype  = resolve_type(pt_id)
                param_strs.append(f"{pname}:{ptype}")
            sigs.append(f"{name}|{ret_type}|{','.join(param_strs)}")

    return sorted(set(sigs))


# ─── Fallback: pure string scan ──────────────────────────────────────────────

def fallback_string_scan(bf: BufferFile) -> List[str]:
    """
    Fallback: scan all buffers for length-prefixed identifier strings.
    Returns bare 'name|int|void' lines for each plausible function name found.
    """
    seen: set = set()
    results: List[str] = []

    for buf_idx in range(bf.num_user_buffers()):
        buf = bf.get_buffer(buf_idx)
        if buf is None:
            continue
        i = 0
        while i < len(buf) - 8:
            slen = _i32(buf, i)
            if 2 <= slen <= 128:
                cand = buf[i+4:i+4+slen]
                if len(cand) == slen:
                    try:
                        s = cand.decode("ascii")
                        if _FUNC_NAME_RE.match(s) and s not in seen:
                            seen.add(s)
                            # Try to extract return type from context
                            # After name: comment(null=-1), category_id(8), ret_id(8)
                            ctx_off = i + 4 + slen
                            ret_type = "int"
                            if ctx_off + 4 <= len(buf):
                                comment_len = _i32(buf, ctx_off)
                                if comment_len < 0:
                                    # null comment
                                    ctx_off2 = ctx_off + 4
                                    if ctx_off2 + 16 <= len(buf):
                                        # cat_id(8) + ret_id(8)
                                        ret_id = _i64(buf, ctx_off2 + 8)
                                        if ret_id == 0:
                                            ret_type = "void"
                                        elif ret_id < 0:
                                            ret_type = "int"
                                        # else: keep "int" as default
                            results.append(f"{s}|{ret_type}|void")
                        i += 4 + slen
                        continue
                    except (UnicodeDecodeError, ValueError):
                        pass
            i += 1

    # Filter: only keep names that look like C identifiers
    # (exclude things like 'Base', 'NULL', 'TRUE', 'FALSE', single-word types)
    EXCLUDE = {"NULL", "TRUE", "FALSE", "true", "false", "void", "int", "char",
               "long", "short", "double", "float", "unsigned", "signed",
               "const", "struct", "union", "enum", "typedef", "extern",
               "static", "inline", "register", "volatile", "auto",
               "Base", "Name", "Key", "Value", "Type", "Data", "Flag",
               "Index", "List", "Map", "Set", "Node", "Entry", "Handle",
               "Buffer", "File", "Path", "Error", "Status", "Result",
               "Info", "Desc", "Attr", "Size", "Count", "Offset", "Length"}

    return sorted({r for r in results
                   if r.split("|")[0] not in EXCLUDE
                   and len(r.split("|")[0]) >= 2})


# ─── Main ─────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(
        description="Extract function signatures from a Ghidra .gdt archive file"
    )
    ap.add_argument("gdt_file", help="Path to the .gdt file")
    ap.add_argument("--output", "-o", default="-",
                    help="Output .txt file path (default: stdout)")
    ap.add_argument("--fallback-only", action="store_true",
                    help="Use only the string-scan fallback (no B-tree parsing)")
    ap.add_argument("--verbose", "-v", action="store_true",
                    help="Print debug info to stderr")
    args = ap.parse_args()

    if args.verbose:
        print(f"[+] Reading: {args.gdt_file}", file=sys.stderr)

    raw = read_gdt_buffer_file(args.gdt_file)
    if args.verbose:
        print(f"[+] Decompressed buffer file: {len(raw):,} bytes", file=sys.stderr)

    bf = BufferFile(raw)
    if args.verbose:
        print(f"[+] Block size: {bf.block_size}, user buffers: {bf.num_user_buffers()}",
              file=sys.stderr)

    if args.fallback_only:
        sigs = fallback_string_scan(bf)
        if args.verbose:
            print(f"[+] String scan: {len(sigs)} identifiers", file=sys.stderr)
    else:
        sigs = extract_all(bf, verbose=args.verbose)
        if not sigs:
            if args.verbose:
                print("[!] B-tree extraction yielded 0 sigs; falling back to string scan",
                      file=sys.stderr)
            sigs = fallback_string_scan(bf)
            if args.verbose:
                print(f"[+] String scan fallback: {len(sigs)} identifiers", file=sys.stderr)

    if args.output == "-":
        for line in sigs:
            print(line)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            for line in sigs:
                f.write(line + "\n")
        if args.verbose:
            print(f"[+] Wrote {len(sigs)} signatures → {args.output}", file=sys.stderr)
        else:
            print(f"Wrote {len(sigs)} signatures → {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
