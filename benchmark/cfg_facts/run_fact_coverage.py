#!/usr/bin/env python3
"""Run Ghidra-vs-Fission CFG input fact coverage for one function or a manifest."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
THIS_DIR = Path(__file__).resolve().parent


def parse_int(value: str) -> int:
    return int(value, 0)


def run(cmd: list[str], *, check: bool = True) -> None:
    subprocess.run(cmd, cwd=ROOT, check=check)


def run_one(
    *,
    binary: Path,
    addr: int,
    language: str,
    compiler: str,
    ghidra_dir: Path | None,
    output_dir: Path,
    fission_release: bool,
) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    ghidra_json = output_dir / "ghidra_facts.json"
    fission_json = output_dir / "fission_facts.json"
    report_json = output_dir / "fact_coverage_report.json"

    run(
        [
            "python3",
            str(THIS_DIR / "ghidra_facts.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            "--language",
            language,
            "--compiler",
            compiler,
            *(["--ghidra-dir", str(ghidra_dir)] if ghidra_dir is not None else []),
            "--output",
            str(ghidra_json),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "fission_facts.py"),
            "--binary",
            str(binary),
            "--addr",
            hex(addr),
            *(["--release"] if fission_release else []),
            "--output",
            str(fission_json),
        ]
    )
    run(
        [
            "python3",
            str(THIS_DIR / "compare_facts.py"),
            "--ghidra",
            str(ghidra_json),
            "--fission",
            str(fission_json),
            "--output",
            str(report_json),
        ],
        check=False,
    )
    report = json.loads(report_json.read_text())
    report["report"] = str(report_json)
    report["ghidra_json"] = str(ghidra_json)
    report["fission_json"] = str(fission_json)
    return report


def run_manifest(args: argparse.Namespace) -> int:
    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    manifest = json.loads(manifest_path.read_text())
    defaults = manifest.get("defaults", {}) or {}
    rows = manifest.get("rows", [])
    if args.row:
        rows = [row for row in rows if row.get("name") == args.row]
        if not rows:
            raise SystemExit(f"no row named {args.row!r} in {manifest_path}")

    row_reports = []
    for row in rows:
        binary = Path(row.get("binary", args.binary or ""))
        if not binary:
            raise SystemExit(f"row {row.get('name', '<unnamed>')} is missing binary")
        binary = binary if binary.is_absolute() else ROOT / binary
        row_name = row.get("name", f"0x{parse_int(str(row['addr'])):x}")
        output_dir = args.output_dir / row_name
        report = run_one(
            binary=binary,
            addr=parse_int(str(row["addr"])),
            language=row.get("language", defaults.get("language", args.language)),
            compiler=row.get("compiler", defaults.get("compiler", args.compiler)),
            ghidra_dir=args.ghidra_dir,
            output_dir=output_dir,
            fission_release=args.fission_release,
        )
        row_reports.append(
            {
                "name": row_name,
                "binary": str(binary),
                "addr": row["addr"],
                "notes": row.get("notes"),
                "report": report["report"],
                "label_recall": report.get("label_recall"),
                "flow_edge_recall": report.get("flow_edge_recall"),
                "flow_edge_precision": report.get("flow_edge_precision"),
                "noreturn_match": report.get("noreturn_match"),
                "missing_labels": report.get("missing_labels", [])[:5],
                "missing_flow_edges": report.get("missing_flow_edges", [])[:5],
            }
        )

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(row_reports),
        "rows": row_reports,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    aggregate_path = args.output_dir / "aggregate_fact_coverage_report.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")

    md_lines = [
        "# CFG Fact Coverage",
        "",
        f"Manifest: `{manifest_path}`",
        f"Rows: {len(row_reports)}",
        "",
        "| Row | Label recall | Flow edge recall | Flow edge precision | Noreturn match |",
        "| --- | ---: | ---: | ---: | --- |",
    ]
    for row in row_reports:
        md_lines.append(
            "| {name} | {label_recall:.3f} | {flow_edge_recall:.3f} | {flow_edge_precision:.3f} | {noreturn_match} |".format(
                **row
            )
        )
    md_path = args.output_dir / "fact_coverage.md"
    md_path.write_text("\n".join(md_lines) + "\n")

    print(
        json.dumps(
            {
                "report": str(aggregate_path),
                "markdown": str(md_path),
                "row_count": aggregate["row_count"],
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--addr", type=parse_int)
    parser.add_argument("--language", default="x86:LE:64:default")
    parser.add_argument("--compiler", default="windows")
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "benchmark/artifacts/cfg_facts/latest",
    )
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--row")
    parser.add_argument("--fission-release", action="store_true")
    args = parser.parse_args()

    if args.manifest:
        return run_manifest(args)
    if args.binary is None or args.addr is None:
        parser.error("--binary and --addr are required unless --manifest is used")

    binary = args.binary if args.binary.is_absolute() else ROOT / args.binary
    report = run_one(
        binary=binary,
        addr=args.addr,
        language=args.language,
        compiler=args.compiler,
        ghidra_dir=args.ghidra_dir,
        output_dir=args.output_dir,
        fission_release=args.fission_release,
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    ok = report.get("label_recall", 0.0) >= 1.0 and report.get("flow_edge_recall", 0.0) >= 1.0
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
