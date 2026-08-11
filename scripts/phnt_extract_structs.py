#!/usr/bin/env python3
"""
phnt_extract_structs.py
─────────────────────────
Compute struct layouts (field names, byte offsets, sizes, for both 32- and
64-bit targets) from System Informer's `phnt` headers -- the most complete
maintained set of undocumented Windows NT Native API struct definitions --
and emit them in Fission's `structures.json` shape.

Unlike the Ghidra GDT / angr JSON extractors, phnt ships plain C headers
with NO precomputed offsets, so this script is a (deliberately narrow) C
struct-layout calculator: it applies the standard MSVC layout algorithm
(natural alignment, no #pragma pack in phnt) itself, run independently for
32-bit and 64-bit pointer widths.

Correctness posture: a struct is only emitted if EVERY field's type size/
align was resolved with confidence (from a seeded primitive table, or
transitively from another phnt struct this script already computed).
Anything unresolvable -- an external SDK type this script doesn't know,
a struct body containing #if/#ifdef (offsets could depend on the target
Windows version) -- is silently skipped rather than guessed. This trades
coverage for trustworthiness: `infer_struct_name_from_offsets` shape-matches
on exact byte offsets, so a wrong offset is worse than a missing struct.

Usage:
    python3 scripts/phnt_extract_structs.py \
        vendor/systeminformer-3.2.25011.2103/phnt/include \
        utils/signatures/typeinfo/win32/base_types.json \
        --output utils/signatures/typeinfo/win32/phnt_structures.json
"""

import argparse
import glob
import json
import re
import sys
from typing import Dict, List, Optional, Tuple

# ─── Type info: (size32, align32, size64, align64, is_pointer) ────────────────

class TypeInfo:
    __slots__ = ("size32", "align32", "size64", "align64", "is_pointer")

    def __init__(self, size32, align32, size64, align64, is_pointer=False):
        self.size32 = size32
        self.align32 = align32
        self.size64 = size64
        self.align64 = align64
        self.is_pointer = is_pointer


def make_scalar(size: int) -> TypeInfo:
    return TypeInfo(size, size, size, size)


def make_pointer_sized() -> TypeInfo:
    return TypeInfo(4, 4, 8, 8, is_pointer=True)


