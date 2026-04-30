#!/usr/bin/env python3
"""Orchestrate loader smoke, raw p-code, and full benchmark runs."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACT_ROOT = REPO_ROOT / "benchmark/artifacts/realworld_suite"
DEFAULT_FISSION = REPO_ROOT / "target/release/fission_cli"


def run_step(name: str, command: list[str], cwd: Path, continue_on_error: bool) -> dict[str, Any]:
    start = time.perf_counter()
    proc = subprocess.run(command, cwd=str(cwd), text=True, capture_output=True)
    elapsed = time.perf_counter() - start
    row = {
        "name": name,
        "command": command,
        "returncode": proc.returncode,
        "elapsed_sec": elapsed,
        "stdout_tail": proc.stdout[-8000:],
        "stderr_tail": proc.stderr[-8000:],
    }
    if proc.returncode != 0 and not continue_on_error:
        raise RuntimeError(f"{name} failed with exit {proc.returncode}\n{proc.stderr[-2000:]}")
    return row


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def has_non_executable_preflight_failures(report: dict[str, Any]) -> bool:
    counts = report.get("failure_bucket_counts") or {}
    if not isinstance(counts, dict):
        return False
    return bool(
        counts.get("container_requires_extraction")
        or counts.get("unsupported_loader_family")
        or counts.get("unsupported_format")
        or counts.get("load_spec")
    )


def same_path(lhs: Path | None, rhs: Path | None) -> bool:
    if lhs is None or rhs is None:
        return False
    return lhs.expanduser().resolve() == rhs.expanduser().resolve()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_ARTIFACT_ROOT / time.strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION)
    parser.add_argument("--ghidra-dir", type=Path, default=REPO_ROOT / "vendor/ghidra/ghidra_12.0.4_PUBLIC")
    parser.add_argument("--loader-manifest", type=Path, help="Manifest for loader smoke.")
    parser.add_argument("--raw-pcode-manifest", type=Path, help="Manifest for raw p-code parity.")
    parser.add_argument("--full-corpus-manifest", type=Path, help="Manifest for full benchmark.")
    parser.add_argument("--build-cli", action="store_true")
    parser.add_argument("--fission-release", action="store_true", help="Pass --fission-release to raw p-code runner.")
    parser.add_argument(
        "--no-loader-preflight-skip",
        action="store_true",
        help="Do not skip raw/full lanes when the same manifest has non-executable loader preflight failures.",
    )
    parser.add_argument("--full-limit", type=int)
    parser.add_argument("--continue-on-error", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    steps: list[dict[str, Any]] = []
    skipped_steps: list[dict[str, Any]] = []
    loader_report: dict[str, Any] | None = None

    def add_step(name: str, command: list[str]) -> None:
        print(f"[*] {name}: {' '.join(command)}")
        steps.append(run_step(name, command, REPO_ROOT, args.continue_on_error))

    if args.build_cli:
        add_step("build_cli_release", ["cargo", "build", "--release", "-p", "fission-cli"])
    if args.loader_manifest:
        loader_output_dir = args.output_dir / "loader_smoke"
        add_step(
            "loader_smoke",
            [
                "python3",
                "scripts/benchmark/run_loader_smoke.py",
                "--manifest",
                str(args.loader_manifest),
                "--fission-bin",
                str(args.fission_bin),
                "--output-dir",
                str(loader_output_dir),
            ],
        )
        report_path = loader_output_dir / "loader_smoke_report.json"
        if report_path.is_file():
            loader_report = load_json(report_path)
    should_skip_same_manifest = (
        loader_report is not None
        and has_non_executable_preflight_failures(loader_report)
        and not args.no_loader_preflight_skip
    )
    if args.raw_pcode_manifest:
        if should_skip_same_manifest and same_path(args.raw_pcode_manifest, args.loader_manifest):
            skipped_steps.append(
                {
                    "name": "raw_pcode",
                    "reason": "skipped_raw_pcode_due_to_loader_preflight",
                    "manifest": str(args.raw_pcode_manifest),
                }
            )
        else:
            command = [
                "python3",
                "benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py",
                "--manifest",
                str(args.raw_pcode_manifest),
                "--ghidra-dir",
                str(args.ghidra_dir),
                "--output-dir",
                str(args.output_dir / "raw_pcode"),
            ]
            if args.fission_release:
                command.append("--fission-release")
            add_step("raw_pcode", command)
    if args.full_corpus_manifest:
        if should_skip_same_manifest and same_path(args.full_corpus_manifest, args.loader_manifest):
            skipped_steps.append(
                {
                    "name": "full_benchmark",
                    "reason": "skipped_full_benchmark_due_to_loader_preflight",
                    "manifest": str(args.full_corpus_manifest),
                }
            )
        else:
            command = [
                "python3",
                "benchmark/full_benchmark/full_decomp_benchmark.py",
                "--corpus-manifest",
                str(args.full_corpus_manifest),
                "--ghidra-dir",
                str(args.ghidra_dir),
                "--fission-bin",
                str(args.fission_bin),
                "--output-dir",
                str(args.output_dir / "full_benchmark"),
            ]
            if args.full_limit is not None:
                command.extend(["--limit", str(args.full_limit)])
            add_step("full_benchmark", command)

    summary = {
        "output_dir": str(args.output_dir),
        "status": "ok" if all(step["returncode"] == 0 for step in steps) else "failed",
        "steps": steps,
        "skipped_steps": skipped_steps,
    }
    summary_path = args.output_dir / "suite_report.json"
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(summary_path), "status": summary["status"]}, indent=2))
    return 0 if summary["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
