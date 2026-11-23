#!/usr/bin/env python3
"""Compare two CLIs' decompile cost by instructions executed, not seconds.

Wall time on a shared CI runner moves 10-20% between identical runs, more
than most regressions. Instructions executed (valgrind cachegrind, `Ir`)
measure the work itself. Measured on one fixture, eight runs, one thread:
the first run is ~3% high (a cold start), the rest within 1.3% of each other.
So each binary is decompiled once to warm up and then RUNS times, and the
minimum is kept; both CLIs run on the same runner in the same job, because
library code can pick different instructions on different CPUs.

The sample is fixed by rule: the first BINARIES manifest binaries in sha256
order, LIMIT functions each (`decomp --all --limit`), single-threaded and
static. That bounds the cost: under valgrind one run of 40 functions took
25-76 s, and start-up alone (load, SLEIGH frontend) is ~3.7e9 instructions,
which stays in the count on purpose -- it is paid on every CLI call. Linux only.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "quality"))
import decbench_full_sweep as sweep  # noqa: E402

RUNS = 2
BINARIES = 6
LIMIT = 15
THRESHOLD = 0.03


def instructions(cli: Path, binary: Path, resources: Path, cache: Path | None = None) -> int:
    env = dict(os.environ, RAYON_NUM_THREADS="1", FISSION_BENCHMARK_NO_CACHE="1")
    if cache is not None:
        # Each CLI compiles its own SLEIGH frontends: a shared cache would hand
        # head the frontends base compiled and hide a SLEIGH compiler change.
        env["FISSION_SLEIGH_CACHE_DIR"] = str(cache)
    with tempfile.TemporaryDirectory() as temp:
        out = Path(temp) / "cachegrind.out"
        subprocess.run(
            ["valgrind", "--tool=cachegrind", "--cache-sim=no", f"--cachegrind-out-file={out}",
             str(cli), "--resource-root", str(resources), "--no-db", "decomp", str(binary),
             "--all", "--limit", str(LIMIT), "--json", "--no-header", "--no-warnings"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env, check=True, timeout=3600,
        )
        for line in out.read_text().splitlines():
            if line.startswith("summary:"):
                return int(line.split()[1])
    raise RuntimeError(f"no cachegrind summary for {binary}")


def cost(cli: Path, binary: Path, resources: Path, runs: int = RUNS, cache: Path | None = None) -> int:
    instructions(cli, binary, resources, cache)  # warm-up, discarded
    return min(instructions(cli, binary, resources, cache) for _ in range(runs))


def compare(rows: list[dict], threshold: float = THRESHOLD) -> dict:
    base = sum(r["base"] for r in rows)
    head = sum(r["head"] for r in rows)
    change = (head - base) / base if base else 0.0
    slower = [r["binary"] for r in rows if r["base"] and (r["head"] - r["base"]) / r["base"] >= threshold]
    return {"base": base, "head": head, "change": change, "regressed": change >= threshold,
            "slower_binaries": slower, "rows": rows, "threshold": threshold}


def markdown(report: dict, title: str) -> str:
    lines = [f"### {title}", "",
             f"Total instructions: {report['base']:,} → {report['head']:,} "
             f"(**{report['change']:+.2%}**; flagged at +{report['threshold']:.0%})", "",
             "| binary | functions | base Ir | head Ir | change |", "|---|---|---|---|---|"]
    for r in report["rows"]:
        delta = (r["head"] - r["base"]) / r["base"] if r["base"] else 0.0
        lines.append(f"| `{r['binary']}` | {r['functions']} | {r['base']:,} | {r['head']:,} | {delta:+.2%} |")
    lines += ["", "Instructions executed under valgrind cachegrind, single-threaded, minimum of "
              f"{RUNS} runs after a warm-up; identical runs differ by up to ~1.3%."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--base-cli", type=Path, required=True)
    parser.add_argument("--head-cli", type=Path, required=True)
    parser.add_argument("--base-resources", type=Path)
    parser.add_argument("--head-resources", type=Path)
    parser.add_argument("--binaries", type=int, default=BINARIES)
    parser.add_argument("--title", default="Decompile cost (instructions executed)")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--fail-on-regression", action="store_true")
    args = parser.parse_args()

    head_resources = args.head_resources or sweep.REPO / "utils"
    base_resources = args.base_resources or head_resources
    caches = Path(tempfile.mkdtemp(prefix="speed-sleigh-cache-"))
    rows = []
    entries = sorted(sweep.manifest(args.data), key=lambda e: e["sha256"])[: args.binaries]
    for entry in entries:
        binary = sweep.fetch(entry["binary_path"], args.data)
        if binary.stat().st_size != entry["size"] or sweep.digest(binary) != entry["sha256"]:
            raise SystemExit(f"dataset binary hash or size differs for {entry['binary_path']}")
        row = {"binary": entry["binary_path"], "functions": LIMIT,
               "base": cost(args.base_cli, binary, base_resources, cache=caches / "base"),
               "head": cost(args.head_cli, binary, head_resources, cache=caches / "head")}
        rows.append(row)
        print(f"{row['binary']}: {row['base']:,} -> {row['head']:,}", flush=True)
    report = compare(rows)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "speed.json").write_text(json.dumps(report, indent=2) + "\n")
    text = markdown(report, args.title)
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    if report["regressed"]:
        print(f"::warning title=Decompile cost::{report['change']:+.2%} instructions executed over the sample")
        if args.fail_on_regression:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
