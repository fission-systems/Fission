"""Run the release CLI contract on a known-good, non-executed fixture."""

import argparse
import json
import subprocess
import sys
from pathlib import Path


def run(cli: Path, *args: str) -> str:
    result = subprocess.run([str(cli), *args], check=True, capture_output=True, text=True, timeout=180)
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
