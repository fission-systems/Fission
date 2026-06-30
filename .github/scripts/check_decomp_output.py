#!/usr/bin/env python3
"""Validate fission_cli decomp --json output from stdin."""
import json, sys

raw = sys.stdin.read()
try:
    r = json.loads(raw)
except json.JSONDecodeError as e:
    sys.stderr.write(f"decomp output is not valid JSON: {e}\n{raw[:200]}\n")
    sys.exit(1)

entry = r[0] if isinstance(r, list) else r
if not isinstance(entry, dict):
    sys.stderr.write(f"unexpected output shape: {raw[:200]}\n")
    sys.exit(1)

if entry.get("error"):
    sys.stderr.write(f"decomp error:\n{entry['error']}\n")
    sys.exit(1)

code = entry.get("code", "")
if not isinstance(code, str) or not code.strip():
    sys.stderr.write(f"decomp returned empty code: {entry}\n")
    sys.exit(1)

print(f"decomp_ok  name={entry.get('name','?')}  code_len={len(code)}")
