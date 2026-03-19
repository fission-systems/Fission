from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any


def normalize_address(value: str) -> str:
    text = str(value).strip()
    if text.startswith(("0x", "0X")):
        text = text[2:]
    return text.lower()


def load_inventory_rows(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if not path.exists():
        return rows
    with path.open() as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
    return rows


def load_inventory_summary(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    return json.loads(path.read_text())


def run_function_facts_inventory(
    root_dir: Path,
    binary_path: Path,
    fission_bin: Path,
    *,
    timeout_ms: int = 10000,
    functions_limit: int | None = None,
    chunk_size: int = 100,
    quiet_batch_errors: bool = True,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    with tempfile.TemporaryDirectory(prefix="fission_function_inventory_") as tmpdir:
        tmpdir_path = Path(tmpdir)
        output_jsonl = tmpdir_path / "rows.jsonl"
        summary_json = tmpdir_path / "summary.json"
        cmd = [
            str(fission_bin),
            str(binary_path),
            "--emit-function-facts-inventory",
            "--timeout-ms",
            str(timeout_ms),
            "--chunk-size",
            str(max(chunk_size, 1)),
            "--output-jsonl",
            str(output_jsonl),
            "--summary-json",
            str(summary_json),
        ]
        if functions_limit is not None:
            cmd.extend(["--functions-limit", str(functions_limit)])
        if quiet_batch_errors:
            cmd.append("--quiet-batch-errors")

        subprocess.run(
            cmd,
            cwd=root_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=True,
        )
        return load_inventory_rows(output_jsonl), load_inventory_summary(summary_json)

