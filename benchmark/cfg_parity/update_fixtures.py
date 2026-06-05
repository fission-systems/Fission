#!/usr/bin/env python3
"""Copy Ghidra CFG snapshots from a parity run into checked-in fixtures."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        required=True,
        help="Directory containing row subdirs from run_cfg_parity.py",
    )
    args = parser.parse_args()

    source = args.source if args.source.is_absolute() else ROOT / args.source
    aggregate_path = source / "aggregate_cfg_parity_report.json"
    aggregate = {}
    if aggregate_path.exists():
        aggregate = json.loads(aggregate_path.read_text())

    match_by_name = {
        row["name"]: row.get("buckets") == ["full_match"]
        for row in aggregate.get("rows", [])
    }

    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)

    updated = []
    for row_dir in sorted(source.iterdir()):
        if not row_dir.is_dir():
            continue
        ghidra_json = row_dir / "ghidra_cfg.json"
        if not ghidra_json.exists():
            continue
        payload = json.loads(ghidra_json.read_text())
        row_name = row_dir.name
        binary = payload.get("binary")
        if isinstance(binary, str) and binary.startswith(str(ROOT)):
            binary = str(Path(binary).relative_to(ROOT))
        fixture = {
            "name": row_name,
            "binary": binary,
            "function_name": payload.get("function_name"),
            "function_address": payload.get("snapshot", {}).get("function_address"),
            "fission_model": "pcode_instruction_cfg",
            "ghidra_model": payload.get("model"),
            "expect_full_match": match_by_name.get(row_name, False),
            "snapshot": payload.get("snapshot"),
        }
        out_path = FIXTURES_DIR / f"{row_name}.json"
        out_path.write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")
        updated.append(out_path.name)

    print(json.dumps({"updated_fixtures": updated, "count": len(updated)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
