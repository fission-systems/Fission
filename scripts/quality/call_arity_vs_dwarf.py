#!/usr/bin/env python3
"""Call-site arity vs DWARF, on stripped input.

Ground truth: llvm-dwarfdump on the UNSTRIPPED binary (named subprograms with a
low_pc; parameters = direct DW_TAG_formal_parameter children; variadic and
unprototyped functions are excluded). Subject: fission_cli decomp --all on the
STRIPPED copy, so no debug info reaches the decompiler. Calls are matched by
address (`sub_<hex>(`), so names never leak.

usage: call_arity_vs_dwarf.py CLI OUTDIR BINARY...   (run from the repo root: sleigh specs are relative;
LLVM_BIN names the directory holding llvm-dwarfdump and llvm-strip when they are not on PATH)
"""
import json, os, re, subprocess, sys, collections

import shutil

LLVM = os.environ.get("LLVM_BIN") or os.path.dirname(shutil.which("llvm-dwarfdump") or "/opt/homebrew/opt/llvm/bin/llvm-dwarfdump")


def dwarf_truth(path):
    out = subprocess.run([f"{LLVM}/llvm-dwarfdump", "--verbose", "--debug-info", path],
                         capture_output=True, text=True).stdout
    funcs = {}
    cur = None
    for line in out.splitlines():
        m = re.match(r"^0x[0-9a-f]+:( +)DW_TAG_(\w+)", line)
        if m:
            depth = (len(m.group(1)) - 1) // 2
            tag = m.group(2)
            if tag == "subprogram" and depth == 1:
                cur = {"name": None, "low": None, "high": None, "params": 0,
                       "variadic": False, "proto": False, "decl": False}
                funcs[id(cur)] = cur
            elif cur is not None and depth == 2 and tag == "formal_parameter":
                cur["params"] += 1
            elif cur is not None and depth == 2 and tag == "unspecified_parameters":
                cur["variadic"] = True
            elif depth <= 1:
                cur = None if tag != "subprogram" else cur
            continue
        if cur is not None:
            m = re.match(r"^\s+DW_AT_(\w+)\s+(?:\[DW_FORM_(\w+)\]\s*)?\((.*)\)\s*$", line)
            if not m:
                continue
            k, form, v = m.group(1), m.group(2), m.group(3)
            if k == "name" and cur["name"] is None:
                q = re.search(r'"([^"]*)"\s*$', v)
                cur["name"] = q.group(1) if q else v.strip('"')
            elif k == "low_pc":
                cur["low"] = int(v.split()[0], 16)
            elif k == "high_pc":
                cur["high"] = int(v.split()[0], 16)
                cur["high_is_addr"] = (form == "addr")
            elif k == "prototyped":
                cur["proto"] = True
            elif k == "declaration":
                cur["decl"] = True
    by_addr = {}
    for f in funcs.values():
        if f["low"] is None or f["name"] is None or f["decl"]:
            continue
        size = (f["high"] - f["low"]) if f.get("high_is_addr") else (f["high"] or 0)
        by_addr[f["low"]] = {"name": f["name"], "params": f["params"], "variadic": f["variadic"],
                             "proto": f["proto"], "size": size}
    return by_addr


def split_args(s):
    """Top-level comma split of the text between the outer parens."""
    args, depth, cur, i = [], 0, "", 0
    in_s = in_c = False
    while i < len(s):
        c = s[i]
        if in_s:
            cur += c
            if c == "\\":
                cur += s[i + 1]; i += 1
            elif c == '"':
                in_s = False
        elif in_c:
            cur += c
            if c == "\\":
                cur += s[i + 1]; i += 1
            elif c == "'":
                in_c = False
        elif c == '"':
            in_s = True; cur += c
        elif c == "'":
            in_c = True; cur += c
        elif c in "([{":
            depth += 1; cur += c
        elif c in ")]}":
            depth -= 1; cur += c
        elif c == "," and depth == 0:
            args.append(cur); cur = ""
        else:
            cur += c
        i += 1
    if cur.strip() or args:
        args.append(cur)
    return [a for a in args if a.strip()] if not (len(args) == 1 and not args[0].strip()) else []


