import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import freshbench_corpus
import freshbench_sweep


def truth(build, project):
    return {"path": f"truth/{build}/{project}/targets.json", "kind": "ground_truth", "project": project}


def report(rows=300, scored=241, types=89, failed=0):
    return {"overall": {"rows": rows, "perfect": 66, "scored": scored, "types_perfect": types, "distance": 6723.0},
            "timing": {"failed_calls": failed, "decompile_cpu_s": 11.0}}


class FreshbenchSweepTests(unittest.TestCase):
    def test_builds_are_every_build_with_targets(self):
        entries = [truth("gcc-O2", "a"), truth("gcc-O2", "b"), truth("i686-gcc-O0", "a"),
                   {"path": "stripped/armv7-gcc-O0/a/x", "kind": "stripped", "project": "a"}]
        self.assertEqual(freshbench_sweep.builds(entries), ["gcc-O2", "i686-gcc-O0"])

    def test_a_shard_fetches_only_the_units_its_filter_can_select(self):
        entries = [truth("gcc-O2", "a"), truth("gcc-O2", "b"), truth("gcc-O0", "a")]
        self.assertEqual(freshbench_sweep.units_for(entries, "gcc-O2", None), {("gcc-O2", "a"), ("gcc-O2", "b")})
        self.assertEqual(freshbench_sweep.units_for(entries, "gcc-O2", "gcc-O2/a"), {("gcc-O2", "a")})
        self.assertEqual(freshbench_sweep.units_for(entries, "gcc-O2", "gcc-O2/a/libx.so"), {("gcc-O2", "a")})
        # A bare binary-name filter could be in any unit of the build.
        self.assertEqual(freshbench_sweep.units_for(entries, "gcc-O2", "libx"), {("gcc-O2", "a"), ("gcc-O2", "b")})

    def test_smoke_requires_every_stage(self):
        self.assertEqual(freshbench_sweep.smoke(report()), [])
        self.assertIn("no row scored for GED", freshbench_sweep.smoke(report(scored=0)))
        self.assertIn("no row got a perfect type score", freshbench_sweep.smoke(report(types=0)))
        self.assertIn("2 failed tool calls", freshbench_sweep.smoke(report(failed=2)))
        self.assertIn("no rows", freshbench_sweep.smoke(report(rows=0)))

    def test_the_table_totals_every_shard(self):
        text = freshbench_sweep.table({"gcc-O2": report(), "gcc-O0": report(rows=100, scored=50)})
        self.assertIn("| **all** | 400 | 132 | 13446 | 291 |", text)


    def test_the_report_blocks_on_a_missing_shard_or_a_row_that_lost_its_output(self):
        def diff(lost):
            return {"changed_outputs": lost, "perfect_gained": 0, "perfect_lost": 0,
                    "distance_delta": 0.0, "lost_output": lost}

        def run(builds_present, lost):
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                for name in builds_present:
                    (tmp / "in" / name).mkdir(parents=True)
                    (tmp / "in" / name / "meta.json").write_text("{}")
                    (tmp / "base" / name).mkdir(parents=True)
                    (tmp / "base" / name / "meta.json").write_text("{}")

                def fake(freshbench, *args):
                    return diff(lost) if args[1] == "diff" else report()

                args = SimpleNamespace(data=tmp, freshbench=Path("fb"), input=tmp / "in",
                                       baseline=tmp / "base", out=tmp / "out")
                with patch.object(freshbench_corpus, "manifest", return_value=[truth("gcc-O2", "a"), truth("gcc-O0", "a")]), \
                        patch.object(freshbench_sweep, "run_json", fake), patch("sys.stdout"):
                    return freshbench_sweep.report(args)

        self.assertEqual(run(["gcc-O2", "gcc-O0"], lost=0), 0)
        self.assertEqual(run(["gcc-O2"], lost=0), 1, "missing shard")
        self.assertEqual(run(["gcc-O2", "gcc-O0"], lost=3), 1, "rows lost their output")


if __name__ == "__main__":
    unittest.main()