PRIMITIVES: Dict[str, TypeInfo] = {
    "VOID": TypeInfo(0, 1, 0, 1),
    "void": TypeInfo(0, 1, 0, 1),
    "CHAR": make_scalar(1), "char": make_scalar(1),
    "UCHAR": make_scalar(1),
    "BYTE": make_scalar(1),
    "BOOLEAN": make_scalar(1),
    "INT8": make_scalar(1), "UINT8": make_scalar(1),
    "int8_t": make_scalar(1), "uint8_t": make_scalar(1),
    "WCHAR": make_scalar(2),
    "SHORT": make_scalar(2), "short": make_scalar(2),
    "USHORT": make_scalar(2),
    "WORD": make_scalar(2),
    "INT16": make_scalar(2), "UINT16": make_scalar(2),
    "int16_t": make_scalar(2), "uint16_t": make_scalar(2),
    "ATOM": make_scalar(2), "LANGID": make_scalar(2), "RTL_ATOM": make_scalar(2),
    "INT": make_scalar(4), "int": make_scalar(4),
    "UINT": make_scalar(4), "unsigned": make_scalar(4),
    "LONG": make_scalar(4), "long": make_scalar(4),
    "ULONG": make_scalar(4),
    "DWORD": make_scalar(4),
    "BOOL": make_scalar(4),
    "FLOAT": make_scalar(4),
    "LONG32": make_scalar(4), "ULONG32": make_scalar(4), "DWORD32": make_scalar(4),
    "INT32": make_scalar(4), "UINT32": make_scalar(4),
    "int32_t": make_scalar(4), "uint32_t": make_scalar(4),
    "LCID": make_scalar(4),
    "NTSTATUS": make_scalar(4),
    "SECURITY_STATUS": make_scalar(4),
    "HRESULT": make_scalar(4),
    "KPRIORITY": make_scalar(4),
    "ACCESS_MASK": make_scalar(4),
    "LOGICAL": make_scalar(4),
    "LONGLONG": make_scalar(8), "ULONGLONG": make_scalar(8),
    "LONG64": make_scalar(8), "ULONG64": make_scalar(8), "DWORD64": make_scalar(8),
    "DWORDLONG": make_scalar(8),
    "INT64": make_scalar(8), "UINT64": make_scalar(8),
    "int64_t": make_scalar(8), "uint64_t": make_scalar(8),
    "DOUBLE": make_scalar(8), "double": make_scalar(8),
    "QWORD": make_scalar(8),
    # pointer-width integers/handles
    "PVOID": make_pointer_sized(), "LPVOID": make_pointer_sized(),
    "HANDLE": make_pointer_sized(),
    "SIZE_T": make_pointer_sized(), "SSIZE_T": make_pointer_sized(),
    "ULONG_PTR": make_pointer_sized(), "LONG_PTR": make_pointer_sized(),
    "DWORD_PTR": make_pointer_sized(),
    "UINT_PTR": make_pointer_sized(), "INT_PTR": make_pointer_sized(),
    "KAFFINITY": make_pointer_sized(),
    "WPARAM": make_pointer_sized(), "LPARAM": make_pointer_sized(),
    "LRESULT": make_pointer_sized(),
    "PWSTR": make_pointer_sized(), "PCWSTR": make_pointer_sized(),
    "PSTR": make_pointer_sized(), "PCSTR": make_pointer_sized(),
    "PWCHAR": make_pointer_sized(), "PCHAR": make_pointer_sized(),
    "PUCHAR": make_pointer_sized(),
    "LPSTR": make_pointer_sized(), "LPCSTR": make_pointer_sized(),
    "LPWSTR": make_pointer_sized(), "LPCWSTR": make_pointer_sized(),
    "PBOOLEAN": make_pointer_sized(),
    "PWCH": make_pointer_sized(), "LPWCH": make_pointer_sized(),
    "PCH": make_pointer_sized(), "LPCH": make_pointer_sized(),
    "PLONG": make_pointer_sized(), "PULONG": make_pointer_sized(),
    "PSHORT": make_pointer_sized(), "PUSHORT": make_pointer_sized(),
    "PINT": make_pointer_sized(), "PUINT": make_pointer_sized(),
    "PBOOL": make_pointer_sized(), "PDWORD": make_pointer_sized(),
    "PWORD": make_pointer_sized(), "PBYTE": make_pointer_sized(),
    "PFLOAT": make_pointer_sized(), "PDOUBLE": make_pointer_sized(),
    "PLONGLONG": make_pointer_sized(), "PULONGLONG": make_pointer_sized(),
    "PSIZE_T": make_pointer_sized(), "PSSIZE_T": make_pointer_sized(),
    "PLARGE_INTEGER": make_pointer_sized(), "PULARGE_INTEGER": make_pointer_sized(),
    "PGUID": make_pointer_sized(), "LPGUID": make_pointer_sized(),
    "PLIST_ENTRY": make_pointer_sized(),
    "PHANDLE": make_pointer_sized(),
    "PACCESS_MASK": make_pointer_sized(),
    "PNTSTATUS": make_pointer_sized(),
    # well-known composite types, hand-verified against real Windows layouts
    # (kept out of phnt itself since they normally come from the real SDK).
    "GUID": TypeInfo(16, 4, 16, 4),
    "IID": TypeInfo(16, 4, 16, 4),
    "CLSID": TypeInfo(16, 4, 16, 4),
    "LUID": TypeInfo(8, 4, 8, 4),
    "LARGE_INTEGER": TypeInfo(8, 8, 8, 8),
    "ULARGE_INTEGER": TypeInfo(8, 8, 8, 8),
    "LIST_ENTRY": TypeInfo(8, 4, 16, 8),
    "SINGLE_LIST_ENTRY": TypeInfo(4, 4, 8, 8),
    "SLIST_ENTRY": TypeInfo(4, 4, 8, 8),
}

# Base-type JSON (Fission's own base_types.json) fills in/overrides on load.


def load_base_types(path: str) -> None:
    with open(path, encoding="utf-8") as f:
        items = json.load(f)
    for item in items:
        PRIMITIVES[item["name"]] = TypeInfo(
            item["size_32"], item["size_32"], item["size_64"], item["size_64"],
            is_pointer=item.get("is_pointer", False),
        )


