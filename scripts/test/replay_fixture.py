#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Compare the repository fixture natively, in Fission, and as exported Python.

Includes an offline oracle dependency matrix and fault-injection calibration suite
(`--calibrate`) verifying that comparison boundaries and witness replay never mask
shared wrong lifts, missing/truncated traces, stale witnesses, or harness errors.
"""

from __future__ import annotations

import argparse
import copy
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
FLAGS = [
    "-O1",
    "-static",
    "-fno-inline",
    "-fno-stack-protector",
    "-fno-pie",
    "-no-pie",
    "-Wl,--build-id=none",
]
BASELINE_FUNCTIONS = ("mix_word", "write_byte")
FUNCTIONS = (*BASELINE_FUNCTIONS, "narrowing_store")
INPUTS = (0, 1, 127, 128, 255, 256, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF)
INITIAL = [19, 83, 227]
EXPECTED_BUFFER_LEN = len(INITIAL)

# Documented dependency graph and blind spots across verification lanes (#1254).
# Agreement between two lanes that share a dependency (such as SLEIGH or eval.rs)
# is bounded common-mode evidence, never independent ground-truth confirmation.
ORACLE_DEPENDENCY_MATRIX = {
    "concrete_prehir_hir": {
        "isa_spec": "shared (cspec/SLEIGH)",
        "lifter": "shared (Ghidra/Rust SLEIGH -> PreHIR -> HIR)",
        "evaluator": "shared (fission-dir/src/eval.rs interpret_prehir vs interpret_hir)",
        "fixture_generation": "boundary cross-product samples (diff::default_samples)",
        "comparator": "diff_prehir_hir_detailed (exact scalar return comparison)",
        "initial_state_mapping": "register/frame defaults + explicit concrete args",
        "independent_of_fission_lift": False,
        "evidence_strength": "structuring_and_lowering_regression_only",
        "common_mode_blind_spots": [
            "shared wrong SLEIGH semantics / shared wrong PreHIR->HIR lift",
            "memory/call side effects in pure concrete scalar diff mode",
            "unsampled inputs outside default_samples",
        ],
    },
    "ground_truth_emulator": {
        "isa_spec": "shared (fission-sleigh / emulator cspec)",
        "lifter": "emulator SLEIGH/JIT vs PreHIR/HIR",
        "evaluator": "fission-emulator vs fission-dir/src/eval.rs",
        "fixture_generation": "deterministic PRNG InitialState runs (DEFAULT_SEED)",
        "comparator": "check_ground_truth_with (ret, calls, arg_bits, stores, undefined frame)",
        "initial_state_mapping": "InitialState registers/frame mapped to ABI slots & scratch",
        "independent_of_fission_lift": False,
        "evidence_strength": "sampled_emulator_consistency",
        "common_mode_blind_spots": [
            "shared SLEIGH semantic defects between emulator and decompiler",
            "host/hardware divergence outside modeled userops/HLE",
            "unsampled branches/inputs",
        ],
    },
    "symbolic_prehir_hir": {
        "isa_spec": "shared (cspec/SLEIGH)",
        "lifter": "shared (PreHIR/HIR symbolic lowering in lower_sym.rs)",
        "evaluator": "fission-solver (bitvector AIG/CDCL + z3 differential)",
        "fixture_generation": "exhaustive bitvector encoding for modeled acyclic domain",
        "comparator": "check_symbolic_equivalence (negated equality query)",
        "initial_state_mapping": "explicit exact-width live-in bindings + initial domain",
        "independent_of_fission_lift": False,
        "evidence_strength": "bounded_symbolic_equivalence_within_modeled_subset",
        "common_mode_blind_spots": [
            "shared PreHIR+HIR wrong semantics before symbolic lowering",
            "loops, calls, and memory ops rejected as unsupported/inconclusive",
        ],
    },
    "native_replay_fixture": {
        "isa_spec": "independent compiled ISA reference (cc on Linux x86-64)",
        "lifter": "host compiler + hardware CPU vs emulator vs printed HIR vs exported Python",
        "evaluator": "hardware native binary vs emulator vs eval::run_hir vs python3",
        "fixture_generation": "checked-in replay_fixture.c over fixed boundary inputs",
        "comparator": "replay_fixture.py classify_observation (return_value, buffer, outside_writes)",
        "initial_state_mapping": "sysv-abi args + 3-byte initial scratch buffer",
        "independent_of_fission_lift": True,
        "evidence_strength": "independent_sampled_native_ground_truth",
        "common_mode_blind_spots": [
            "limited to sampled inputs in INPUTS and fixed fixture functions",
            "requires witness freshness check so stale SLEIGH/revision does not pass",
        ],
    },
}


def run(*command: str | Path) -> str:
    return subprocess.run(
        command,
        cwd=REPO,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        timeout=90,
    ).stdout


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def case_input(case: dict) -> tuple[str, int, list[int]]:
    function, value, initial = case["function"], case["input"], case["initial_bytes"]
    if (
        function not in FUNCTIONS
        or type(value) is not int
        or not 0 <= value < 2**32
        or not isinstance(initial, list)
        or len(initial) != EXPECTED_BUFFER_LEN
        or any(type(b) is not int or not 0 <= b <= 255 for b in initial)
    ):
        raise ValueError("invalid repository fixture case")
    return function, value, initial


def _canonical_lane(obs: dict) -> tuple[int, list[int], list]:
    return (obs["return_value"], obs["buffer"], obs["outside_writes"])


def validate_observation_lane(
    lane_name: str,
    obs: object,
    *,
    expected_buf_len: int = EXPECTED_BUFFER_LEN,
) -> tuple[str | None, str | None]:
    """Validate one observation lane; never permit missing or errored data to pass."""
    if not isinstance(obs, dict):
        if lane_name == "native":
            return (
                "inconclusive_native_harness_failure",
                "native observation missing or not an object",
            ) if obs is not None else (
                "inconclusive_missing_observation",
                "missing 'native' observation lane",
            )
        return (
            "inconclusive_missing_observation",
            f"missing or malformed '{lane_name}' observation lane",
        )

    if obs.get("error"):
        if lane_name == "native":
            return (
                "inconclusive_native_harness_failure",
                f"native harness error: {obs.get('error')}",
            )
        return (
            "inconclusive_asymmetric_timeout"
            if obs.get("timeout") or obs.get("unsupported")
            else "inconclusive_missing_observation",
            f"{lane_name} lane reported error: {obs.get('error')}",
        )

    if obs.get("timeout") is True or obs.get("unsupported") is True:
        if lane_name == "native":
            return (
                "inconclusive_native_harness_failure",
                "native harness timed out or reported unsupported",
            )
        return (
            "inconclusive_asymmetric_timeout",
            f"{lane_name} lane reported timeout/unsupported",
        )

    if obs.get("truncated") is True:
        return (
            "inconclusive_truncated_trace",
            f"{lane_name} observation explicitly marked truncated",
        )

    for key in ("return_value", "buffer", "outside_writes"):
        if key not in obs:
            return (
                "inconclusive_missing_observation",
                f"{lane_name} observation missing required key '{key}'",
            )

    if type(obs["return_value"]) is not int:
        return (
            "inconclusive_missing_observation",
            f"{lane_name} observation has non-integer return_value",
        )
    buf = obs["buffer"]
    if not isinstance(buf, list) or any(type(b) is not int or not 0 <= b <= 255 for b in buf):
        return (
            "inconclusive_missing_observation",
            f"{lane_name} observation has invalid buffer contents",
        )
    if len(buf) != expected_buf_len:
        return (
            "inconclusive_truncated_trace",
            f"{lane_name} buffer length {len(buf)} != expected {expected_buf_len}",
        )
    if not isinstance(obs["outside_writes"], list):
        return (
            "inconclusive_missing_observation",
            f"{lane_name} observation has non-list outside_writes",
        )
    return None, None


def classify_observation(
    result: object,
    *,
    require_python: bool = False,
    expected_buf_len: int = EXPECTED_BUFFER_LEN,
) -> dict:
    """Classify one multi-lane replay observation without masking shared or missing faults."""
    base = {
        "proved": False,
        "evidence_scope": "sampled_concrete",
    }
    if not isinstance(result, dict):
        return {
            **base,
            "status": "inconclusive",
            "classification": "inconclusive_missing_observation",
            "reason": "observation payload is not a JSON object",
        }

    # Check native first so a failing native harness is never masked even when
    # internal Fission lanes (`emulator` and `hir`) happen to agree.
    required_lanes = ["native", "emulator", "hir"]
    if require_python or "python" in result:
        required_lanes.append("python")

    for lane in required_lanes:
        if lane not in result:
            return {
                **base,
                "status": "inconclusive",
                "classification": "inconclusive_missing_observation",
                "reason": f"missing required observation lane '{lane}'",
            }
        err_class, err_reason = validate_observation_lane(
            lane, result[lane], expected_buf_len=expected_buf_len
        )
        if err_class is not None:
            return {
                **base,
                "status": "inconclusive",
                "classification": err_class,
                "reason": err_reason,
            }

    native_val = _canonical_lane(result["native"])
    emu_val = _canonical_lane(result["emulator"])
    hir_val = _canonical_lane(result["hir"])
    py_ok = "python" not in result or _canonical_lane(result["python"]) == native_val

    emu_ok = emu_val == native_val
    hir_ok = hir_val == native_val

    if emu_ok and hir_ok and py_ok:
        return {
            **base,
            "status": "agreed",
            "classification": "sampled_agreement",
            "reason": "native, emulator, HIR, and exported Python (where present) agree on sampled inputs",
        }
    if not emu_ok and not hir_ok and emu_val == hir_val:
        return {
            **base,
            "status": "diverged",
            "classification": "diverged_shared_fission_fault",
            "reason": "emulator and HIR agree with each other but diverge from native ground truth",
        }
    if emu_ok and not hir_ok:
        return {
            **base,
            "status": "diverged",
            "classification": "diverged_hir_only",
            "reason": "emulator matches native ground truth while HIR diverges",
        }
    if hir_ok and not emu_ok:
        return {
            **base,
            "status": "diverged",
            "classification": "diverged_emulator_only",
            "reason": "HIR matches native ground truth while emulator diverges",
        }
    if emu_ok and hir_ok and not py_ok:
        return {
            **base,
            "status": "diverged",
            "classification": "diverged_python_export",
            "reason": "exported Python diverges from agreeing native/emulator/HIR lanes",
        }
    return {
        **base,
        "status": "diverged",
        "classification": "diverged_all_lanes",
        "reason": "multiple observation lanes disagree with native ground truth",
    }


def validate_witness_freshness(witness: object, current_identity: dict) -> tuple[bool, list[str]]:
    """Reject stale, malformed, or cross-revision replay witnesses explicitly."""
    reasons: list[str] = []
    if not isinstance(witness, dict):
        return False, ["witness is not a JSON object"]
    if witness.get("version") != 1:
        reasons.append(f"unsupported witness version: {witness.get('version')!r}")

    checked_fields = (
        "fixture_sha256",
        "source_sha256",
        "sleigh_manifest_sha256",
        "abi",
        "compiler_flags",
        "fission_revision",
    )
    for field in checked_fields:
        if field not in witness:
            reasons.append(f"witness missing identity field '{field}'")
        elif witness[field] != current_identity.get(field):
            reasons.append(
                f"stale witness '{field}': witness={witness[field]!r} current={current_identity.get(field)!r}"
            )
    if "observations" in witness:
        verdict = classify_observation(witness["observations"])
        if verdict["status"] == "inconclusive":
            reasons.append(f"witness observations invalid: {verdict['classification']}")
    return len(reasons) == 0, reasons


def observe(case: dict, runner: Path, fixture: Path, native: list[str], ported) -> dict:
    function, value, initial = case_input(case)
    args = [function, str(value), *(str(b) for b in initial)]
    try:
        result = json.loads(run(runner, "observe", fixture, *args))
    except (subprocess.SubprocessError, json.JSONDecodeError) as exc:
        return {
            "emulator": {"error": f"runner failure: {exc}"},
            "hir": {"error": f"runner failure: {exc}"},
            "native": {"error": "not evaluated due to runner failure"},
        }

    try:
        native_return, native_buffer = json.loads(run(*native, *args))
        result["native"] = {
            "return_value": native_return,
            "buffer": native_buffer,
            "outside_writes": [],
        }
    except (subprocess.SubprocessError, json.JSONDecodeError, ValueError) as exc:
        result["native"] = {"error": f"native harness failure: {exc}"}

    if function == "mix_word":
        result["python"] = {
            "return_value": ported(value) & 0xFFFFFFFF,
            "buffer": list(initial),
            "outside_writes": [],
        }
    return result


def agrees(result: dict) -> bool:
    return classify_observation(result)["status"] == "agreed"


def save_witness(path: Path, case: dict, identity: dict, result: dict) -> None:
    with path.open("x") as stream:
        json.dump({"version": 1, **identity, **case, "observations": result}, stream, indent=2)
        stream.write("\n")


def run_calibration_matrix() -> dict:
    """Exercise fault-injection and classification controls offline with denominators."""
    baseline = {
        "emulator": {"return_value": 42, "buffer": [19, 83, 227], "outside_writes": []},
        "hir": {"return_value": 42, "buffer": [19, 83, 227], "outside_writes": []},
        "native": {"return_value": 42, "buffer": [19, 83, 227], "outside_writes": []},
        "python": {"return_value": 42, "buffer": [19, 83, 227], "outside_writes": []},
    }
    base_identity = {
        "fixture_sha256": "a" * 64,
        "source_sha256": "b" * 64,
        "sleigh_manifest_sha256": "c" * 64,
        "abi": "x86_64-linux-sysv",
        "compiler_flags": list(FLAGS),
        "fission_revision": "deadbeef" * 5,
    }
    base_witness = {
        "version": 1,
        **base_identity,
        "function": "mix_word",
        "input": 1,
        "initial_bytes": list(INITIAL),
        "observations": copy.deepcopy(baseline),
    }

    cases_out = []

    def check_obs(name: str, payload: object, exp_status: str, exp_class: str) -> None:
        verdict = classify_observation(payload)
        matched = (
            verdict["status"] == exp_status
            and verdict["classification"] == exp_class
            and verdict["proved"] is False
        )
        false_pass = exp_status != "agreed" and agrees(payload if isinstance(payload, dict) else {})
        cases_out.append(
            {
                "name": name,
                "expected_status": exp_status,
                "expected_classification": exp_class,
                "actual_status": verdict["status"],
                "actual_classification": verdict["classification"],
                "matched": matched,
                "false_pass": false_pass,
            }
        )

    # 1. Positive control: sampled agreement (never proved=True).
    check_obs("positive_control_agreement", baseline, "agreed", "sampled_agreement")

    # 2. Shared wrong lift/eval: emulator and HIR agree with wrong 99 while native is 42.
    shared_wrong = copy.deepcopy(baseline)
    shared_wrong["emulator"]["return_value"] = 99
    shared_wrong["hir"]["return_value"] = 99
    check_obs(
        "shared_wrong_lift_emulator_and_hir_agree",
        shared_wrong,
        "diverged",
        "diverged_shared_fission_fault",
    )

    # 3. HIR-only return-value corruption.
    hir_val = copy.deepcopy(baseline)
    hir_val["hir"]["return_value"] = 43
    check_obs("hir_only_value_corruption", hir_val, "diverged", "diverged_hir_only")

    # 4. HIR-only buffer store corruption.
    hir_store = copy.deepcopy(baseline)
    hir_store["hir"]["buffer"][1] = 0
    check_obs("hir_only_store_buffer_corruption", hir_store, "diverged", "diverged_hir_only")

    # 5. HIR-only out-of-bounds store corruption.
    hir_oob = copy.deepcopy(baseline)
    hir_oob["hir"]["outside_writes"] = [["0xdeadbeef", 1]]
    check_obs("hir_only_outside_write_corruption", hir_oob, "diverged", "diverged_hir_only")

    # 6. Exported Python corruption.
    py_bad = copy.deepcopy(baseline)
    py_bad["python"]["return_value"] = 7
    check_obs("python_export_corruption", py_bad, "diverged", "diverged_python_export")

    # 7. Missing HIR lane.
    no_hir = copy.deepcopy(baseline)
    del no_hir["hir"]
    check_obs(
        "missing_hir_observation", no_hir, "inconclusive", "inconclusive_missing_observation"
    )

    # 8. Missing native lane (must NEVER pass even if emulator == hir).
    no_native = copy.deepcopy(baseline)
    del no_native["native"]
    check_obs(
        "missing_native_observation", no_native, "inconclusive", "inconclusive_missing_observation"
    )

    # 9. Truncated trace buffer.
    trunc_buf = copy.deepcopy(baseline)
    trunc_buf["hir"]["buffer"] = [19, 83]
    check_obs("truncated_buffer_trace", trunc_buf, "inconclusive", "inconclusive_truncated_trace")

    # 10. Asymmetric timeout on HIR while emulator & native match.
    asym_timeout = copy.deepcopy(baseline)
    asym_timeout["hir"] = {"timeout": True, "error": "fuel exhausted"}
    check_obs(
        "asymmetric_hir_timeout", asym_timeout, "inconclusive", "inconclusive_asymmetric_timeout"
    )

    # 11. Native harness failure masked by agreeing emulator == hir.
    native_fail = copy.deepcopy(baseline)
    native_fail["native"] = {"error": "docker runtime failed"}
    check_obs(
        "native_harness_failure_masked_by_shared_fission",
        native_fail,
        "inconclusive",
        "inconclusive_native_harness_failure",
    )

    # 12 & 13. Stale witnesses (SLEIGH manifest mismatch, git revision mismatch).
    for name, field, bad_val in (
        ("stale_witness_sleigh_manifest_mismatch", "sleigh_manifest_sha256", "f" * 64),
        ("stale_witness_revision_mismatch", "fission_revision", "0" * 40),
    ):
        stale = copy.deepcopy(base_witness)
        stale[field] = bad_val
        fresh, reasons = validate_witness_freshness(stale, base_identity)
        actual_class = "inconclusive_stale_witness" if not fresh else "sampled_agreement"
        actual_status = "inconclusive" if not fresh else "agreed"
        cases_out.append(
            {
                "name": name,
                "expected_status": "inconclusive",
                "expected_classification": "inconclusive_stale_witness",
                "actual_status": actual_status,
                "actual_classification": actual_class,
                "matched": (not fresh) and bool(reasons),
                "false_pass": fresh,
                "reasons": reasons,
            }
        )

    total = len(cases_out)
    matched = sum(1 for c in cases_out if c["matched"])
    false_passes = sum(1 for c in cases_out if c["false_pass"])
    false_negatives = sum(
        1 for c in cases_out if not c["matched"] and c["expected_status"] != "agreed"
    )
    return {
        "schema_version": 1,
        "denominators": {
            "total_cases": total,
            "matched_expected": matched,
            "false_passes": false_passes,
            "false_negatives": false_negatives,
        },
        "oracle_dependency_matrix": ORACLE_DEPENDENCY_MATRIX,
        "cases": cases_out,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, help="built concrete_replay example")
    parser.add_argument(
        "--calibrate",
        action="store_true",
        help="run the offline fault-injection & oracle dependency calibration matrix",
    )
    parser.add_argument(
        "--cc", default="cc", help="compiler command, e.g. 'zig cc -target x86_64-linux-musl'"
    )
    parser.add_argument(
        "--linux-container",
        help="run the fixed native fixture in this linux/amd64 Docker image",
    )
    parser.add_argument("--witness", type=Path, default=Path("replay-witness.json"))
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--replay", type=Path)
    mode.add_argument("--function", choices=FUNCTIONS)
    parser.add_argument("--python-out", type=Path)
    args = parser.parse_args()

    if args.calibrate:
        report = run_calibration_matrix()
        print(json.dumps(report, indent=2, sort_keys=True))
        d = report["denominators"]
        return 0 if d["matched_expected"] == d["total_cases"] and d["false_passes"] == 0 else 1

    if not args.runner:
        parser.error("--runner is required unless --calibrate is passed")
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
        if case is not None:
            fresh, reasons = validate_witness_freshness(case, identity)
            if not fresh:
                parser.error("; ".join(reasons))
        native = [str(fixture)]
        if args.linux_container:
            native = [
                "docker",
                "run",
                "--rm",
                "--platform",
                "linux/amd64",
                "--network",
                "none",
                "--volume",
                f"{fixture}:/fixture:ro",
                "--entrypoint",
                "/fixture",
                args.linux_container,
            ]

        source = run(args.runner.resolve(), "python", fixture, "mix_word")
        module = work / "ported.py"
        module.write_text(source)
        ported = runpy.run_path(str(module))["ported"]
        if args.python_out:
            with args.python_out.open("x") as output:
                output.write(source)

        cases = (
            [case]
            if case is not None
            else [
                {"function": function, "input": value, "initial_bytes": INITIAL}
                for function in functions
                for value in INPUTS
            ]
        )
        for current in cases:
            result = observe(current, args.runner.resolve(), fixture, native, ported)
            verdict = classify_observation(result, require_python=(current["function"] == "mix_word"))
            if verdict["status"] != "agreed":
                if case is None:
                    save_witness(args.witness, current, identity, result)
                    print(f"Wrote {args.witness}")
                print(json.dumps({"verdict": verdict, "observations": result}))
                return 1
        print(f"Agreement on {len(cases)} cases (native, emulator, printed HIR; Python for mix_word)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