def calls_in(code, skip_addr_hex):
    body_start = code.find("{")
    body = code[body_start:] if body_start >= 0 else ""
    res = []
    for m in re.finditer(r"\bsub_([0-9a-f]+)\s*\(", body):
        j = m.end()
        depth, k = 1, j
        in_s = False
        while k < len(body) and depth:
            c = body[k]
            if in_s:
                if c == "\\": k += 1
                elif c == '"': in_s = False
            elif c == '"': in_s = True
            elif c == "(": depth += 1
            elif c == ")": depth -= 1
            k += 1
        if depth:
            continue
        inner = body[j:k - 1]
        res.append((int(m.group(1), 16), len(split_args(inner))))
    return res


def signature_arity(code, name):
    m = re.search(r"\b" + re.escape(name) + r"\s*\(([^;{]*)\)\s*\{", code)
    if not m:
        return None
    inner = m.group(1).strip()
    if inner in ("", "void"):
        return 0
    return len(split_args(inner))


def main():
    cli, outdir, *bins = sys.argv[1:]
    os.makedirs(outdir, exist_ok=True)
    tally = collections.Counter()
    rows = []
    for b in bins:
        tag = b.replace("/", "_")
        stripped = os.path.join(outdir, tag + ".stripped")
        subprocess.run([f"{LLVM}/llvm-strip", "--strip-all", "-o", stripped, b], check=True)
        truth = dwarf_truth(b)
        env = dict(os.environ, FISSION_BENCHMARK_NO_CACHE="1")
        p = subprocess.run([cli, "decomp", stripped, "--all", "--json", "--no-db", "--timeout-ms", "20000"],
                           capture_output=True, env=env)
        try:
            funcs = json.loads(p.stdout)
        except Exception:
            print("FAILED", b, p.stderr[-200:]); continue
        funcs = [f for f in funcs if "code" in f]
        n_call = n_ok = 0
        for f in funcs:
            for addr, got in calls_in(f["code"], f["address"]):
                t = truth.get(addr)
                if not t or t["variadic"] or not t["proto"]:
                    continue
                kind = "exact" if got == t["params"] else ("under" if got < t["params"] else "over")
                size_b = "<=64" if t["size"] <= 64 else ("<=512" if t["size"] <= 512 else ">512")
                tally[("call", kind, size_b)] += 1
                tally[("call", kind, "all")] += 1
                rows.append({"bin": b, "caller": f["address"], "callee": hex(addr), "name": t["name"],
                             "want": t["params"], "got": got, "kind": kind, "size": t["size"]})
        for f in funcs:
            a = int(f["address"], 16) if isinstance(f["address"], str) else f["address"]
            t = truth.get(a)
            if not t or t["variadic"] or not t["proto"]:
                continue
            got = signature_arity(f["code"], f["name"])
            if got is None:
                continue
            kind = "exact" if got == t["params"] else ("under" if got < t["params"] else "over")
            tally[("sig", kind, "all")] += 1
            rows.append({"bin": b, "sig": True, "callee": hex(a), "name": t["name"], "want": t["params"],
                         "got": got, "kind": kind, "size": t["size"]})
    json.dump(rows, open(os.path.join(outdir, "rows.json"), "w"))
    for what in ("call", "sig"):
        tot = sum(v for (w, k, s), v in tally.items() if w == what and s == "all")
        if not tot:
            continue
        line = {k: tally[(what, k, "all")] for k in ("exact", "under", "over")}
        print(f"{what}: n={tot} " + " ".join(f"{k}={v} ({100*v/tot:.1f}%)" for k, v in line.items()))
    for s in ("<=64", "<=512", ">512"):
        tot = sum(tally[("call", k, s)] for k in ("exact", "under", "over"))
        if tot:
            print(f"  call callee size {s}: n={tot} " + " ".join(
                f"{k}={tally[('call', k, s)]} ({100*tally[('call', k, s)]/tot:.1f}%)" for k in ("exact", "under", "over")))


main()
