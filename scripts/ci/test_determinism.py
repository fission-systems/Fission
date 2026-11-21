import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

import decompiler_smoke
import determinism_check


def output(codes):
    return json.dumps([{"address": address, "code": code} for address, code in codes.items()])


class DeterminismTests(unittest.TestCase):
    def test_smoke_accepts_identical_runs(self):
        with patch.object(decompiler_smoke, "run", return_value=output({"0x10": "a", "0x20": "b"})):
            with patch("sys.stdout"):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_smoke_rejects_a_function_that_changes_between_runs(self):
        runs = iter([output({"0x10": "a", "0x20": "b"}), output({"0x10": "a", "0x20": "b"}),
                     output({"0x10": "a", "0x20": "B"})])
        with patch.object(decompiler_smoke, "run", side_effect=lambda *a, **k: next(runs)):
            with self.assertRaisesRegex(ValueError, "0x20"):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_smoke_rejects_a_function_that_disappears(self):
        runs = iter([output({"0x10": "a", "0x20": "b"}), output({"0x10": "a"}), output({"0x10": "a", "0x20": "b"})])
        with patch.object(decompiler_smoke, "run", side_effect=lambda *a, **k: next(runs)):
            with self.assertRaises(ValueError):
                decompiler_smoke.check_determinism(Path("cli"), Path("fixture"))

    def test_nightly_reports_every_differing_address(self):
        runs = [{"0x1": "x", "0x2": "y"}, {"0x1": "x", "0x2": "z"}, {"0x1": "x", "0x3": "w"}]
        self.assertEqual(determinism_check.differing(runs), ["0x2", "0x3"])

    def test_sample_is_fixed_and_bounded(self):
        entries = [{"sha256": f"{n:02x}", "functions": ["f"] * (n + 1), "binary_path": str(n)} for n in range(40)]
        chosen = determinism_check.sample(list(reversed(entries)))
        self.assertEqual(chosen, determinism_check.sample(entries))
        before_last = sum(len(e["functions"]) for e in chosen[:-1])
        self.assertLess(before_last, determinism_check.FUNCTION_BUDGET)


if __name__ == "__main__":
    unittest.main()
