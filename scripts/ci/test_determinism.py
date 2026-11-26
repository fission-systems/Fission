import json
import unittest
from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import patch

import decompiler_smoke
import determinism_check


def output(codes, workers):
    return json.dumps({
        "_meta": {"worker_count": workers},
        "functions": [{"address": address, "code": code} for address, code in codes.items()],
    })


class DeterminismTests(unittest.TestCase):
    def test_smoke_accepts_identical_runs(self):
        codes = {"0x10": "a", "0x20": "b"}
        with patch.object(decompiler_smoke, "run", side_effect=[output(codes, 1), output(codes, 2), output(codes, 2)]):
            with patch("sys.stdout"):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_smoke_rejects_a_function_that_changes_between_runs(self):
        runs = [output({"0x10": "a", "0x20": "b"}, 1), output({"0x10": "a", "0x20": "b"}, 2),
                output({"0x10": "a", "0x20": "B"}, 2)]
        with patch.object(decompiler_smoke, "run", side_effect=runs):
            with self.assertRaisesRegex(ValueError, "0x20"):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_smoke_rejects_a_function_that_disappears(self):
        runs = [output({"0x10": "a", "0x20": "b"}, 1), output({"0x10": "a"}, 1),
                output({"0x10": "a", "0x20": "b"}, 2)]
        with patch.object(decompiler_smoke, "run", side_effect=runs):
            with self.assertRaises(ValueError):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_decompile_checks_the_worker_count_reported_by_the_cli(self):
        codes = {str(address): "code" for address in range(4)}
        result = CompletedProcess([], 0, stdout=output(codes, 1), stderr="")
        with patch.object(decompiler_smoke.subprocess, "run", return_value=result) as run:
            with self.assertRaisesRegex(ValueError, "reported 1 workers, expected 4"):
                decompiler_smoke.decompile_codes(Path("cli"), Path("fixture"), 4)
            self.assertIn("--benchmark", run.call_args.args[0])
            self.assertEqual(run.call_args.kwargs["env"]["FISSION_RUST_DECOMP_WORKERS"], "4")

    def test_nightly_reports_every_differing_address(self):
        runs = [{"0x1": "x", "0x2": "y"}, {"0x1": "x", "0x2": "z"}, {"0x1": "x", "0x3": "w"}]
        self.assertEqual(determinism_check.differing(runs), ["0x2", "0x3"])

    def test_sample_order_is_fixed_and_takes_only_stripped_binaries(self):
        entries = [{"sha256": f"{n:02x}", "kind": kind, "path": f"{kind}/{n}"}
                   for n in range(10) for kind in ("stripped", "binary")]
        chosen = determinism_check.ordered(list(reversed(entries)))
        self.assertEqual(chosen, determinism_check.ordered(entries))
        self.assertEqual({e["kind"] for e in chosen}, {"stripped"})
        self.assertEqual([e["sha256"] for e in chosen], sorted(e["sha256"] for e in chosen))

if __name__ == "__main__":
    unittest.main()
