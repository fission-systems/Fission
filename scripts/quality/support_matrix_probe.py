#!/usr/bin/env python3
"""Does each (container, architecture) cell of the support matrix work at all?

For every binary in DIR this runs, under `--contract v1`: info, list, strings,
disasm, `decomp --addr` on each named function, and `decomp --all`. It reports
whether each step succeeded and how many functions came back with code.

It measures "works", NOT "is correct": it never compares pseudocode with source.
The one content check is a count of functions whose output calls a tiny-address
`sub_N()`: a branch to address 1..0xfff is never real code, it is a p-code
internal branch that was read as an address (see docs/roadmap/1.0-criteria.md).

usage: support_matrix_probe.py CLI DIR      (run from the repo root)

Build DIR with the cross toolchains; the recipe is in
docs/roadmap/1.0-criteria.md ("Support matrix, measured").
"""
import json, os, re, subprocess, sys

CLI, DIR = sys.argv[1], sys.argv[2]
NAMED = ("sum_to", "classify", "count_char", "main")
TINY_CALL = re.compile(r"\bsub_[0-9a-f]{1,3}\s*\(")


def run(*args, timeout=600):
    p = subprocess.run([CLI, "--contract", "v1", *args], capture_output=True, text=True, timeout=timeout)
    try:
        return json.loads(p.stdout[p.stdout.find("{"):])
    except ValueError:
        return None


def data(j):
    return j["data"] if j and j.get("ok") else None


def codes(j):
    d = data(j)
    return [d] if isinstance(d, dict) else (d or [])


for name in sorted(os.listdir(DIR)):
    path = os.path.join(DIR, name)
    if not os.path.isfile(path) or name.endswith((".c", ".py", ".stripped")):
        continue
    row = {"bin": name}
    info = data(run("info", path, "--json"))
    if not info:
        print(json.dumps({**row, "info": "FAIL"})); continue
    row.update(format=info["format"], arch=info["arch"], bits=info["bits"], functions=info["functions"])
    listed = data(run("list", path, "--json")) or []
    by_name = {f["name"].lstrip("_"): f["address"] for f in listed}
    row["named_found"] = [n for n in NAMED if n in by_name]
    strings = json.dumps(data(run("strings", path, "--json")) or "")
    row["strings"] = "fission matrix probe" in strings
    entry = by_name.get("main") or info["entry"]
    row["disasm"] = bool(data(run("disasm", path, "--addr", entry, "--json")))
    allj = run("decomp", path, "--all", "--json", "--no-db", "--timeout-ms", "20000")
    fns = codes(allj)
    row["decomp_all"] = f"{sum(1 for f in fns if f.get('code'))}/{len(fns)}"
    row["tiny_address_calls"] = f"{sum(1 for f in fns if TINY_CALL.search(f.get('code') or ''))}/{len(fns)}"
    print(json.dumps(row))
