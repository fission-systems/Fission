#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Compare the repository fixture natively, in Fission, and as exported Python."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import runpy
import shlex
import subprocess
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "crates/fission-dir/testdata/replay_fixture.c"
FLAGS = ["-O1", "-static", "-fno-inline", "-fno-stack-protector", "-fno-pie", "-no-pie",
         "-Wl,--build-id=none"]
BASELINE_FUNCTIONS = ("mix_word", "write_byte")
FUNCTIONS = (*BASELINE_FUNCTIONS, "narrowing_store")
INPUTS = (0, 1, 127, 128, 255, 256, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF)
INITIAL = [19, 83, 227]


def run(*command: str | Path) -> str:
    return subprocess.run(
        command, cwd=REPO, check=True, text=True, stdout=subprocess.PIPE, timeout=90,
    ).stdout


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def case_input(case: dict) -> tuple[str, int, list[int]]:
    function, value, initial = case["function"], case["input"], case["initial_bytes"]
    if (function not in FUNCTIONS or type(value) is not int or not 0 <= value < 2**32
            or not isinstance(initial, list) or len(initial) != 3
            or any(type(b) is not int or not 0 <= b <= 255 for b in initial)):
        raise ValueError("invalid repository fixture case")
    return function, value, initial


def observe(case: dict, runner: Path, fixture: Path, native: list[str], ported) -> dict:
    function, value, initial = case_input(case)
    args = [function, str(value), *(str(b) for b in initial)]
    result = json.loads(run(runner, "observe", fixture, *args))
    native_return, native_buffer = json.loads(run(*native, *args))
    result["native"] = {"return_value": native_return, "buffer": native_buffer, "outside_writes": []}
    if function == "mix_word":
        result["python"] = {"return_value": ported(value) & 0xFFFFFFFF, "buffer": initial,
                            "outside_writes": []}
    return result


def agrees(result: dict) -> bool:
    lanes = ("emulator", "hir") + (("python",) if "python" in result else ())
    return all(result[lane] == result["native"] for lane in lanes)


def save_witness(path: Path, case: dict, identity: dict, result: dict) -> None:
    with path.open("x") as stream:
        json.dump({"version": 1, **identity, **case, "observations": result}, stream, indent=2)
        stream.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True, help="built concrete_replay example")
    parser.add_argument("--cc", default="cc", help="compiler command, e.g. 'zig cc -target x86_64-linux-musl'")
    parser.add_argument("--linux-container", help="run the fixed native fixture in this linux/amd64 Docker image")
    parser.add_argument("--witness", type=Path, default=Path("replay-witness.json"))
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--replay", type=Path)
    mode.add_argument("--function", choices=FUNCTIONS)
    parser.add_argument("--python-out", type=Path)
    args = parser.parse_args()
    if not args.linux_container and (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        parser.error("native comparison requires Linux x86-64 or --linux-container")

    functions = (args.function,) if args.function else BASELINE_FUNCTIONS
    case = json.loads(args.replay.read_text()) if args.replay else None
    if case is not None:
        if case.get("version") != 1:
            parser.error("invalid replay case version")
        case_input(case)

    with tempfile.TemporaryDirectory(prefix="fission-replay-") as directory:
        work = Path(directory)
        fixture = work / "fixture"
        compiler = shlex.split(args.cc)
        version = run(*compiler, "--version").splitlines()[0]
        run(*compiler, *FLAGS, SOURCE, "-o", fixture)
        identity = {
            "fixture_sha256": digest(fixture),
            "source_sha256": digest(SOURCE),
            "compiler": version,
            "compiler_command": compiler,
            "runner_sha256": digest(args.runner.resolve()),
            "sleigh_manifest_sha256": digest(REPO / "utils/sleigh-specs/MANIFEST.sha256.json"),
            "compiler_flags": FLAGS,
            "abi": "x86_64-linux-sysv",
            "fission_revision": run("git", "rev-parse", "HEAD").strip(),
        }
        if case is not None and (
            case["fixture_sha256"] != identity["fixture_sha256"]
            or case["source_sha256"] != identity["source_sha256"]
        ):
            parser.error("fixture identity differs from the witness; use its compiler and source revision")
        native = [str(fixture)]
        if args.linux_container:
            native = [
                "docker", "run", "--rm", "--platform", "linux/amd64", "--network", "none",
                "--volume", f"{fixture}:/fixture:ro", "--entrypoint", "/fixture",
                args.linux_container,
            ]

        source = run(args.runner.resolve(), "python", fixture, "mix_word")
        module = work / "ported.py"
        module.write_text(source)
        ported = runpy.run_path(str(module))["ported"]
        if args.python_out:
            with args.python_out.open("x") as output:
                output.write(source)

        cases = [case] if case is not None else [
            {"function": function, "input": value, "initial_bytes": INITIAL}
            for function in functions for value in INPUTS
        ]
        for current in cases:
            result = observe(current, args.runner.resolve(), fixture, native, ported)
            if not agrees(result):
                if case is None:
                    save_witness(args.witness, current, identity, result)
                    print(f"Wrote {args.witness}")
                print(json.dumps(result))
                return 1
        print(f"Agreement on {len(cases)} cases (native, emulator, printed HIR; Python for mix_word)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
