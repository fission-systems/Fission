#!/usr/bin/env python3
"""Compare Ghidra and Fission disassembly rows.

This is a loader/decode diagnostic lane. It reports address, byte, and textual
instruction parity without following thunks or repairing semantic output.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import pyghidra


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_GHIDRA_DIRS = (
    ROOT / "vendor" / "ghidra" / "ghidra_12.0.4_PUBLIC",
    ROOT / "vendor" / "ghidra" / "ghidra-Ghidra_12.0.4_build",
)


def parse_int(value: str) -> int:
    return int(value, 0)


def normalize_ghidra_install_dir(path: Path) -> Path:
    candidate = path.expanduser().resolve()
    if (candidate / "Ghidra" / "application.properties").exists():
        return candidate
    if candidate.name == "Ghidra" and (candidate / "application.properties").exists():
        return candidate.parent
    return candidate


def is_launchable_ghidra_install_dir(path: Path) -> bool:
    install_dir = normalize_ghidra_install_dir(path)
    return (
        (install_dir / "Ghidra" / "application.properties").exists()
        and (install_dir / "Ghidra" / "Features" / "PyGhidra" / "lib" / "PyGhidra.jar").exists()
        and (install_dir / "support").exists()
    )


def resolve_ghidra_dir(cli_value: Path | None) -> Path:
    candidates: list[Path] = []
    if cli_value is not None:
        candidates.append(cli_value)
    for env_name in ("FISSION_GHIDRA_DIR", "GHIDRA_INSTALL_DIR"):
        env_dir = os.environ.get(env_name)
        if env_dir:
            candidates.append(Path(env_dir))
    candidates.extend(DEFAULT_GHIDRA_DIRS)
    for candidate in candidates:
        normalized = normalize_ghidra_install_dir(candidate)
        if is_launchable_ghidra_install_dir(normalized):
            return normalized
    checked = ", ".join(str(normalize_ghidra_install_dir(path)) for path in candidates)
    raise FileNotFoundError(f"Launchable Ghidra install not found. Checked: {checked}")


def instruction_text(instr: Any) -> str:
    return str(instr).strip()


def java_bytes_to_hex(raw: Any) -> str:
    return " ".join(f"{int(byte) & 0xff:02x}" for byte in raw)


def dump_ghidra_asm(binary: Path, addr: int, count: int, ghidra_dir: Path) -> dict[str, Any]:
    pyghidra.start(install_dir=ghidra_dir)
    with pyghidra.open_program(binary, analyze=True) as flat:
        program = flat.getCurrentProgram()
        address_space = program.getAddressFactory().getDefaultAddressSpace()
        listing = program.getListing()
        current = address_space.getAddress(addr)
        instructions: list[dict[str, Any]] = []
        for _ in range(count):
            instr = listing.getInstructionAt(current)
            if instr is None:
                instructions.append(
                    {
                        "address": f"0x{int(current.getOffset()):x}",
                        "status": "error",
                        "error": f"no instruction at 0x{int(current.getOffset()):x}",
                    }
                )
                break
            instructions.append(
                {
                    "address": f"0x{int(instr.getAddress().getOffset()):x}",
                    "status": "ok",
                    "bytes": java_bytes_to_hex(instr.getBytes()),
                    "instruction": instruction_text(instr),
                    "mnemonic": str(instr.getMnemonicString()),
                    "flow_type": str(instr.getFlowType()),
                    "flows": [f"0x{int(flow.getOffset()):x}" for flow in instr.getFlows()],
                }
            )
            current = instr.getAddress().add(instr.getLength())
        return {"tool": "ghidra", "instructions": instructions}


def extract_json(stdout: str) -> Any:
    stripped = stdout.strip()
    for marker in ("[", "{"):
        idx = stripped.find(marker)
        if idx >= 0:
            try:
                return json.loads(stripped[idx:])
            except json.JSONDecodeError:
                continue
    raise ValueError(f"failed to parse JSON from fission stdout: {stdout[:400]}")


def dump_fission_asm(fission_bin: Path, binary: Path, addr: int, count: int) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        "disasm",
        str(binary),
        "--addr",
        hex(addr),
        "--count",
        str(count),
        "--json",
    ]
    proc = subprocess.run(cmd, text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(
            f"fission disasm failed with exit={proc.returncode}\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )
    instructions = extract_json(proc.stdout)
    if not isinstance(instructions, list):
        raise ValueError("fission disasm JSON must be a list")
    return {"tool": "fission", "instructions": instructions, "stderr": proc.stderr}


def normalize_text(text: str | None) -> str:
    return " ".join((text or "").lower().replace(",", " , ").split())


def sequence_similarity(left: str, right: str) -> float:
    if not left and not right:
        return 1.0
    if not left or not right:
        return 0.0
    return difflib.SequenceMatcher(None, left, right, autojunk=False).ratio()


def bytes_similarity(left: str | None, right: str | None) -> float:
    left_tokens = normalize_text(left).split()
    right_tokens = normalize_text(right).split()
    if not left_tokens and not right_tokens:
        return 1.0
    if not left_tokens or not right_tokens:
        return 0.0
    max_len = max(len(left_tokens), len(right_tokens))
    matches = sum(1 for lhs, rhs in zip(left_tokens, right_tokens) if lhs == rhs)
    return matches / max_len


def compare_instruction(ghidra: dict[str, Any] | None, fission: dict[str, Any] | None) -> dict[str, Any]:
    if ghidra is None:
        return {
            "bucket": "missing_ghidra_instruction",
            "similarity_score": 0.0,
            "address_score": 0.0,
            "bytes_score": 0.0,
            "text_score": 0.0,
        }
    if fission is None:
        return {
            "bucket": "missing_fission_instruction",
            "similarity_score": 0.0,
            "address_score": 0.0,
            "bytes_score": 0.0,
            "text_score": 0.0,
        }
    if ghidra.get("status") != "ok":
        return {
            "bucket": "ghidra_decode_error",
            "ghidra_error": ghidra.get("error"),
            "similarity_score": 0.0,
            "address_score": 0.0,
            "bytes_score": 0.0,
            "text_score": 0.0,
        }
    address_match = ghidra.get("address") == fission.get("address")
    bytes_match = normalize_text(ghidra.get("bytes")) == normalize_text(fission.get("bytes"))
    text_match = normalize_text(ghidra.get("instruction")) == normalize_text(fission.get("instruction"))
    address_score = 1.0 if address_match else 0.0
    byte_score = bytes_similarity(ghidra.get("bytes"), fission.get("bytes"))
    text_score = sequence_similarity(
        normalize_text(ghidra.get("instruction")),
        normalize_text(fission.get("instruction")),
    )
    similarity_score = (address_score + byte_score + text_score) / 3.0
    bucket = "full_match" if address_match and bytes_match and text_match else "asm_mismatch"
    return {
        "bucket": bucket,
        "address_match": address_match,
        "bytes_match": bytes_match,
        "text_match": text_match,
        "address_score": address_score,
        "bytes_score": byte_score,
        "text_score": text_score,
        "similarity_score": similarity_score,
        "ghidra": ghidra,
        "fission": fission,
    }


def average(values: list[float]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)


def similarity_summary(comparisons: list[dict[str, Any]]) -> dict[str, float]:
    return {
        "average_similarity_score": average(
            [float(comparison.get("similarity_score", 0.0)) for comparison in comparisons]
        ),
        "average_address_score": average(
            [float(comparison.get("address_score", 0.0)) for comparison in comparisons]
        ),
        "average_bytes_score": average(
            [float(comparison.get("bytes_score", 0.0)) for comparison in comparisons]
        ),
        "average_text_score": average(
            [float(comparison.get("text_score", 0.0)) for comparison in comparisons]
        ),
    }


def run_row(
    entry: dict[str, Any],
    row: dict[str, Any],
    ghidra_dir: Path,
    fission_bin: Path,
    output_dir: Path,
) -> dict[str, Any]:
    binary = Path(entry["binary_path"])
    addr = parse_int(row["address"])
    count = int(row.get("count", 1))
    started_at = time.perf_counter()
    ghidra = dump_ghidra_asm(binary, addr, count, ghidra_dir)
    fission = dump_fission_asm(fission_bin, binary, addr, count)
    elapsed = time.perf_counter() - started_at
    comparisons = []
    max_len = max(len(ghidra["instructions"]), len(fission["instructions"]))
    for idx in range(max_len):
        comparisons.append(
            compare_instruction(
                ghidra["instructions"][idx] if idx < len(ghidra["instructions"]) else None,
                fission["instructions"][idx] if idx < len(fission["instructions"]) else None,
            )
        )
    bucket_totals: dict[str, int] = {}
    for comparison in comparisons:
        bucket_totals[comparison["bucket"]] = bucket_totals.get(comparison["bucket"], 0) + 1
    row_similarity = similarity_summary(comparisons)
    report = {
        "entry_id": entry.get("id"),
        "row_id": row.get("id"),
        "role": row.get("role"),
        "binary": str(binary),
        "address": row["address"],
        "count": count,
        "elapsed_sec": elapsed,
        "bucket_totals": bucket_totals,
        **row_similarity,
        "ghidra": ghidra,
        "fission": fission,
        "comparisons": comparisons,
    }
    row_dir = output_dir / str(entry.get("id", "entry")) / str(row.get("id", f"0x{addr:x}"))
    row_dir.mkdir(parents=True, exist_ok=True)
    (row_dir / "asm_parity_report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--ghidra-dir", type=Path, default=None)
    parser.add_argument("--fission-bin", type=Path, default=ROOT / "target" / "release" / "fission_cli")
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text())
    ghidra_dir = resolve_ghidra_dir(args.ghidra_dir)
    rows: list[dict[str, Any]] = []
    for entry in manifest.get("entries", []):
        for row in entry.get("rows", []):
            rows.append(run_row(entry, row, ghidra_dir, args.fission_bin, args.output_dir))

    aggregate: dict[str, Any] = {
        "manifest": manifest.get("name"),
        "row_count": len(rows),
        "bucket_totals": {},
        "rows": [
            {
                "entry_id": row["entry_id"],
                "row_id": row["row_id"],
                "address": row["address"],
                "bucket_totals": row["bucket_totals"],
            }
            for row in rows
        ],
    }
    for row in rows:
        for bucket, count in row["bucket_totals"].items():
            aggregate["bucket_totals"][bucket] = aggregate["bucket_totals"].get(bucket, 0) + count
    all_comparisons = [
        comparison
        for row in rows
        for comparison in row.get("comparisons", [])
        if isinstance(comparison, dict)
    ]
    aggregate.update(similarity_summary(all_comparisons))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "aggregate_asm_parity_report.json").write_text(
        json.dumps(aggregate, indent=2, sort_keys=True) + "\n"
    )
    print(json.dumps(aggregate, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