# ─── Comment / preprocessor stripping ─────────────────────────────────────────

def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    return text


    # Matches any SAL annotation token: leading '_', one or more
    # underscore-separated word segments, trailing '_' (e.g. _In_,
    # _Out_opt_, _Field_size_bytes_part_opt_, _When_), optionally followed
    # by a parenthesized argument list.
_SAL_RE = re.compile(r"_[A-Za-z0-9]+(?:_[A-Za-z0-9]+)*_(\s*\([^()]*\))?")


def strip_sal(text: str) -> str:
    return _SAL_RE.sub(" ", text)


# ─── Struct/union body extraction (brace matching) ─────────────────────────────

_STRUCT_START_RE = re.compile(r"typedef\s+(struct|union)\s*(_?\w+)?\s*\{")


def find_top_level_struct_blocks(text: str) -> List[Tuple[str, str, str]]:
    """Return (kind, body_without_braces, trailer_up_to_semicolon) tuples for
    every top-level `typedef struct/union { ... } Name1, *PName1, ...;`."""
    blocks = []
    i = 0
    n = len(text)
    for m in _STRUCT_START_RE.finditer(text):
        if m.start() < i:
            continue  # inside a block we already consumed (shouldn't happen at top level)
        kind = m.group(1)
        start = m.end() - 1  # index of the opening '{'
        depth = 0
        j = start
        while j < n:
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        if j >= n:
            continue
        body = text[start + 1:j]
        semi = text.find(";", j)
        if semi == -1:
            continue
        trailer = text[j + 1:semi]
        blocks.append((kind, body, trailer))
        i = semi + 1
    return blocks


def extract_typedef_names(trailer: str) -> List[str]:
    """From ` Name1, *PName1, Name2` extract the non-pointer names."""
    names = []
    for part in trailer.split(","):
        part = part.strip()
        if not part or part.startswith("*"):
            continue
        part = re.sub(r"\[.*?\]", "", part).strip()
        if re.match(r"^[A-Za-z_]\w*$", part):
            names.append(part)
    return names


def extract_pointer_typedef_names(trailer: str) -> List[str]:
    names = []
    for part in trailer.split(","):
        part = part.strip()
        if part.startswith("*"):
            name = part.lstrip("*").strip()
            if re.match(r"^[A-Za-z_]\w*$", name):
                names.append(name)
    return names


# ─── Top-level statement splitting (semicolon-delimited, brace-depth-aware) ────

def split_top_level_statements(body: str) -> List[str]:
    stmts = []
    depth = 0
    cur = []
    for ch in body:
        if ch == "{":
            depth += 1
            cur.append(ch)
        elif ch == "}":
            depth -= 1
            cur.append(ch)
        elif ch == ";" and depth == 0:
            stmt = "".join(cur).strip()
            if stmt:
                stmts.append(stmt)
            cur = []
        else:
            cur.append(ch)
    tail = "".join(cur).strip()
    if tail:
        stmts.append(tail)
    return stmts


# ─── Field parsing ──────────────────────────────────────────────────────────

_BITFIELD_RE = re.compile(r"^([A-Za-z_][\w\s\*]*?)\s+(\w+)?\s*:\s*(\d+)\s*$")
_FUNCPTR_RE = re.compile(r"\(\s*[A-Za-z_]*\s*\*\s*(\w+)\s*\)\s*\(")
_ARRAY_RE = re.compile(r"^(\w+)\s*((?:\[[^\]]*\])+)$")


class Layout:
    __slots__ = ("fields32", "fields64", "size32", "align32", "size64", "align64")

    def __init__(self):
        self.fields32 = []  # (name, type_name, offset, size)
        self.fields64 = []
        self.size32 = 0
        self.align32 = 1
        self.size64 = 0
        self.align64 = 1


def round_up(x: int, a: int) -> int:
    if a <= 1:
        return x
    return (x + a - 1) // a * a


