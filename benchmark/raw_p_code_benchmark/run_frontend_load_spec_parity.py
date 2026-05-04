#!/usr/bin/env python3
"""Run load_spec vs entry-id frontend parity on canonical raw-P-code manifest rows."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=THIS_DIR / "canonical_rows.json",
        help="canonical_rows.json (repo-relative paths inside rows)",
    )
    parser.add_argument(
        "--only",
        action="append",
        dest="only_names",
        metavar="ROW_NAME",
        help="run only rows whose name matches (repeatable); default: test-functions-add",
    )
    parser.add_argument(
        "--cargo-profile",
        choices=("debug", "release"),
        default="release",
        help="cargo profile for fission-sleigh example",
    )
    return parser.parse_args()


def row_matches(names: list[str] | None, row_name: str) -> bool:
    if not names:
        return row_name == "test-functions-add"
    return row_name in names


def main() -> None:
    args = parse_args()
    manifest_path = args.manifest
    if not manifest_path.is_file():
        print(f"error: manifest not found: {manifest_path}", file=sys.stderr)
        sys.exit(1)

    payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    rows = payload.get("rows", [])
    if not isinstance(rows, list):
        print("error: manifest rows must be a list", file=sys.stderr)
        sys.exit(1)

    cargo_profile_flag = [] if args.cargo_profile == "debug" else ["--release"]

    ran = 0
    for row in rows:
        if not isinstance(row, dict):
            continue
        name = str(row.get("name", "")).strip()
        if not row_matches(args.only_names, name):
            continue
        binary_rel = row.get("binary")
        addr_s = row.get("addr")
        if not binary_rel or not addr_s:
            print(f"error: row {name!r} missing binary or addr", file=sys.stderr)
            sys.exit(1)
        binary_path = ROOT / str(binary_rel)
        if not binary_path.is_file():
            print(f"error: missing binary for row {name}: {binary_path}", file=sys.stderr)
            sys.exit(1)

        cmd = [
            "cargo",
            "run",
            "-p",
            "fission-sleigh",
            *cargo_profile_flag,
            "--locked",
            "--example",
            "frontend_load_spec_parity",
            "--",
            "--binary",
            str(binary_path),
            "--addr",
            str(addr_s),
        ]
        env = os.environ.copy()
        spec_dir = ROOT / "utils" / "sleigh-specs"
        if spec_dir.is_dir():
            env.setdefault("FISSION_SLEIGH_SPEC_DIR", str(spec_dir))

        print("+", " ".join(cmd), flush=True)
        subprocess.run(cmd, cwd=ROOT, env=env, check=True)
        ran += 1

    if ran == 0:
        print(
            "error: no manifest rows matched "
            + (repr(args.only_names) if args.only_names else "default filter"),
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
