"""Run the release CLI contract on a known-good, non-executed fixture."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def run(cli: Path, *args: str, threads: int | None = None) -> str:
    env = dict(os.environ)
    if threads is not None:
        env["RAYON_NUM_THREADS"] = str(threads)
    result = subprocess.run(
        [str(cli), *args], check=True, capture_output=True, text=True, timeout=180, env=env
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


def code_by_address(output: str) -> dict[str, str]:
    entries = json.loads(output)
    entries = entries if isinstance(entries, list) else [entries]
    return {
        str(entry.get("address")): hashlib.sha256(str(entry.get("code")).encode()).hexdigest()
        for entry in entries
    }


def check_determinism(cli: Path, fixture: Path) -> None:
    """The same input must decompile to the same text, whatever the thread
    count (AGENTS.md core rule 4: outputs feed snapshots and benchmarks)."""
    args = ("decomp", str(fixture), "--all", "--json", "--no-header", "--no-warnings")
    runs = [code_by_address(run(cli, *args, threads=threads)) for threads in (1, 4, 4)]
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
