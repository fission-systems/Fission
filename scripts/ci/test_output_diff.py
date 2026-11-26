import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import freshbench_corpus
import output_diff


def truth(build, project, sha, functions):
    return {"path": f"truth/{build}/{project}/targets.json", "kind": "ground_truth",
            "project": project, "sha256": sha, "size": 0, "functions": functions}


class OutputDiffTests(unittest.TestCase):
    def test_units_follow_the_targets_hash_order_until_the_budget(self):
        entries = [
            truth("gcc-O2", "a", "30", 5),
            truth("gcc-O0", "b", "10", 4),
            truth("i686-gcc-O0", "c", "20", 4),
            {"path": "stripped/gcc-O0/b/x", "kind": "stripped", "project": "b", "sha256": "00", "size": 1},
        ]

        def fake_fetch(entry, root):
            path = Path(root) / entry["path"].replace("/", "_")
            path.write_text(json.dumps({"binaries": {"x": {"functions": [{}] * entry["functions"]}}}))
            return path

        with tempfile.TemporaryDirectory() as root, patch.object(freshbench_corpus, "fetch", fake_fetch):
            # 4 + 4 reaches the budget of 8: the unit after it is not taken.
            self.assertEqual(output_diff.units(entries, Path(root), 8), {("gcc-O0", "b"), ("i686-gcc-O0", "c")})
            self.assertEqual(output_diff.units(list(reversed(entries)), Path(root), 8),
                             {("gcc-O0", "b"), ("i686-gcc-O0", "c")})


if __name__ == "__main__":
    unittest.main()