def parse_declarator_list(base_type_tokens: str, decl_part: str):
    """Split 'Type A, *B, C[4]' style comma lists (base type already
    separated out) into (name, extra_pointer_depth, array_dims) tuples."""
    results = []
    for raw in split_commas_top_level(decl_part):
        raw = raw.strip()
        if not raw:
            continue
        stars = 0
        while raw.startswith("*"):
            stars += 1
            raw = raw[1:].lstrip()
        arr_m = _ARRAY_RE.match(raw.replace(" ", ""))
        dims = []
        name = raw
        if arr_m:
            name = arr_m.group(1)
            for d in re.findall(r"\[([^\]]*)\]", arr_m.group(2)):
                dims.append(d.strip())
        name = name.strip()
        if not re.match(r"^[A-Za-z_]\w*$", name):
            return None
        results.append((name, stars, dims))
    return results


def split_commas_top_level(s: str) -> List[str]:
    parts = []
    depth = 0
    cur = []
    for ch in s:
        if ch in "([":
            depth += 1
            cur.append(ch)
        elif ch in ")]":
            depth -= 1
            cur.append(ch)
        elif ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    parts.append("".join(cur))
    return parts


_QUALIFIERS = {"CONST", "const", "volatile", "FAR", "NEAR", "WINAPI", "NTAPI",
               "CALLBACK", "APIENTRY", "_declspec(align(8))"}


def clean_tokens(text: str) -> str:
    text = strip_sal(text)
    for q in ("CONST", "const", "volatile"):
        text = re.sub(rf"\b{q}\b", " ", text)
    return text


