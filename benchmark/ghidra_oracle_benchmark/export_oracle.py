#!/usr/bin/env python3
"""Export Ghidra semantic-oracle facts for Stage Parity / timeout analysis."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
FULL_BENCHMARK = ROOT / "benchmark" / "full_benchmark"
ORACLE_DIR = Path(__file__).resolve().parent

sys.path.insert(0, str(FULL_BENCHMARK))
sys.path.insert(0, str(ORACLE_DIR))

from collectors import collect_binary_snapshot, collect_function_oracle  # noqa: E402

from grand_finale_support.runners import (  # noqa: E402
    _build_ghidra_function_identity_maps,
    _resolve_ghidra_seed_target,
)


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def manifest_sha256(path: Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def snapshot_only_rows(binary_id: str, binary_snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """Emit one row when the manifest has no function targets (micro loader / smoke)."""

    return [
        {
            "binary_id": binary_id,
            "feature_group": "snapshot",
            "feature": "binary_only",
            "address": None,
            "requested_name": None,
            "match_evidence": "SnapshotOnly",
            "canonical_identity": {},
            "binary_snapshot": binary_snapshot,
            "ghidra": {
                "decompile_success": False,
                "decompile_sec": 0.0,
                "decompile_failure_reason": "no_manifest_rows",
                "collector_warnings": [],
            },
        }
    ]


def export_rows_for_program(
    program: Any,
    *,
    binary_id: str,
    rows: list[dict[str, Any]],
    timeout_sec: int,
    decomp: Any,
    monitor: Any,
    binary_snapshot: dict[str, Any],
) -> list[dict[str, Any]]:
    fm = program.getFunctionManager()
    addr_factory = program.getAddressFactory()
    image_base_offset = int(program.getImageBase().getOffset())
    identity_maps = _build_ghidra_function_identity_maps(list(fm.getFunctions(True)), image_base_offset)

    out_rows: list[dict[str, Any]] = []
    for row in rows:
        addr = str(row.get("addr") or row.get("address") or "").strip()
        name = str(row.get("name") or "").strip()
        if not addr:
            out_rows.append(
                {
                    "binary_id": binary_id,
                    "feature_group": row.get("feature_group"),
                    "feature": row.get("feature"),
                    "address": addr,
                    "requested_name": name,
                    "match_evidence": "ManifestMissingAddr",
                    "canonical_identity": {},
                    "binary_snapshot": binary_snapshot,
                    "ghidra": collect_function_oracle(program, None, decomp, timeout_sec, monitor),
                }
            )
            continue

        target, evidence, details = _resolve_ghidra_seed_target(addr, name, addr_factory, fm, identity_maps)

        oracle = collect_function_oracle(program, target, decomp, timeout_sec, monitor)
        out_rows.append(
            {
                "binary_id": binary_id,
                "feature_group": row.get("feature_group"),
                "feature": row.get("feature"),
                "address": addr,
                "requested_name": name,
                "match_evidence": evidence,
                "canonical_identity": details,
                "binary_snapshot": binary_snapshot,
                "ghidra": oracle,
            }
        )
    return out_rows


def export_file_binary(
    repo_root: Path,
    bin_entry: dict[str, Any],
    timeout_sec: int,
    pyghidra_mod: Any,
) -> list[dict[str, Any]]:
    rel_path = bin_entry.get("path")
    if not rel_path:
        raise ValueError(f"binary {bin_entry.get('id')} missing path")
    binary_path = repo_root / str(rel_path)
    if not binary_path.is_file():
        raise FileNotFoundError(binary_path)

    import jpype  # noqa: PLC0415
    from ghidra.app.decompiler import DecompInterface  # noqa: PLC0415
    from ghidra.util.task import ConsoleTaskMonitor  # noqa: PLC0415

    rows_out: list[dict[str, Any]] = []
    monitor = ConsoleTaskMonitor()

    with pyghidra_mod.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        decomp = DecompInterface()
        decomp.openProgram(program)
        binary_snapshot = collect_binary_snapshot(program)
        rows_list = list(bin_entry.get("rows") or [])
        try:
            if not rows_list:
                rows_out.extend(snapshot_only_rows(str(bin_entry["id"]), binary_snapshot))
            else:
                rows_out.extend(
                    export_rows_for_program(
                        program,
                        binary_id=str(bin_entry["id"]),
                        rows=rows_list,
                        timeout_sec=timeout_sec,
                        decomp=decomp,
                        monitor=monitor,
                        binary_snapshot=binary_snapshot,
                    )
                )
        finally:
            try:
                decomp.dispose()
            except Exception:
                pass

    jpype.java.lang.System.gc()
    return rows_out


def export_hex_binary(
    bin_entry: dict[str, Any],
    timeout_sec: int,
    pyghidra_mod: Any,
    artifact_parent: Path,
) -> list[dict[str, Any]]:
    hex_blob = bin_entry.get("hex_bytes")
    if not hex_blob:
        raise ValueError(f"binary {bin_entry.get('id')} missing hex_bytes")
    raw = bytes.fromhex(str(hex_blob).replace(" ", "").replace("\n", ""))

    program_name = str(bin_entry.get("program_name") or "synthetic")
    language = str(bin_entry.get("language") or "DATA:LE:64:default")
    loader_name = str(bin_entry.get("loader") or "BinaryLoader")
    binary_id = str(bin_entry["id"])

    import jpype  # noqa: PLC0415
    from ghidra.app.decompiler import DecompInterface  # noqa: PLC0415
    from ghidra.util.task import ConsoleTaskMonitor  # noqa: PLC0415

    proj_parent = artifact_parent / "micro_projects" / binary_id
    proj_parent.mkdir(parents=True, exist_ok=True)
    project_dir_name = f"oracle_micro_{binary_id}"

    ByteArrayCls = jpype.JArray(jpype.JByte)
    jbytes = ByteArrayCls(raw)

    rows_out: list[dict[str, Any]] = []
    monitor = ConsoleTaskMonitor()

    with pyghidra_mod.open_project(proj_parent, project_dir_name, create=True) as project:
        loader_b = pyghidra_mod.program_loader().project(project).source(jbytes).name(program_name)
        loader_b = loader_b.loaders(loader_name).language(language)
        with loader_b.load() as load_results:
            load_results.save(pyghidra_mod.task_monitor())

        prog_path = "/" + program_name.lstrip("/")
        with pyghidra_mod.program_context(project, prog_path) as program:
            pyghidra_mod.analyze(program, pyghidra_mod.task_monitor())

            decomp = DecompInterface()
            decomp.openProgram(program)
            binary_snapshot = collect_binary_snapshot(program)
            rows_list = list(bin_entry.get("rows") or [])
            try:
                if not rows_list:
                    rows_out.extend(snapshot_only_rows(binary_id, binary_snapshot))
                else:
                    rows_out.extend(
                        export_rows_for_program(
                            program,
                            binary_id=binary_id,
                            rows=rows_list,
                            timeout_sec=timeout_sec,
                            decomp=decomp,
                            monitor=monitor,
                            binary_snapshot=binary_snapshot,
                        )
                    )
            finally:
                try:
                    decomp.dispose()
                except Exception:
                    pass

    jpype.java.lang.System.gc()
    return rows_out


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--ghidra-dir", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--repo-root", type=Path, default=ROOT)
    p.add_argument("--per-function-timeout-sec", type=int, default=180)
    p.add_argument("--binary-id", type=str, default=None, help="process only this manifest binary id")
    return p.parse_args()


def main() -> int:
    args = parse_args()
    manifest = load_manifest(args.manifest)
    binaries = manifest.get("binaries") or []
    if not isinstance(binaries, list):
        raise SystemExit("manifest.binaries must be a list")

    os.environ["GHIDRA_INSTALL_DIR"] = str(args.ghidra_dir)
    import pyghidra  # noqa: PLC0415

    pyghidra.start(install_dir=args.ghidra_dir)

    artifact_parent = args.repo_root / "benchmark" / "artifacts" / "ghidra_oracle_micro"
    artifact_parent.mkdir(parents=True, exist_ok=True)

    all_rows: list[dict[str, Any]] = []
    wall_start = time.perf_counter()

    for bin_entry in binaries:
        if not isinstance(bin_entry, dict):
            continue
        bid = str(bin_entry.get("id") or "").strip()
        if args.binary_id and bid != args.binary_id:
            continue
        if not bid:
            raise SystemExit("manifest binary entry missing id")

        if bin_entry.get("hex_bytes"):
            all_rows.extend(
                export_hex_binary(
                    bin_entry,
                    args.per_function_timeout_sec,
                    pyghidra,
                    artifact_parent,
                )
            )
        else:
            all_rows.extend(
                export_file_binary(
                    args.repo_root,
                    bin_entry,
                    args.per_function_timeout_sec,
                    pyghidra,
                )
            )

    payload = {
        "_meta": {
            "tool": "ghidra_oracle_export",
            "manifest": str(args.manifest),
            "manifest_sha256": manifest_sha256(args.manifest),
            "ghidra_install_dir": str(args.ghidra_dir),
            "wall_clock_sec": round(time.perf_counter() - wall_start, 6),
            "row_count": len(all_rows),
        },
        "rows": all_rows,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"wrote {args.out} ({len(all_rows)} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
