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
QUOTIENT = "crates/fission-quotient/testdata/quotient_fixture.elf"
QUOTIENT_A64 = "crates/fission-quotient/testdata/quotient_fixture_aarch64.elf"


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
        # One case per status the payload can carry.
        ("quotient", [(QUOTIENT, "--name", "auth_step"), (QUOTIENT_A64, "--name", "auth_step"),
                      (QUOTIENT, "--name", "split_step"),
                      (QUOTIENT, "--name", "counter_step", "--max-states", "4"),
                      (QUOTIENT, "--name", "latch_step", "--conflict-budget", "0"),
                      (QUOTIENT, "--name", "clock_step")]),
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
        selector = {"decomp": ("--all",), "quotient": ("--addr", "0x1000")}.get(command, ())
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
        if command == "quotient":
            # A status the contract does not define must not validate: a
            # reader branches on `kind` to know how far the claim reaches.
            claim = copy.deepcopy(result)
            claim["data"]["status"] = {"kind": "proved", "horizon": 4}
            if validator.is_valid(claim):
                raise RuntimeError("quotient: an undefined status kind was accepted")
        broken = copy.deepcopy(result)
        broken["data"] = True
        if validator.is_valid(broken):
            raise RuntimeError(f"{command}: payload type drift was not detected")
        print(f"{command}: checked {len(cases)} success responses, a CLI error, and a rejected mutation")


if __name__ == "__main__":
    check(str(Path(sys.argv[1]).resolve()))
