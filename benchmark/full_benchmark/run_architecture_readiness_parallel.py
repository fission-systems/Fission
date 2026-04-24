#!/usr/bin/env python3
"""Run architecture readiness smoke in parallel over a corpus manifest."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CORPUS = ROOT / "benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json"
DEFAULT_OUTPUT = ROOT / "benchmark/artifacts/full_benchmark/architecture_readiness_latest"
DEFAULT_CLI = ROOT / "target/release/fission_cli"


def extract_json_payload(text: str) -> Any:
    lines = text.splitlines()
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("{") or stripped.startswith("["):
            return json.loads("\n".join(lines[index:]))
    raise ValueError(f"no JSON payload found in output:\n{text}")


def parse_hexish(value: str | int | None) -> int | None:
    if value is None:
        return None
    if isinstance(value, int):
        return value
    return int(value, 0)


def normalize_error_text(value: str | None) -> str:
    if not value:
        return ""
    normalized = value.replace("│", " ").replace("×", " ").replace("•", " ")
    normalized = re.sub(r"\s+", " ", normalized)
    return normalized.strip()


def classify_failure_bucket(
    info: dict[str, Any],
    list_stage: dict[str, Any],
    disasm: dict[str, Any],
) -> tuple[str | None, str | None, str | None, str | None]:
    info_error = str(info.get("error") or "")
    list_error = str(list_stage.get("error") or "")
    disasm_error = str(disasm.get("error") or "")
    normalized_info_error = normalize_error_text(info_error)
    normalized_list_error = normalize_error_text(list_error)
    normalized_disasm_error = normalize_error_text(disasm_error)

    if "unsupported machine for ELF" in normalized_info_error or "unsupported machine for ELF" in normalized_list_error:
        return (
            "loader_unsupported_machine",
            "fission-core::architecture::select_elf_load_spec",
            normalized_disasm_error or None,
            "matched 'unsupported machine for ELF' in info/list error",
        )
    if info.get("status") == "error" or list_stage.get("status") == "error":
        return (
            "loader_parse_failure",
            "fission-loader",
            normalized_disasm_error or None,
            "info/list stage returned error without unsupported-machine match",
        )
    if "compile-only" in normalized_disasm_error and "entry" in normalized_disasm_error:
        return (
            "selection_compile_only",
            "fission-sleigh::runtime::registry",
            normalized_disasm_error or None,
            "matched normalized disasm error containing 'compile-only' and 'entry'",
        )
    if "has no executable entry" in normalized_disasm_error:
        return (
            "selection_no_executable_entry",
            "fission-sleigh::runtime::registry",
            normalized_disasm_error or None,
            "matched normalized disasm error containing 'has no executable entry'",
        )
    if "unknown runtime language id" in normalized_disasm_error:
        return (
            "selection_unknown_language",
            "fission-sleigh::runtime::registry",
            normalized_disasm_error or None,
            "matched normalized disasm error containing 'unknown runtime language id'",
        )
    if disasm.get("status") == "skipped":
        return (
            "no_disasm_seed",
            "benchmark/full_benchmark/run_architecture_readiness_parallel.py",
            normalized_disasm_error or None,
            "disasm stage skipped because no seed address was available",
        )
    if disasm.get("status") == "error":
        return (
            "disasm_runtime_failure",
            "fission-sleigh",
            normalized_disasm_error or None,
            "disasm stage failed without matching selection/loader buckets",
        )
    return None, None, normalized_disasm_error or None, None


def run_json_command(cmd: list[str]) -> dict[str, Any]:
    started_at = time.perf_counter()
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed_sec = time.perf_counter() - started_at
    result: dict[str, Any] = {
        "cmd": cmd,
        "returncode": proc.returncode,
        "wall_clock_sec": elapsed_sec,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "status": "ok" if proc.returncode == 0 else "error",
    }
    if proc.returncode == 0:
        result["json"] = extract_json_payload(proc.stdout)
    return result


def readiness_class(info_ok: bool, list_ok: bool, disasm_ok: bool) -> str:
    if disasm_ok:
        return "disasm_ready"
    if list_ok:
        return "inventory_ready"
    if info_ok:
        return "metadata_ready"
    return "load_failed"


def diff_counter(
    current: dict[str, int],
    baseline: dict[str, int],
) -> dict[str, int]:
    keys = sorted(set(current) | set(baseline))
    return {key: int(current.get(key, 0)) - int(baseline.get(key, 0)) for key in keys}


def build_comparison_to_baseline(
    current: dict[str, Any],
    baseline: dict[str, Any],
    *,
    baseline_report: str,
) -> dict[str, Any]:
    baseline_rows = {row["id"]: row for row in baseline.get("rows", [])}
    current_rows = {row["id"]: row for row in current.get("rows", [])}

    rows_promoted_to_disasm_ready = []
    rows_no_longer_load_failed = []
    readiness_transitions: dict[str, int] = {}
    failure_bucket_transitions: dict[str, int] = {}

    for row_id, row in current_rows.items():
        old = baseline_rows.get(row_id)
        if old is None:
            continue
        old_class = old.get("readiness_class")
        new_class = row.get("readiness_class")
        if old_class != new_class:
            key = f"{old_class}->{new_class}"
            readiness_transitions[key] = readiness_transitions.get(key, 0) + 1
        if old_class != "disasm_ready" and new_class == "disasm_ready":
            rows_promoted_to_disasm_ready.append(row_id)
        if old_class == "load_failed" and new_class != "load_failed":
            rows_no_longer_load_failed.append(row_id)

        old_bucket = old.get("failure_bucket")
        new_bucket = row.get("failure_bucket")
        if old_bucket != new_bucket:
            key = f"{old_bucket}->{new_bucket}"
            failure_bucket_transitions[key] = failure_bucket_transitions.get(key, 0) + 1

    current_perf = current.get("performance_summary", {})
    baseline_perf = baseline.get("performance_summary", {})
    perf_delta = {
        "total_wall_clock_sec": (
            current_perf.get("total_wall_clock_sec", 0.0)
            - baseline_perf.get("total_wall_clock_sec", 0.0)
        ),
        "avg_wall_clock_sec_per_binary": (
            (current_perf.get("avg_wall_clock_sec_per_binary") or 0.0)
            - (baseline_perf.get("avg_wall_clock_sec_per_binary") or 0.0)
        ),
    }

    return {
        "baseline_report": baseline_report,
        "readiness_totals_delta": diff_counter(
            current.get("readiness_totals", {}),
            baseline.get("readiness_totals", {}),
        ),
        "failure_bucket_totals_delta": diff_counter(
            current.get("failure_bucket_totals", {}),
            baseline.get("failure_bucket_totals", {}),
        ),
        "stage_totals_delta": diff_counter(
            current.get("stage_totals", {}),
            baseline.get("stage_totals", {}),
        ),
        "performance_summary_delta": perf_delta,
        "row_transition_counts": dict(sorted(readiness_transitions.items())),
        "failure_bucket_transition_counts": dict(sorted(failure_bucket_transitions.items())),
        "rows_promoted_to_disasm_ready": sorted(rows_promoted_to_disasm_ready),
        "rows_no_longer_load_failed": sorted(rows_no_longer_load_failed),
    }


def run_entry(entry: dict[str, Any], cli_binary: Path, output_dir: Path) -> dict[str, Any]:
    binary_path = Path(entry["binary_path"])
    row_dir = output_dir / entry["id"]
    row_dir.mkdir(parents=True, exist_ok=True)

    info_result = run_json_command([str(cli_binary), "info", str(binary_path), "--json"])
    list_result = run_json_command([str(cli_binary), "list", str(binary_path), "--json"])
    sections_result = None

    disasm_addr = None
    seed_source = None
    functions = []
    if list_result["status"] == "ok":
        functions = list_result.get("json", [])
        if functions:
            disasm_addr = parse_hexish(functions[0].get("address"))
            seed_source = "first_listed_function"
    if disasm_addr is None and info_result["status"] == "ok":
        disasm_addr = parse_hexish(info_result["json"].get("entry"))
        if disasm_addr is not None:
            seed_source = "entry_point"
    if disasm_addr is None and info_result["status"] == "ok":
        sections_result = run_json_command([str(cli_binary), "info", str(binary_path), "--sections", "--json"])
        if sections_result["status"] == "ok":
            for section in sections_result.get("json", []):
                if section.get("executable"):
                    disasm_addr = parse_hexish(section.get("virtual_address"))
                    seed_source = "first_executable_section"
                    break

    if disasm_addr is not None:
        disasm_result = run_json_command(
            [str(cli_binary), "disasm", str(binary_path), "--addr", hex(disasm_addr), "--count", "4", "--json"]
        )
    else:
        disasm_result = {
            "status": "skipped",
            "returncode": None,
            "wall_clock_sec": 0.0,
            "reason": "no_disasm_addr",
        }

    info_json = info_result.get("json", {}) if info_result["status"] == "ok" else {}
    disasm_json = disasm_result.get("json", []) if disasm_result.get("status") == "ok" else []
    failure_bucket, failure_owner, normalized_disasm_error, classification_reason = classify_failure_bucket(
        {
            "status": info_result["status"],
            "error": info_result.get("stderr") or info_result.get("stdout"),
        },
        {
            "status": list_result["status"],
            "error": list_result.get("stderr") or list_result.get("stdout"),
        },
        {
            "status": disasm_result["status"],
            "error": disasm_result.get("reason") or disasm_result.get("stderr") or disasm_result.get("stdout"),
        },
    )

    row_report = {
        "id": entry["id"],
        "binary_path": str(binary_path),
        "tags": entry.get("tags", []),
        "info": {
            "status": info_result["status"],
            "returncode": info_result["returncode"],
            "wall_clock_sec": info_result["wall_clock_sec"],
            "arch": info_json.get("arch"),
            "bits": info_json.get("bits"),
            "format": info_json.get("format"),
            "functions": info_json.get("functions"),
            "entry": info_json.get("entry"),
            "error": None if info_result["status"] == "ok" else info_result.get("stderr") or info_result.get("stdout"),
        },
        "list": {
            "status": list_result["status"],
            "returncode": list_result["returncode"],
            "wall_clock_sec": list_result["wall_clock_sec"],
            "function_count": len(functions),
            "first_function": functions[0] if functions else None,
            "error": None if list_result["status"] == "ok" else list_result.get("stderr") or list_result.get("stdout"),
        },
        "disasm": {
            "status": disasm_result["status"],
            "returncode": disasm_result.get("returncode"),
            "wall_clock_sec": disasm_result["wall_clock_sec"],
            "addr": hex(disasm_addr) if disasm_addr is not None else None,
            "instruction_count": len(disasm_json),
            "first_instruction": disasm_json[0] if disasm_json else None,
            "error": None
            if disasm_result["status"] == "ok"
            else disasm_result.get("reason") or disasm_result.get("stderr") or disasm_result.get("stdout"),
        },
        "sections_seed_probe": {
            "status": sections_result["status"] if sections_result is not None else "not_run",
            "wall_clock_sec": sections_result["wall_clock_sec"] if sections_result is not None else 0.0,
        },
    }
    row_report["readiness_class"] = readiness_class(
        row_report["info"]["status"] == "ok",
        row_report["list"]["status"] == "ok",
        row_report["disasm"]["status"] == "ok",
    )
    row_report["seed_source"] = seed_source
    row_report["failure_bucket"] = failure_bucket
    row_report["failure_owner"] = failure_owner
    row_report["normalized_disasm_error"] = normalized_disasm_error
    row_report["classification_reason"] = classification_reason
    row_report["performance"] = {
        "total_wall_clock_sec": (
            row_report["info"]["wall_clock_sec"]
            + row_report["list"]["wall_clock_sec"]
            + row_report["disasm"]["wall_clock_sec"]
            + row_report["sections_seed_probe"]["wall_clock_sec"]
        )
    }

    report_path = row_dir / "architecture_readiness_report.json"
    report_path.write_text(json.dumps(row_report, indent=2, sort_keys=True) + "\n")
    row_report["report"] = str(report_path)
    return row_report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--cli-binary", type=Path, default=DEFAULT_CLI)
    parser.add_argument("--baseline-report", type=Path, default=None)
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args()

    manifest_path = args.manifest if args.manifest.is_absolute() else ROOT / args.manifest
    output_dir = args.output_dir if args.output_dir.is_absolute() else ROOT / args.output_dir
    cli_binary = args.cli_binary if args.cli_binary.is_absolute() else ROOT / args.cli_binary

    if not cli_binary.exists():
        raise SystemExit(
            f"fission_cli release binary not found at {cli_binary}. Run `cargo build -p fission-cli --release` first."
        )

    manifest = json.loads(manifest_path.read_text())
    entries = manifest.get("entries", [])
    output_dir.mkdir(parents=True, exist_ok=True)

    rows: list[dict[str, Any]] = []
    with ThreadPoolExecutor(max_workers=max(1, args.workers)) as executor:
        futures = [executor.submit(run_entry, entry, cli_binary, output_dir) for entry in entries]
        for future in as_completed(futures):
            rows.append(future.result())

    rows.sort(key=lambda row: row["id"])
    readiness_totals: dict[str, int] = {}
    failure_bucket_totals: dict[str, int] = {}
    failure_owner_totals: dict[str, int] = {}
    stage_totals = {
        "info_ok": 0,
        "list_ok": 0,
        "disasm_ok": 0,
    }
    total_wall_clock_sec = 0.0
    for row in rows:
        readiness_totals[row["readiness_class"]] = readiness_totals.get(row["readiness_class"], 0) + 1
        if row["info"]["status"] == "ok":
            stage_totals["info_ok"] += 1
        if row["list"]["status"] == "ok":
            stage_totals["list_ok"] += 1
        if row["disasm"]["status"] == "ok":
            stage_totals["disasm_ok"] += 1
        if row.get("failure_bucket"):
            failure_bucket_totals[row["failure_bucket"]] = failure_bucket_totals.get(row["failure_bucket"], 0) + 1
        if row.get("failure_owner"):
            failure_owner_totals[row["failure_owner"]] = failure_owner_totals.get(row["failure_owner"], 0) + 1
        total_wall_clock_sec += float(row["performance"]["total_wall_clock_sec"])

    aggregate = {
        "manifest": str(manifest_path),
        "row_count": len(rows),
        "readiness_totals": dict(sorted(readiness_totals.items())),
        "failure_bucket_totals": dict(sorted(failure_bucket_totals.items())),
        "failure_owner_totals": dict(sorted(failure_owner_totals.items())),
        "stage_totals": stage_totals,
        "performance_summary": {
            "total_wall_clock_sec": total_wall_clock_sec,
            "avg_wall_clock_sec_per_binary": total_wall_clock_sec / len(rows) if rows else None,
        },
        "rows": rows,
    }
    if args.baseline_report is not None:
        baseline_path = args.baseline_report if args.baseline_report.is_absolute() else ROOT / args.baseline_report
        baseline = json.loads(baseline_path.read_text())
        aggregate["comparison_to_baseline"] = build_comparison_to_baseline(
            aggregate,
            baseline,
            baseline_report=str(baseline_path),
        )
    aggregate_path = output_dir / "architecture_readiness_aggregate.json"
    aggregate_path.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    summary = {
        "report": str(aggregate_path),
        "row_count": aggregate["row_count"],
        "readiness_totals": aggregate["readiness_totals"],
        "failure_bucket_totals": aggregate["failure_bucket_totals"],
        "stage_totals": aggregate["stage_totals"],
        "performance_summary": aggregate["performance_summary"],
    }
    if "comparison_to_baseline" in aggregate:
        summary["comparison_to_baseline"] = aggregate["comparison_to_baseline"]
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
