import argparse
import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "quality"))
import decbench_full_sweep as sweep


class SweepTests(unittest.TestCase):
    def test_partition_covers_every_binary_once(self):
        entries = [{"binary_path": str(n), "functions": list(range(n + 1))} for n in range(7)]
        shards = sweep.partition(entries, 3)
        self.assertEqual(
            {entry["binary_path"] for group in shards for entry in group},
            {entry["binary_path"] for entry in entries},
        )
        self.assertEqual(sum(map(len, shards)), len(entries))
        self.assertEqual(shards, sweep.partition(list(reversed(entries)), 3))

    def test_only_names_one_exact_binary_in_any_shard(self):
        entries = [{"binary_path": f"binaries/O0/p/{n}", "functions": list(range(n + 1))} for n in range(7)]
        chosen = sweep.select(entries, 3, 0, "binaries/O0/p/5")
        self.assertEqual([entry["binary_path"] for entry in chosen], ["binaries/O0/p/5"])
        # A prefix is not a name: it selects nothing, not every binary under it.
        with self.assertRaisesRegex(ValueError, "empty"):
            sweep.select(entries, 3, 0, "binaries/O0/p/")

    def test_smoke_requires_every_stage_to_produce_data(self):
        with tempfile.TemporaryDirectory() as temp:
            rows = Path(temp) / "rows.jsonl"
            scored = {"status": "ok", "ged": 3, "type_match": 0.5}
            rows.write_text(json.dumps(scored) + "\n")
            with contextlib.redirect_stdout(io.StringIO()):
                sweep.smoke_check(rows, 0)
            for missing in ("ged", "type_match"):
                partial = {key: value for key, value in scored.items() if key != missing}
                rows.write_text(json.dumps(partial) + "\n")
                with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                    sweep.smoke_check(rows, 0)
            rows.write_text(json.dumps(scored) + "\n")
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                sweep.smoke_check(rows, 1)

    def test_full_report_rejects_missing_rows_and_detects_regressions(self):
        entries = [{"binary_path": "binary", "functions": ["one", "two"]}]
        meta = {"sha": "test", "cli_sha256": "test", "shard": 0, "crashes": 0, "provenance": {}}
        rows = [
            {"binary": "binary", "function": name, "status": "ok", "ged": 0, "type_match": 1}
            for name in entries[0]["functions"]
        ]
        with tempfile.TemporaryDirectory() as temp, patch.object(sweep, "manifest", return_value=entries):
            root = Path(temp)
            incoming = root / "incoming"
            shard = incoming / "shard-0"
            shard.mkdir(parents=True)
            (shard / "meta.json").write_text(json.dumps(meta))
            baseline = root / "baseline"
            args = argparse.Namespace(input=incoming, data=root, out=baseline, baseline=None, shards=1)
            shard_rows = shard / "rows.jsonl"
            shard_rows.write_text(json.dumps(rows[0]) + "\n")
            with self.assertRaisesRegex(ValueError, "row identities"):
                sweep.report(args)
            shard_rows.write_text("".join(json.dumps(row) + "\n" for row in rows))
            with contextlib.redirect_stdout(io.StringIO()):
                sweep.report(args)
            args.baseline, args.out = baseline, root / "candidate"
            rows[1] = {"binary": "binary", "function": "two", "status": "no_output"}
            shard_rows.write_text("".join(json.dumps(row) + "\n" for row in rows))
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
                sweep.report(args)
            report = json.loads((args.out / "summary.json").read_text())
            self.assertEqual(report["regressions"], 1)
            self.assertEqual(report["candidate"]["ged_total_distance"], 0)
            self.assertTrue(report["compared"])


if __name__ == "__main__":
    unittest.main()
