"""Positive controls for scripts/check/owner_boundaries.sh.

The scan once passed vacuously: a missing directory or a missing `rg` read as
"no match". These tests run the real script against a synthetic tree, with
`rg` and with only `grep` on PATH, and require a planted violation to fail and
a missing directory to error.
"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/check/owner_boundaries.sh"
DIRS = [
    "crates/fission-pcode/src/render",
    "crates/fission-pcode/src/midend/builder",
    "crates/fission-pcode/src/midend/structuring",
    "crates/fission-midend-normalize/src",
    "crates/fission-midend-structuring/src",
]


class OwnerBoundaryScan(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        (self.tmp / "scripts/check").mkdir(parents=True)
        shutil.copy(SCRIPT, self.tmp / "scripts/check/owner_boundaries.sh")
        for d in DIRS:
            (self.tmp / d).mkdir(parents=True)
            (self.tmp / d / "lib.rs").write_text("pub fn clean() {}\n")

    def tearDown(self):
        shutil.rmtree(self.tmp)

    def run_scan(self, tools):
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir(exist_ok=True)
        for tool in tools:
            found = shutil.which(tool)
            if found is None:
                self.skipTest(f"{tool} not installed")
            link = bin_dir / tool
            if not link.exists():
                link.symlink_to(found)
        bash = shutil.which("bash")
        return subprocess.run(
            [bash, str(self.tmp / "scripts/check/owner_boundaries.sh")],
            env={"PATH": str(bin_dir)},
            capture_output=True,
            text=True,
        )

    def each_searcher(self):
        # grep only, then rg (rg wins when present).
        yield ["dirname", "grep"]
        if shutil.which("rg"):
            yield ["dirname", "grep", "rg"]

    def test_clean_tree_passes(self):
        for tools in self.each_searcher():
            with self.subTest(tools=tools):
                r = self.run_scan(tools)
                self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_planted_violations_fail(self):
        cases = [
            ("crates/fission-pcode/src/render", "use fission_midend_normalize::x;\n"),
            ("crates/fission-pcode/src/render", "use crate::midend::structuring::y;\n"),
            ("crates/fission-midend-normalize/src", "fn f() { apply_hir_presentation(); }\n"),
            ("crates/fission-midend-structuring/src", "fn f() { apply_hir_presentation(); }\n"),
            ("crates/fission-pcode/src/midend/builder", "fn f() { apply_hir_presentation(); }\n"),
        ]
        for tools in self.each_searcher():
            for d, src in cases:
                with self.subTest(tools=tools, dir=d):
                    bad = self.tmp / d / "bad.rs"
                    bad.write_text(src)
                    try:
                        r = self.run_scan(tools)
                    finally:
                        bad.unlink()
                    self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
                    self.assertIn("BOUNDARY FAIL", r.stdout)

    def test_missing_directory_is_an_error_not_ok(self):
        shutil.rmtree(self.tmp / "crates/fission-midend-normalize/src")
        for tools in self.each_searcher():
            with self.subTest(tools=tools):
                r = self.run_scan(tools)
                self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
                self.assertIn("does not exist", r.stderr)

    def test_no_search_tool_is_an_error_not_ok(self):
        # With neither rg nor grep the old script printed OK for every rule.
        r = self.run_scan(["dirname"])
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertNotIn("Owner boundary scan passed", r.stdout)


if __name__ == "__main__":
    unittest.main()