def compute_layout(body: str, type_table: Dict[str, TypeInfo],
                    struct_layouts: Dict[str, Layout], is_union: bool) -> Optional[Layout]:
    stmts = split_top_level_statements(body)
    layout32_offset = 0
    layout64_offset = 0
    max_align32 = 1
    max_align64 = 1
    fields32 = []
    fields64 = []

    # bitfield run state: (underlying_type_name, unit_start32, unit_start64)
    bitfield_run_type = None

    for raw_stmt in stmts:
        stmt = clean_tokens(raw_stmt).strip()
        if not stmt:
            continue
        if stmt.startswith("#"):
            return None  # shouldn't happen (handled earlier) but be safe

        # nested anonymous/named struct or union
        nested_m = re.match(r"^(struct|union)\s*(_?\w+)?\s*\{", stmt)
        if nested_m:
            depth = 0
            j = stmt.index("{")
            k = j
            while k < len(stmt):
                if stmt[k] == "{":
                    depth += 1
                elif stmt[k] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            if k >= len(stmt):
                return None
            inner_body = stmt[j + 1:k]
            trailer = stmt[k + 1:].strip()
            sub_is_union = nested_m.group(1) == "union"
            sub = compute_layout(inner_body, type_table, struct_layouts, sub_is_union)
            if sub is None:
                return None
            field_name = trailer.strip() if trailer else None
            base32 = 0 if is_union else round_up(layout32_offset, sub.align32)
            base64 = 0 if is_union else round_up(layout64_offset, sub.align64)
            if field_name:
                fields32.append((field_name, "struct", base32, sub.size32))
                fields64.append((field_name, "struct", base64, sub.size64))
            else:
                # true anonymous member: flatten directly
                for name, tn, off, sz in sub.fields32:
                    fields32.append((name, tn, base32 + off, sz))
                for name, tn, off, sz in sub.fields64:
                    fields64.append((name, tn, base64 + off, sz))
            end32 = base32 + sub.size32
            end64 = base64 + sub.size64
            max_align32 = max(max_align32, sub.align32)
            max_align64 = max(max_align64, sub.align64)
            if is_union:
                layout32_offset = max(layout32_offset, end32)
                layout64_offset = max(layout64_offset, end64)
            else:
                layout32_offset = end32
                layout64_offset = end64
            bitfield_run_type = None
            continue

        # function pointer field
        fp_m = _FUNCPTR_RE.search(stmt)
        if fp_m:
            name = fp_m.group(1)
            ti = type_table.get("PVOID")
            base32 = 0 if is_union else round_up(layout32_offset, ti.align32)
            base64 = 0 if is_union else round_up(layout64_offset, ti.align64)
            fields32.append((name, "funcptr", base32, ti.size32))
            fields64.append((name, "funcptr", base64, ti.size64))
            end32, end64 = base32 + ti.size32, base64 + ti.size64
            max_align32 = max(max_align32, ti.align32)
            max_align64 = max(max_align64, ti.align64)
            if is_union:
                layout32_offset = max(layout32_offset, end32)
                layout64_offset = max(layout64_offset, end64)
            else:
                layout32_offset = end32
                layout64_offset = end64
            bitfield_run_type = None
            continue

        # bitfield
        bf_m = _BITFIELD_RE.match(stmt)
        if bf_m:
            base_type = bf_m.group(1).strip()
            ti = type_table.get(base_type)
            if ti is None:
                return None
            if bitfield_run_type != base_type:
                # start a new storage unit
                base32 = 0 if is_union else round_up(layout32_offset, ti.align32)
                base64 = 0 if is_union else round_up(layout64_offset, ti.align64)
                name = bf_m.group(2) or "_bitfield"
                fields32.append((name, base_type, base32, ti.size32))
                fields64.append((name, base_type, base64, ti.size64))
                end32, end64 = base32 + ti.size32, base64 + ti.size64
                if is_union:
                    layout32_offset = max(layout32_offset, end32)
                    layout64_offset = max(layout64_offset, end64)
                else:
                    layout32_offset = end32
                    layout64_offset = end64
                max_align32 = max(max_align32, ti.align32)
                max_align64 = max(max_align64, ti.align64)
                bitfield_run_type = base_type
            continue

        bitfield_run_type = None

        # regular field(s): split base type from declarator list
        m = re.match(r"^([A-Za-z_][\w\s]*?)\s+([\*\w].*)$", stmt)
        if not m:
            return None
        base_type_name = re.sub(r"\s+", " ", m.group(1)).strip()
        decl_part = m.group(2).strip()
        if base_type_name in ("struct", "union", "enum"):
            # "struct TAG Field;" referencing an already-typedef'd tag -- skip,
            # too ambiguous without full tag tracking.
            return None

        decls = parse_declarator_list(base_type_name, decl_part)
        if decls is None:
            return None

        for name, stars, dims in decls:
            if stars > 0:
                ti = type_table.get("PVOID")
                type_name = base_type_name + " *" * stars
            else:
                ti = type_table.get(base_type_name)
                type_name = base_type_name
            if ti is None:
                return None

            elem_size32, elem_size64 = ti.size32, ti.size64
            elem_align32, elem_align64 = ti.align32, ti.align64

            count = 1
            flexible = False
            for d in dims:
                if d == "":
                    flexible = True
                    continue
                try:
                    count *= int(d, 0)
                except ValueError:
                    flexible = True

            if flexible:
                field_size32 = 0
                field_size64 = 0
            else:
                field_size32 = elem_size32 * count
                field_size64 = elem_size64 * count

            base32 = 0 if is_union else round_up(layout32_offset, elem_align32)
            base64 = 0 if is_union else round_up(layout64_offset, elem_align64)
            fields32.append((name, type_name, base32, field_size32))
            fields64.append((name, type_name, base64, field_size64))
            end32 = base32 + field_size32
            end64 = base64 + field_size64
            max_align32 = max(max_align32, elem_align32)
            max_align64 = max(max_align64, elem_align64)
            if is_union:
                layout32_offset = max(layout32_offset, end32)
                layout64_offset = max(layout64_offset, end64)
            else:
                layout32_offset = end32
                layout64_offset = end64

    result = Layout()
    result.fields32 = fields32
    result.fields64 = fields64
    result.align32 = max_align32
    result.align64 = max_align64
    result.size32 = round_up(layout32_offset, max_align32)
    result.size64 = round_up(layout64_offset, max_align64)
    return result


# ─── Driver: multi-pass resolution across all structs ──────────────────────────

