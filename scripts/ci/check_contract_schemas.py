#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Validate the public contract schemas against real CLI responses."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import sys

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[2]
PE = "crates/fission-loader/testdata/x64_pdb_struct_test.exe"
ELF = "crates/fission-dir/testdata/ground_truth_fixture.elf"
SHARED = "crates/fission-sleigh/testdata/a64_csel_select.so"


def invoke(cli: str, *args: str, status: int = 0) -> dict:
    result = subprocess.run(
        [cli, *args], capture_output=True, text=True, cwd=ROOT, timeout=60,
    )
    if result.returncode != status:
        raise RuntimeError(f"{args}: expected exit {status}, got {result.returncode}\n{result.stderr}")
    return json.loads(result.stdout)


def check(cli: str) -> None:
    for command, cases in (
        ("info", [(PE,), (PE, "--sections"), (PE, "--identity"), (ELF, "--imports"),
                  (SHARED, "--exports")]),
        ("list", [(PE,), (ELF,)]),
        ("decomp", [(PE, "--addr", "0x140001040"), (PE, "--all"),
                    (PE, "--all", "--benchmark")]),
    ):
        schema = invoke(cli, "schema", command)
        Draft202012Validator.check_schema(schema)
        committed = json.loads((ROOT / f"docs/schemas/cli-{command}-data.v1.schema.json").read_text())
        committed.pop("$schema")
        if schema["properties"]["data"] != committed:
            raise RuntimeError(f"{command}: CLI schema differs from the committed schema")
        validator = Draft202012Validator(schema)
        for case in cases:
            result = invoke(cli, "--contract", "v1", "--no-db", command, *case, "--json")
            validator.validate(result)
        selector = ("--all",) if command == "decomp" else ()
        error = invoke(
            cli, "--contract", "v1", command, "missing-fission-fixture.elf",
            *selector, "--json", status=3,
        )
        validator.validate(error)
        if error["ok"]:
            raise RuntimeError(f"{command}: missing input succeeded")
        if command == "decomp":
            partial = copy.deepcopy(result)
            partial["ok"] = False
            partial["error"] = {"code": "partial_failure", "kind": "partial",
                                "message": "sample item failed", "retryable": False}
            rows = partial["data"]["functions"]
            rows[0].pop("code")
            rows[0]["error"] = "sample item failed"
            validator.validate(partial)
        broken = copy.deepcopy(result)
        broken["data"] = True
        if validator.is_valid(broken):
            raise RuntimeError(f"{command}: payload type drift was not detected")
        print(f"{command}: checked {len(cases)} success responses, a CLI error, and a rejected mutation")


if __name__ == "__main__":
    check(str(Path(sys.argv[1]).resolve()))
