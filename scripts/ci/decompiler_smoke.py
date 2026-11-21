"""Run the release CLI contract on a known-good, non-executed fixture."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def run(cli: Path, *args: str, workers: int | None = None, timeout: int = 180) -> str:
    env = dict(os.environ)
    if workers is not None:
        env["FISSION_RUST_DECOMP_WORKERS"] = str(workers)
        env["FISSION_BENCHMARK_NO_CACHE"] = "1"
    result = subprocess.run(
        [str(cli), *args], check=True, capture_output=True, text=True, timeout=timeout, env=env
    )
    if not result.stdout.strip():
        raise ValueError(f"{args[0]} returned empty output")
    return result.stdout


def check(cli: Path, fixture: Path, symbol: str | None) -> None:
    name = str(fixture)
    run(cli, "info", name)
    functions = json.loads(run(cli, "list", name, "--json"))
    candidates = [item for item in functions if not item.get("is_import")]
    if symbol is not None:
        candidates = [item for item in candidates if item.get("name", "").lstrip("_") == symbol]
    address = next((item["address"] for item in candidates if item.get("address")), None)
    if not address:
        raise ValueError(f"{fixture.name}: no address for {symbol or 'a defined function'}")
    run(cli, "raw-pcode", name, "--addr", address)
    output = run(cli, "decomp", name, "--addr", address, "--json", "--timeout-ms", "120000")
    entry = json.loads(output)
    if isinstance(entry, list):
        entry = entry[0] if entry else {}
    if entry.get("error") or not entry.get("code", "").strip():
        raise ValueError(f"{fixture.name}: decomp returned no C at {address}")
    print(f"{fixture.name}: info, list, raw-pcode and decomp succeeded at {address}")
    check_determinism(cli, fixture)


def decompile_codes(
    cli: Path, binary: Path, workers: int, *, resources: Path | None = None, timeout: int = 180,
) -> dict[str, str]:
    prefix = ("--resource-root", str(resources)) if resources is not None else ()
    output = json.loads(run(
        cli, *prefix, "--no-db", "decomp", str(binary), "--all", "--json", "--benchmark",
        "--no-header", "--no-warnings", workers=workers, timeout=timeout,
    ))
    entries = output["functions"]
    if not entries:
        raise ValueError(f"{binary.name}: no functions were decompiled")
    expected = min(workers, len(entries))
    actual = output["_meta"]["worker_count"]
    if actual != expected:
        raise ValueError(f"{binary.name}: CLI reported {actual} workers, expected {expected}")
    codes = {}
    for entry in entries:
        address = str(entry["address"])
        if address in codes:
            raise ValueError(f"{binary.name}: duplicate function address {address}")
        codes[address] = hashlib.sha256(str(entry.get("code")).encode()).hexdigest()
    return codes


def check_determinism(cli: Path, fixture: Path) -> None:
    """The same input must decompile to the same text, whatever the thread
    count (AGENTS.md core rule 4: outputs feed snapshots and benchmarks)."""
    runs = [decompile_codes(cli, fixture, workers) for workers in (1, 4, 4)]
    differing = sorted(
        address for address in runs[0] if len({r.get(address) for r in runs}) != 1
    )
    if differing or runs[0].keys() != runs[1].keys() or runs[0].keys() != runs[2].keys():
        raise ValueError(f"{fixture.name}: output differs between runs at {differing[:10]}")
    print(f"{fixture.name}: {len(runs[0])} functions decompile identically over 3 runs")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=Path)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--symbol")
    args = parser.parse_args()
    try:
        check(args.cli.resolve(), args.fixture.resolve(), args.symbol)
    except subprocess.CalledProcessError as error:
        print(error.stderr, file=sys.stderr)
        raise
