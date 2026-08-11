#!/usr/bin/env python3
"""
angr_wdk_extract_signatures.py
───────────────────────────────
Extract Windows kernel-mode (WDK) API signatures from angr's vendored
`procedures/definitions/wdk/*.json` prototype database and emit them in
Fission's pipe-delimited format:

    FunctionName|ReturnType|param0:Type0,param1:Type1,...

Fission's existing ntoskrnl_signatures.txt only covers ~340 hand-curated
ntoskrnl functions; angr ships JSON-derived prototypes for ntoskrnl (1713
functions) plus 13 other WDK libraries Fission has zero coverage for
(fltmgr, ndis, gdi32, hal, ksecdd, clfs, fwpkclnt, fwpuclnt, offreg, pshed,
secur32, vhfum, api-ms-win-dx-d3dkmt).

Usage:
    python3 scripts/angr_wdk_extract_signatures.py \
        vendor/angr-master/angr/procedures/definitions/wdk \
        --skip-names utils/signatures/typeinfo/win32/ntoskrnl_signatures.txt \
                     utils/signatures/typeinfo/win32/win_api_signatures.txt \
        --output utils/signatures/typeinfo/win32/wdk_signatures.txt

Type format note: each JSON file's "proto" field is a Python repr() string
(single-quoted dict literal) that mixes in raw JSON booleans/null (`false`/
`true`/`null` instead of `False`/`True`/`None`), so it is neither valid JSON
nor valid Python -- ast.literal_eval() rejects the bare `false`/`true`/`null`
identifiers. We regex-substitute them to their Python spellings first, then
literal_eval. Type nodes are Fission's existing convention has "type names
are informational only" (see ntoskrnl_signatures.txt header) -- arity is
authoritative -- so we render pointer chains as "Inner *" and use the WDK's
own typedef name (`_ref`'s `name` field, e.g. NTSTATUS/HANDLE/PDEVICE_OBJECT)
wherever the JSON gives us one, which is the most readable option available.
"""

import argparse
import ast
import glob
import os
import re
import sys
from typing import Dict, List, Optional, Set, Tuple

_BOOL_NULL_RE = re.compile(r"\b(false|true|null)\b")
_BOOL_NULL_MAP = {"false": "False", "true": "True", "null": "None"}
_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def fix_literal(s: str) -> str:
    return _BOOL_NULL_RE.sub(lambda m: _BOOL_NULL_MAP[m.group(1)], s)


def type_to_str(t: Optional[dict], depth: int = 0) -> str:
    if not isinstance(t, dict) or depth > 12:
        return "void"
    tt = t.get("_t")
    if tt == "ptr":
        return f"{type_to_str(t.get('pts_to'), depth + 1)} *"
    if tt == "_ref":
        name = t.get("name")
        if name and _NAME_RE.match(name):
            return name
        return type_to_str({"_t": t.get("ot")}, depth + 1) if t.get("ot") else "void"
    if tt == "bot":
        return "void"
    if tt == "int":
        return t.get("label") or ("unsigned int" if t.get("signed") is False else "int")
    if tt == "char":
        return t.get("label") or ("unsigned char" if t.get("signed") is False else "char")
    if tt == "short":
        return t.get("label") or ("unsigned short" if t.get("signed") is False else "short")
    if tt == "llong":
        return t.get("label") or ("unsigned long long" if t.get("signed") is False else "long long")
    if tt == "union":
        name = t.get("name")
        return name if name and name != "<anon>" and _NAME_RE.match(name) else "ULONG"
    return "void"


def extract_library(json_path: str) -> List[Tuple[str, str]]:
    import json

    with open(json_path, encoding="utf-8") as f:
        data = json.load(f)

    results: List[Tuple[str, str]] = []
    for name, entry in data.get("functions", {}).items():
        if not _NAME_RE.match(name):
            continue
        proto_str = fix_literal(entry["proto"])
        try:
            proto = ast.literal_eval(proto_str)
        except (ValueError, SyntaxError):
            continue
        if not isinstance(proto, dict) or proto.get("_t") != "func":
            continue

        ret_type = type_to_str(proto.get("returnty"))
        args = proto.get("args") or []
        arg_names = proto.get("arg_names") or []

        if not args:
            results.append((name, f"{name}|{ret_type}|void"))
            continue

        params = []
        seen_param_names: Set[str] = set()
        for i, arg in enumerate(args):
            raw_pname = arg_names[i] if i < len(arg_names) and arg_names[i] else f"arg{i}"
            pname = re.sub(r"[^A-Za-z0-9_]", "_", raw_pname) or f"arg{i}"
            if not pname[0].isalpha() and pname[0] != "_":
                pname = f"a_{pname}"
            while pname in seen_param_names:
                pname = f"{pname}_"
            seen_param_names.add(pname)
            params.append(f"{pname}:{type_to_str(arg)}")
        results.append((name, f"{name}|{ret_type}|{','.join(params)}"))

    return results


def load_existing_names(paths: List[str]) -> Set[str]:
    names: Set[str] = set()
    for path in paths:
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                name = line.split("|", 1)[0].strip()
                if name:
                    names.add(name)
    return names


def main():
    ap = argparse.ArgumentParser(
        description="Extract WDK kernel API signatures from angr's vendored definitions JSON"
    )
    ap.add_argument("wdk_dir", help="Path to angr's procedures/definitions/wdk directory")
    ap.add_argument("--skip-names", nargs="*", default=[],
                     help="Existing signature .txt files whose names should NOT be duplicated")
    ap.add_argument("--output", "-o", default="-", help="Output .txt path (default: stdout)")
    ap.add_argument("--verbose", "-v", action="store_true")
    args = ap.parse_args()

    existing = load_existing_names(args.skip_names)
    if args.verbose:
        print(f"[+] {len(existing)} existing names to skip", file=sys.stderr)

    by_library: Dict[str, List[Tuple[str, str]]] = {}
    seen_this_run: Set[str] = set()
    for json_path in sorted(glob.glob(os.path.join(args.wdk_dir, "*.json"))):
        lib = os.path.splitext(os.path.basename(json_path))[0]
        entries = extract_library(json_path)
        kept = []
        for name, line in entries:
            if name in existing or name in seen_this_run:
                continue
            seen_this_run.add(name)
            kept.append((name, line))
        if kept:
            by_library[lib] = sorted(kept)
        if args.verbose:
            print(f"  [{lib}] {len(entries)} parsed, {len(kept)} new", file=sys.stderr)

    lines = [
        "# Windows Driver Kit (WDK) kernel-mode API signatures",
        "# Source: angr's vendored procedures/definitions/wdk/*.json (extracted via scripts/angr_wdk_extract_signatures.py)",
        "# Format: name|return_type|param_name:type,...  (void = no params)",
        "# Arity is authoritative; type names are informational only.",
        "",
    ]
    total = 0
    for lib in sorted(by_library):
        entries = by_library[lib]
        lines.append(f"# ── {lib} ({len(entries)}) ─────────────────────────────────────────")
        for _name, line in entries:
            lines.append(line)
            total += 1
        lines.append("")

    out_text = "\n".join(lines).rstrip() + "\n"
    if args.output == "-":
        print(out_text)
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out_text)
        print(f"Wrote {total} new signatures across {len(by_library)} libraries → {args.output}",
              file=sys.stderr)


if __name__ == "__main__":
    main()
