#!/usr/bin/env python3
"""Tests for verification oracle calibration matrix and witness freshness (#1254)."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import replay_fixture  # noqa: E402


REPO_ROOT = Path(__file__).resolve().parents[2]
REPLAY_SCRIPT = REPO_ROOT / "scripts" / "test" / "replay_fixture.py"


class ReplayCalibrationTests(unittest.TestCase):
    def test_offline_calibration_matrix_passes_all_controls_without_false_passes(self) -> None:
        report = replay_fixture.run_calibration_matrix()
        d = report["denominators"]
        self.assertGreaterEqual(d["total_cases"], 13)
        self.assertEqual(d["matched_expected"], d["total_cases"], report)
        self.assertEqual(d["false_passes"], 0, report)
        self.assertEqual(d["false_negatives"], 0, report)

        matrix = report["oracle_dependency_matrix"]
        for lane in (
            "concrete_prehir_hir",
            "ground_truth_emulator",
            "symbolic_prehir_hir",
            "native_replay_fixture",
        ):
            self.assertIn(lane, matrix)
            self.assertTrue(matrix[lane]["common_mode_blind_spots"])
        self.assertFalse(matrix["concrete_prehir_hir"]["independent_of_fission_lift"])
        self.assertFalse(matrix["symbolic_prehir_hir"]["independent_of_fission_lift"])
        self.assertTrue(matrix["native_replay_fixture"]["independent_of_fission_lift"])

    def test_calibrate_cli_subcommand_exits_zero_and_emits_valid_json(self) -> None:
        proc = subprocess.run(
            [sys.executable, str(REPLAY_SCRIPT), "--calibrate"],
            cwd=REPO_ROOT,
            text=True,
            capture_output=True,
            check=True,
        )
        payload = json.loads(proc.stdout)
        d = payload["denominators"]
        self.assertEqual(d["matched_expected"], d["total_cases"])
        self.assertEqual(d["false_passes"], 0)

    def test_witness_freshness_rejects_every_stale_or_incomplete_field(self) -> None:
        base_identity = {
            "fixture_sha256": "a" * 64,
            "source_sha256": "b" * 64,
            "sleigh_manifest_sha256": "c" * 64,
            "abi": "x86_64-linux-sysv",
            "compiler_flags": list(replay_fixture.FLAGS),
            "fission_revision": "1234567890" * 4,
        }
        good = {
            "version": 1,
            **base_identity,
            "function": "mix_word",
            "input": 1,
            "initial_bytes": [19, 83, 227],
            "observations": {
                "emulator": {"return_value": 1, "buffer": [19, 83, 227], "outside_writes": []},
                "hir": {"return_value": 1, "buffer": [19, 83, 227], "outside_writes": []},
                "native": {"return_value": 2, "buffer": [19, 83, 227], "outside_writes": []},
            },
        }
        ok, reasons = replay_fixture.validate_witness_freshness(good, base_identity)
        self.assertTrue(ok, reasons)

        for field, bad_val in (
            ("version", 2),
            ("fixture_sha256", "0" * 64),
            ("source_sha256", "0" * 64),
            ("sleigh_manifest_sha256", "0" * 64),
            ("abi", "aarch64-linux"),
            ("compiler_flags", ["-O0"]),
            ("fission_revision", "0" * 40),
        ):
            mutated = copy.deepcopy(good)
            mutated[field] = bad_val
            fresh, mut_reasons = replay_fixture.validate_witness_freshness(mutated, base_identity)
            self.assertFalse(fresh, f"expected {field} mismatch to fail")
            self.assertTrue(mut_reasons)

    def test_observe_native_harness_failure_never_returns_agreement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            runner = Path(tmp) / "runner"
            fixture = Path(tmp) / "fixture"
            case = {"function": "mix_word", "input": 1, "initial_bytes": [19, 83, 227]}

            def fake_run(*cmd: str | Path) -> str:
                if Path(cmd[0]) == runner:
                    return json.dumps(
                        {
                            "emulator": {"return_value": 11, "buffer": [19, 83, 227], "outside_writes": []},
                            "hir": {"return_value": 11, "buffer": [19, 83, 227], "outside_writes": []},
                        }
                    )
                raise subprocess.CalledProcessError(1, list(cmd))

            with mock.patch.object(replay_fixture, "run", side_effect=fake_run):
                obs = replay_fixture.observe(case, runner, fixture, [str(fixture)], lambda x: x)
            verdict = replay_fixture.classify_observation(obs, require_python=True)
            self.assertEqual(verdict["status"], "inconclusive")
            self.assertEqual(verdict["classification"], "inconclusive_native_harness_failure")
            self.assertFalse(replay_fixture.agrees(obs))


if __name__ == "__main__":
    unittest.main()
