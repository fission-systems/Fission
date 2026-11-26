import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import parallel_bisect as bisect_ci


def out(index, with_output=100, failed=0):
    return {"index": index, "commit": f"{index:040x}", "rows": 100, "rows_with_output": with_output,
            "failed_calls": failed, "perfect": 10, "distance": 50.0, "outputs_sha256": "x"}


class BisectTests(unittest.TestCase):
    def test_a_short_range_is_measured_whole(self):
        commits = [str(i) for i in range(10)]
        self.assertEqual(bisect_ci.plan(commits, 48), commits)

    def test_a_long_range_keeps_both_ends_and_spaces_the_rest(self):
        commits = [str(i) for i in range(200)]
        picked = bisect_ci.plan(commits, 48)
        self.assertEqual(len(picked), 48)
        self.assertEqual((picked[0], picked[-1]), ("0", "199"))
        self.assertEqual(picked, sorted(picked, key=int))

    def test_the_first_commit_that_loses_output_is_named(self):
        results = [out(0), out(1), out(2, with_output=97), out(3, with_output=97)]
        self.assertEqual(bisect_ci.first_bad(results, "output")["index"], 2)

    def test_a_new_failed_tool_call_is_a_regression(self):
        self.assertEqual(bisect_ci.first_bad([out(0), out(1, failed=1)], "output")["index"], 1)

    def test_nothing_regressed(self):
        self.assertIsNone(bisect_ci.first_bad([out(0), out(1, with_output=101)], "output"))

    def test_speed_compares_with_the_good_cli_on_the_same_runner(self):
        # The second runner counts 2% more for everything: not a regression.
        results = [{"index": 0, "instructions": 1000, "good_instructions": 1000},
                   {"index": 1, "instructions": 1040, "good_instructions": 1020},
                   {"index": 2, "instructions": 1060, "good_instructions": 1020}]
        self.assertEqual(bisect_ci.first_bad(results, "speed", threshold=0.03)["index"], 2)

    def test_speed_uses_the_nightly_threshold(self):
        results = [{"index": 0, "instructions": 1000}, {"index": 1, "instructions": 1020},
                   {"index": 2, "instructions": 1031}]
        self.assertEqual(bisect_ci.first_bad(results, "speed", threshold=0.03)["index"], 2)

    def test_the_verdict_says_when_the_culprit_range_was_sampled(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            for r in (out(0), {**out(5), "index": 5}, {**out(9, with_output=90), "index": 9}):
                (tmp / str(r["index"])).mkdir()
                (tmp / str(r["index"]) / "probe.json").write_text(json.dumps(r))
            args = SimpleNamespace(probe="output", input=tmp, out=tmp / "out")
            with patch("sys.stdout"):
                bisect_ci.verdict(args)
            text = (tmp / "out" / "bisect.md").read_text()
            self.assertIn(f"`{9:040x}`", text)
            self.assertIn("was sampled", text)


if __name__ == "__main__":
    unittest.main()