def main():
    ap = argparse.ArgumentParser(description="Compute struct layouts from phnt C headers")
    ap.add_argument("phnt_include_dir")
    ap.add_argument("base_types_json")
    ap.add_argument("--output", "-o", default="-")
    ap.add_argument("--verbose", "-v", action="store_true")
    args = ap.parse_args()

    load_base_types(args.base_types_json)
    type_table: Dict[str, TypeInfo] = dict(PRIMITIVES)

    # Gather all struct/union blocks + their names, and simple typedef aliases.
    pending: List[Tuple[List[str], List[str], str, bool]] = []  # (names, ptr_names, body, is_union)
    alias_re = re.compile(r"^typedef\s+(?:const\s+)?([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*;\s*$")
    ptr_alias_re = re.compile(r"^typedef\s+(?:const\s+)?([A-Za-z_]\w*)\s*\*\s*([A-Za-z_]\w*)\s*;\s*$")

    for path in sorted(glob.glob(f"{args.phnt_include_dir}/*.h")):
        text = strip_comments(open(path, encoding="utf-8", errors="replace").read())
        for kind, body, trailer in find_top_level_struct_blocks(text):
            names = extract_typedef_names(trailer)
            ptr_names = extract_pointer_typedef_names(trailer)
            # A pointer to this type is ptr-sized regardless of whether the
            # pointee's own layout is computable, so register it even for
            # bodies we're about to skip (e.g. version-conditional #if
            # blocks like TEB/PEB) -- otherwise every OTHER struct with a
            # `PThisType` field transitively fails to resolve too.
            for pname in ptr_names:
                type_table.setdefault(pname, PRIMITIVES["PVOID"])
            if re.search(r"#\s*if", body):
                continue  # version-conditional layout -- skip, don't guess
            if not names:
                continue
            pending.append((names, ptr_names, body, kind == "union"))

        for line in text.splitlines():
            line_s = line.strip()
            m = alias_re.match(line_s)
            if m:
                target, alias = m.group(1), m.group(2)
                if alias not in type_table and target in type_table:
                    type_table[alias] = type_table[target]
                continue
            pm = ptr_alias_re.match(line_s)
            if pm:
                _target, alias = pm.group(1), pm.group(2)
                type_table.setdefault(alias, PRIMITIVES["PVOID"])

    if args.verbose:
        print(f"[+] {len(pending)} struct/union blocks (post #if filter)", file=sys.stderr)

    resolved: Dict[str, Layout] = {}
    remaining = list(pending)
    for _round in range(30):
        progressed = False
        still_remaining = []
        for names, ptr_names, body, is_union in remaining:
            if names[0] in resolved:
                continue
            layout = compute_layout(body, type_table, resolved, is_union)
            if layout is None:
                still_remaining.append((names, ptr_names, body, is_union))
                continue
            for n in names:
                resolved[n] = layout
                type_table[n] = TypeInfo(layout.size32, layout.align32, layout.size64, layout.align64)
            progressed = True
        remaining = still_remaining
        if not progressed:
            break

    if args.verbose:
        print(f"[+] resolved {len(resolved)} structs, {len(remaining)} unresolved after fixed point",
              file=sys.stderr)

    out = []
    seen_names = set()
    for names, _ptr_names, _body, _u in pending:
        primary = names[0]
        if primary in seen_names or primary not in resolved:
            continue
        # WOW64 thunk structs (e.g. UNICODE_STRING64, LIST_ENTRY32) always use
        # a FIXED pointer width by design regardless of target bitness -- our
        # native-pointer-width layout math doesn't model that, so skip rather
        # than emit a wrong dual-width size for this specific family.
        if re.search(r"(32|64)$", primary) and primary not in ("KAFFINITY",):
            continue
        seen_names.add(primary)
        layout = resolved[primary]
        if layout.size64 == 0 or not layout.fields64:
            continue
        fields = []
        seen_field_names = set()
        for i in range(len(layout.fields64)):
            name64, _tn64, off64, sz64 = layout.fields64[i]
            name32, _tn32, off32, sz32 = layout.fields32[i]
            fname = name64
            suffix = 2
            while fname in seen_field_names:
                fname = f"{name64}_{suffix}"
                suffix += 1
            seen_field_names.add(fname)
            fields.append({
                "name": fname,
                "type_name": _tn64,
                "offset_32": off32,
                "offset_64": off64,
                "size_32": sz32,
                "size_64": sz64,
            })
        out.append({
            "name": primary,
            "size_32": layout.size32,
            "size_64": layout.size64,
            "fields": fields,
        })

    out.sort(key=lambda s: s["name"])
    out_text = json.dumps(out, indent=2)
    if args.output == "-":
        print(out_text)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out_text)
        print(f"Wrote {len(out)} structs -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
