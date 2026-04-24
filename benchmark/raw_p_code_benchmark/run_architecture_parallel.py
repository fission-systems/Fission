#!/usr/bin/env python3
"""Run architecture-scoped raw p-code smoke rows in parallel."""

from __future__ import annotations

import argparse
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def run_row(
    *,
    row: dict[str, Any],
    ghidra_dir: Path | None,
    output_root: Path,
    fission_release: bool,
) -> dict[str, Any]:
    row_name = row["name"]
    output_dir = output_root / row_name
    cmd = [
        "python3",
        str(THIS_DIR / "run_raw_pcode_parity.py"),
        "--binary",
        str((ROOT / row["binary"]).resolve()),
        "--addr",
        str(row["addr"]),
        "--count",
        str(row.get("count", 8)),
        "--language",
        row["language"],
        "--compiler",
        row["compiler"],
        "--output-dir",
        str(output_dir),
    ]
    if ghidra_dir is not None:
        cmd += ["--ghidra-dir", str(ghidra_dir)]
    if fission_release:
        cmd.append("--fission-release")

    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    row_report: dict[str, Any] = {
        "name": row_name,
        "entry_id": row.get("entry_id"),
        "language": row["language"],
        "compiler": row["compiler"],
        "binary": str((ROOT / row["binary"]).resolve()),
        "output_dir": str(output_dir),
        "status": "ok" if proc.returncode == 0 else "error",
        "returncode": proc.returncode,
    }
    if proc.returncode == 0:
        summary = json.loads(proc.stdout)
        row_report["summary"] = summary
        row_report["report"] = summary.get("report")
        row_report["bucket_totals"] = summary.get("bucket_totals", {})
        row_report["performance"] = summary.get("performance", {})
    else:
        row_report["stdout"] = proc.stdout
        row_report["stderr"] = proc.stderr
    return row_report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=THIS_DIR / "llvm_arch_smoke_rows.json",
    )
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "benchmark/artifacts/raw_p_code_benchmark/architecture_parallel_latest",
    )
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--fission-release", action="store_true")
    args = parser.parse_args()

    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    manifest = json.loads(manifest_path.read_text())
    rows = manifest.get("rows", [])
    args.output_dir.mkdir(parents=True, exist_ok=True)

    results = []
    with ThreadPoolExecutor(max_workers=max(1, args.workers)) as pool:
        futures = {
            pool.submit(
                run_row,
                row=row,
                ghidra_dir=args.ghidra_dir,
                output_root=args.output_dir,
                fission_release=args.fission_release,
            ): row
            for row in rows
        }
        for future in as_completed(futures):
            results.append(future.result())

    results.sort(key=lambda item: item["name"])
    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(results),
        "ok_count": sum(1 for item in results if item["status"] == "ok"),
        "error_count": sum(1 for item in results if item["status"] != "ok"),
        "rows": results,
    }
    aggregate_path = args.output_dir / "architecture_parallel_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "report": str(aggregate_path),
        "row_count": aggregate["row_count"],
        "ok_count": aggregate["ok_count"],
        "error_count": aggregate["error_count"],
    }, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
