import unittest
from unittest.mock import patch

import speed_compare


class SpeedCompareTests(unittest.TestCase):
    def test_flags_a_total_increase_at_the_threshold(self):
        rows = [{"binary": "a", "functions": 1, "base": 100, "head": 103},
                {"binary": "b", "functions": 1, "base": 100, "head": 100}]
        report = speed_compare.compare(rows, threshold=0.01)
        self.assertTrue(report["regressed"])
        self.assertEqual(report["slower_binaries"], ["a"])

    def test_noise_below_the_threshold_is_not_a_regression(self):
        rows = [{"binary": "a", "functions": 1, "base": 1000, "head": 1012}]
        self.assertFalse(speed_compare.compare(rows, threshold=0.03)["regressed"])

    def test_a_speedup_is_not_a_regression(self):
        rows = [{"binary": "a", "functions": 1, "base": 1000, "head": 600}]
        report = speed_compare.compare(rows)
        self.assertFalse(report["regressed"])
        self.assertLess(report["change"], 0)

    def test_cost_discards_the_warm_up_and_keeps_the_minimum(self):
        # The first value is the warm-up. It is the smallest here so that
        # keeping it would change the answer.
        counts = iter([90, 104, 101, 103])
        with patch.object(speed_compare, "instructions", side_effect=lambda *a: next(counts)):
            self.assertEqual(speed_compare.cost("cli", "bin", "res", runs=3), 101)


if __name__ == "__main__":
    unittest.main()
