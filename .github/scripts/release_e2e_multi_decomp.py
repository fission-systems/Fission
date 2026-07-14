#!/usr/bin/env python3
"""Decompile several functions from a fission_cli list --json payload (release E2E)."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--list-json", required=True)
    ap.add_argument("--max-functions", type=int, default=5)
    ap.add_argument("--timeout-ms", type=int, default=180_000)
    args = ap.parse_args()

    raw = Path(args.list_json).read_text(encoding="utf-8")
    data = json.loads(raw)
    items = data if isinstance(data, list) else data.get("functions") or data.get("items") or []
    if not items:
        print("error: empty function list", file=sys.stderr)
        return 1

    # Prefer real code symbols; skip empty names.
    candidates: list[dict] = []
    for f in items:
        if not isinstance(f, dict):
            continue
        name = (f.get("name") or "").strip()
        addr = f.get("address") or f.get("addr")
        if not addr:
            continue
        if f.get("is_import") or f.get("is_thunk_like"):
            continue
        candidates.append({"name": name or "?", "address": str(addr)})

    if not candidates:
        print("error: no decompilable candidates in list", file=sys.stderr)
        return 1

    selected = candidates[: max(1, args.max_functions)]
    print(f"decompiling {len(selected)} function(s)")
    for entry in selected:
        addr = entry["address"]
        print(f"  - {entry['name']} @ {addr}")
        cmd = [
            args.cli,
            "decomp",
            args.binary,
            "--addr",
            addr,
            "--profile",
            "speed",
            "--timeout-ms",
            str(args.timeout_ms),
            "--json",
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if proc.returncode != 0:
            print(proc.stdout, file=sys.stderr)
            print(proc.stderr, file=sys.stderr)
            print(f"error: decomp failed for {addr}", file=sys.stderr)
            return 1
        check = subprocess.run(
            [sys.executable, str(Path(__file__).with_name("check_decomp_output.py"))],
            input=proc.stdout,
            text=True,
            check=False,
        )
        if check.returncode != 0:
            print(f"error: decomp JSON invalid for {addr}", file=sys.stderr)
            return 1
    print("multi_decomp_ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
