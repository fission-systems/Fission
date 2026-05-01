#!/usr/bin/env python3
"""Canary checks for bundled Ghidra data and Detect It Easy signatures.

Fails fast when checkout/submodule paths break or the DIE corpus is emptied.
Not a semantic correctness gate — filesystem presence and coarse counts only."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def _die_sg_count(die_root: Path) -> int:
    return sum(1 for _ in die_root.rglob("*.sg"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Repository root (default: infer from script location)",
    )
    parser.add_argument(
        "--min-sg-files",
        type=int,
        default=1500,
        help="Minimum recursive .sg files under DIE mirror (default: 1500)",
    )
    args = parser.parse_args()

    root = args.repo_root
    if root is None:
        root = Path(__file__).resolve().parents[2]

    ghidra = root / "utils" / "ghidra-data"
    ghidra_processors = ghidra / "Ghidra" / "Processors"

    die = root / "utils" / "signatures" / "die" / "detect-it-easy"
    die_db = die / "db"

    errors: list[str] = []

    if not ghidra.is_dir():
        errors.append(f"missing Ghidra data directory: {ghidra}")
    elif not ghidra_processors.is_dir():
        errors.append(f"missing Ghidra processors tree: {ghidra_processors}")

    if not die.is_dir():
        errors.append(f"missing DIE mirror root: {die}")
    elif not die_db.is_dir():
        errors.append(f"missing DIE db/: {die_db}")

    spot_checks = [
        die / "db" / "PE" / "packer_UPX.2.sg",
        die / "db" / "ELF" / "UPX.2.sg",
    ]
    for p in spot_checks:
        if not p.is_file():
            errors.append(f"missing DIE spot-check file: {p}")

    sg_total = _die_sg_count(die) if die.is_dir() else 0
    if sg_total < args.min_sg_files:
        errors.append(
            f"DIE .sg count too low: {sg_total} (minimum {args.min_sg_files})"
        )

    if errors:
        print("third_party_data_canary: FAILED", file=sys.stderr)
        for line in errors:
            print(f"  - {line}", file=sys.stderr)
        return 1

    print(
        f"third_party_data_canary: OK (Ghidra data under {ghidra}, "
        f"DIE .sg files={sg_total})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
