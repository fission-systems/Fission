#!/usr/bin/env python3
"""Run loader-only smoke checks for a corpus manifest."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from collections import Counter
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FISSION = REPO_ROOT / "target/release/fission_cli"


def load_manifest(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    entries = payload.get("entries", payload)
    if not isinstance(entries, list):
        raise ValueError(f"manifest has no entries list: {path}")
    return {"name": payload.get("name", path.stem) if isinstance(payload, dict) else path.stem, "entries": entries}


def resolve_binary(path: str) -> Path:
    candidate = Path(path).expanduser()
    if candidate.is_absolute():
        return candidate
    return (REPO_ROOT / candidate).resolve()


def extract_json(stdout: str) -> Any:
    stripped = stdout.strip()
    starts = [idx for idx in (stripped.find("{"), stripped.find("[")) if idx >= 0]
    if not starts:
        raise ValueError("command produced no JSON payload")
    return json.loads(stripped[min(starts) :])


def run_json(command: list[str], timeout: float) -> tuple[Any | None, dict[str, Any]]:
    start = time.perf_counter()
    proc = subprocess.run(command, text=True, capture_output=True, timeout=timeout)
    elapsed = time.perf_counter() - start
    meta = {
        "returncode": proc.returncode,
        "elapsed_sec": elapsed,
        "stderr_tail": proc.stderr[-4000:],
        "stdout_tail": proc.stdout[-4000:],
    }
    if proc.returncode != 0:
        return None, meta
    try:
        return extract_json(proc.stdout), meta
    except Exception as exc:  # noqa: BLE001 - diagnostic payload should preserve parser error.
        meta["json_error"] = str(exc)
        return None, meta


def classify_error(meta: dict[str, Any]) -> str:
    text = f"{meta.get('stderr_tail', '')}\n{meta.get('stdout_tail', '')}\n{meta.get('json_error', '')}".lower()
    if "unsupported machine" in text:
        return "unsupported_machine"
    if "unknown binary format" in text or "unsupported format" in text:
        return "unsupported_format"
    if "load spec" in text:
        return "load_spec"
    if "timed out" in text or "timeout" in text:
        return "timeout"
    if meta.get("json_error"):
        return "json_parse"
    return "loader_error"


def run_entry(entry: dict[str, Any], fission_bin: Path, timeout: float, output_dir: Path) -> dict[str, Any]:
    binary = resolve_binary(str(entry["binary_path"]))
    row_id = str(entry.get("id") or binary.stem)
    info, info_meta = run_json([str(fission_bin), "info", "--json", str(binary)], timeout)
    functions = None
    list_meta: dict[str, Any] | None = None
    if info is not None:
        functions, list_meta = run_json([str(fission_bin), "list", "--json", str(binary)], timeout)
    status = "loaded" if info is not None else "load_failed"
    row = {
        "id": row_id,
        "binary_path": str(binary),
        "sha256": ((entry.get("metadata") or {}).get("sha256")),
        "status": status,
        "failure_bucket": None if status == "loaded" else classify_error(info_meta),
        "info": info,
        "function_count": len(functions) if isinstance(functions, list) else None,
        "list_error_bucket": classify_error(list_meta) if info is not None and functions is None and list_meta else None,
        "info_meta": info_meta,
        "list_meta": list_meta,
    }
    (output_dir / "rows").mkdir(parents=True, exist_ok=True)
    (output_dir / "rows" / f"{row_id}.json").write_text(json.dumps(row, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return row


def build_aggregate(rows: list[dict[str, Any]], manifest: dict[str, Any]) -> dict[str, Any]:
    status_counts = Counter(row["status"] for row in rows)
    failure_counts = Counter(row["failure_bucket"] for row in rows if row.get("failure_bucket"))
    format_counts = Counter(str((row.get("info") or {}).get("format", "unknown")) for row in rows if row.get("info"))
    arch_counts = Counter(str((row.get("info") or {}).get("arch", "unknown")) for row in rows if row.get("info"))
    function_counts = [row["function_count"] for row in rows if isinstance(row.get("function_count"), int)]
    return {
        "manifest_name": manifest["name"],
        "row_count": len(rows),
        "loaded": status_counts.get("loaded", 0),
        "load_failed": status_counts.get("load_failed", 0),
        "status_counts": dict(sorted(status_counts.items())),
        "failure_bucket_counts": dict(sorted(failure_counts.items())),
        "format_counts": dict(sorted(format_counts.items())),
        "arch_counts": dict(sorted(arch_counts.items())),
        "function_count_total": sum(function_counts),
        "function_count_min": min(function_counts) if function_counts else None,
        "function_count_max": max(function_counts) if function_counts else None,
        "rows": rows,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION)
    parser.add_argument("--timeout-sec", type=float, default=30.0)
    parser.add_argument("--limit", type=int)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    manifest = load_manifest(args.manifest)
    entries = manifest["entries"][: args.limit] if args.limit else manifest["entries"]
    args.output_dir.mkdir(parents=True, exist_ok=True)
    rows = [run_entry(entry, args.fission_bin, args.timeout_sec, args.output_dir) for entry in entries]
    aggregate = build_aggregate(rows, manifest)
    report_path = args.output_dir / "loader_smoke_report.json"
    report_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(report_path), "loaded": aggregate["loaded"], "load_failed": aggregate["load_failed"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
