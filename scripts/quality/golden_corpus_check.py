#!/usr/bin/env python3
"""Fast local regression gate: no Docker, no Ghidra reference.

Batch-decompiles a curated corpus of functions with the local `fission_cli`
build and diffs NIR/HIR text against a checked-in golden snapshot. This is
NOT the official quality oracle (see `fission-benchmark` docker parity
suite / `pr_canary.sh` for that) -- it is a cheap, broad-coverage "did this
change anything" gate meant to run in well under a minute on every
migration/perf slice, replacing hand-typed `decomp` + `diff` rituals.

Workflow
--------
  # After verifying a change is intentional/correct:
  python3 scripts/quality/golden_corpus_check.py update

  # On every future change, before/after a rebuild:
  cargo build -p fission-cli --profile quick-release
  python3 scripts/quality/golden_corpus_check.py check

`check` also re-runs a few known-heavy functions several times to catch
run-to-run nondeterminism (see PROJECT.md's determinism notes) without
needing the full Docker suite.

Snapshot: scripts/quality/golden_corpus/snapshot.json (checked into git --
regressions show as a diff on `check`; intentional changes show as a
reviewable diff on the `update` commit).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
SNAPSHOT_PATH = REPO_ROOT / "scripts" / "quality" / "golden_corpus" / "snapshot.json"
DEFAULT_BENCHMARK_ROOT = REPO_ROOT.parent / "fission-benchmark"
CORPUS_SUBDIR = "corpus/dev/binaries/c"

# One binary per (program family x compiler/opt) axis we most care about
# breaking: gcc -O0 (baseline codegen) and gcc -O2 (optimized, most
# structuring pressure). Kept deliberately small -- measured ~1s/function
# average, so this (8 families x 2 variants x limit 10 = up to 160
# functions) targets a few minutes, not the docker parity suite's tens of
# minutes. Widen via --binaries/--limit for a broader (slower) sweep.
DEFAULT_BINARIES = [
    f"{family}_{variant}.exe"
    for family in (
        "advanced_patterns",
        "control_flow",
        "crypto",
        "data_structures",
        "math",
        "memory_layouts",
        "semantic_stress",
        "string_utils",
    )
    for variant in ("gcc_O0", "gcc_O2")
]

# Functions worth re-running several times per `check` to catch run-to-run
# nondeterminism cheaply (see PROJECT.md: hash-iteration-order bugs, and the
# wall-clock SESE_REGION_PROOF_BUDGET_MS follow-up). (binary, addr) pairs.
DETERMINISM_TARGETS = [
    ("semantic_stress_gcc_O3.exe", "0x140001560"),  # bounded_tlv_sum
    ("semantic_stress_gcc-m32_O2.exe", "0x00401680"),  # state_machine_score
]

DEFAULT_LIMIT = 10
DEFAULT_TIMEOUT_MS = 20000


def default_cli() -> Path:
    quick = REPO_ROOT / "target" / "quick-release" / "fission_cli"
    if quick.exists():
        return quick
    return REPO_ROOT / "target" / "release" / "fission_cli"


def resolve_cli(cli_arg: str | None) -> Path:
    path = Path(cli_arg) if cli_arg else default_cli()
    if not path.exists():
        print(f"error: fission_cli not found at {path}", file=sys.stderr)
        print(
            "  build it first: cargo build -p fission-cli --profile quick-release",
            file=sys.stderr,
        )
        sys.exit(1)
    return path


def decompile_all(
    cli: Path, binary: Path, limit: int, timeout_ms: int
) -> list[dict[str, Any]]:
    proc = subprocess.run(
        [
            str(cli),
            "decomp",
            "--all",
            "--limit",
            str(limit),
            "--timeout-ms",
            str(timeout_ms),
            "--benchmark",
            "--json",
            "--layer",
            "both",
            str(binary),
        ],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
    )
    if proc.returncode != 0:
        print(f"error: decomp --all failed for {binary.name}", file=sys.stderr)
        print(proc.stderr[-2000:], file=sys.stderr)
        sys.exit(1)
    return json.loads(proc.stdout)["functions"]


def decompile_one(cli: Path, binary: Path, addr: str) -> str:
    proc = subprocess.run(
        [str(cli), "decomp", "--addr", addr, "--layer", "nir", str(binary)],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
    )
    if proc.returncode != 0:
        print(f"error: decomp --addr {addr} failed for {binary.name}", file=sys.stderr)
        print(proc.stderr[-2000:], file=sys.stderr)
        sys.exit(1)
    return proc.stdout


def build_snapshot(
    cli: Path, benchmark_root: Path, binaries: list[str], limit: int, timeout_ms: int
) -> dict[str, Any]:
    snapshot: dict[str, Any] = {}
    corpus_dir = benchmark_root / CORPUS_SUBDIR
    for name in binaries:
        binary_path = corpus_dir / name
        if not binary_path.exists():
            print(f"warning: binary not found, skipping: {binary_path}", file=sys.stderr)
            continue
        functions = decompile_all(cli, binary_path, limit, timeout_ms)
        entry: dict[str, Any] = {}
        for fn in functions:
            key = f"{fn['name']}@{fn['address']}"
            entry[key] = {
                "code_nir": fn.get("code_nir") or fn.get("code"),
                "code_hir": fn.get("code_hir"),
            }
        snapshot[name] = entry
        print(f"  {name}: {len(entry)} functions", file=sys.stderr)
    return snapshot


def cmd_update(args: argparse.Namespace) -> int:
    cli = resolve_cli(args.cli)
    benchmark_root = Path(args.benchmark_root)
    binaries = args.binaries or DEFAULT_BINARIES
    print(f"using CLI: {cli}", file=sys.stderr)
    print(f"building snapshot from {len(binaries)} binaries...", file=sys.stderr)
    fresh = build_snapshot(cli, benchmark_root, binaries, args.limit, args.timeout_ms)

    # Merge into any existing snapshot so a scoped `--binaries` update (e.g.
    # only re-running the families a change could plausibly affect) doesn't
    # silently drop every other binary's golden entries.
    snapshot: dict[str, Any] = {}
    if SNAPSHOT_PATH.exists() and not args.replace:
        snapshot = json.loads(SNAPSHOT_PATH.read_text())
    snapshot.update(fresh)

    total = sum(len(v) for v in snapshot.values())
    SNAPSHOT_PATH.parent.mkdir(parents=True, exist_ok=True)
    SNAPSHOT_PATH.write_text(json.dumps(snapshot, indent=1, sort_keys=True) + "\n")
    print(f"wrote {SNAPSHOT_PATH} ({total} functions across {len(snapshot)} binaries)")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    if not SNAPSHOT_PATH.exists():
        print(f"error: no snapshot at {SNAPSHOT_PATH}. Run `update` first.", file=sys.stderr)
        return 1
    cli = resolve_cli(args.cli)
    benchmark_root = Path(args.benchmark_root)
    golden = json.loads(SNAPSHOT_PATH.read_text())
    binaries = args.binaries or list(golden.keys())

    print(f"using CLI: {cli}", file=sys.stderr)
    print(f"checking {len(binaries)} binaries against golden snapshot...", file=sys.stderr)

    failures: list[str] = []
    total_checked = 0
    corpus_dir = benchmark_root / CORPUS_SUBDIR
    for name in binaries:
        binary_path = corpus_dir / name
        if not binary_path.exists():
            print(f"warning: binary not found, skipping: {binary_path}", file=sys.stderr)
            continue
        golden_entry = golden.get(name, {})
        functions = decompile_all(cli, binary_path, args.limit, args.timeout_ms)
        current_keys = set()
        for fn in functions:
            key = f"{fn['name']}@{fn['address']}"
            current_keys.add(key)
            total_checked += 1
            current = {
                "code_nir": fn.get("code_nir") or fn.get("code"),
                "code_hir": fn.get("code_hir"),
            }
            expected = golden_entry.get(key)
            if expected is None:
                failures.append(f"{name}::{key} -- NEW function not in snapshot")
                continue
            if current["code_nir"] != expected["code_nir"]:
                failures.append(f"{name}::{key} -- NIR mismatch")
            if current["code_hir"] != expected["code_hir"]:
                failures.append(f"{name}::{key} -- HIR mismatch")
        missing = set(golden_entry.keys()) - current_keys
        for key in missing:
            failures.append(f"{name}::{key} -- MISSING from current output")

    print(f"checked {total_checked} functions across {len(binaries)} binaries", file=sys.stderr)

    if not args.skip_determinism:
        print("checking determinism (repeat-run stability)...", file=sys.stderr)
        for binary_name, addr in DETERMINISM_TARGETS:
            binary_path = corpus_dir / binary_name
            if not binary_path.exists():
                continue
            outputs = {
                decompile_one(cli, binary_path, addr) for _ in range(args.determinism_runs)
            }
            if len(outputs) > 1:
                failures.append(
                    f"{binary_name}@{addr} -- NONDETERMINISTIC: "
                    f"{len(outputs)} distinct outputs across {args.determinism_runs} runs"
                )

    if failures:
        print(f"\nFAIL: {len(failures)} issue(s):\n", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        print(
            "\nIf these changes are intentional, review them (e.g. via "
            "local_decomp_observe.py) then run `update` to accept the new "
            "snapshot.",
            file=sys.stderr,
        )
        return 1

    print("\nPASS: golden corpus matches, determinism holds.")
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--cli", default=None, help="Path to fission_cli (default: quick-release, falling back to release)")
    common.add_argument(
        "--benchmark-root",
        default=str(DEFAULT_BENCHMARK_ROOT),
        help=f"fission-benchmark repo root (default: {DEFAULT_BENCHMARK_ROOT})",
    )
    common.add_argument("--binaries", nargs="*", default=None, help="Override the binary list")
    common.add_argument("--limit", type=int, default=DEFAULT_LIMIT, help="Max functions per binary")
    common.add_argument("--timeout-ms", type=int, default=DEFAULT_TIMEOUT_MS, help="Per-function decomp timeout")

    p_update = sub.add_parser("update", parents=[common], help="Rebuild the golden snapshot from the current CLI build")
    p_update.add_argument(
        "--replace",
        action="store_true",
        help="Discard the existing snapshot instead of merging (default merges, so a scoped --binaries run doesn't drop other binaries' entries)",
    )
    p_update.set_defaults(func=cmd_update)

    p_check = sub.add_parser("check", parents=[common], help="Diff current output against the golden snapshot")
    p_check.add_argument("--skip-determinism", action="store_true", help="Skip the repeat-run determinism check")
    p_check.add_argument("--determinism-runs", type=int, default=5, help="Repeat count per determinism target")
    p_check.set_defaults(func=cmd_check)

    return p


def main() -> int:
    args = build_parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
