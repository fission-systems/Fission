import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

import toolchain_canary as canary

ICE_LOG = """   Compiling fission-pcode v0.3.0
error: the compiler unexpectedly panicked. this is a bug.

note: we would appreciate a bug report: https://github.com/rust-lang/rust/issues/new
error: could not compile `fission-pcode` (lib)
"""


def run(*argv):
    out = io.StringIO()
    with redirect_stdout(out):
        code = canary.main(list(argv))
    return code, out.getvalue()


class IceTests(unittest.TestCase):
    def test_finds_the_panic_line(self):
        self.assertEqual(
            canary.find_ice(ICE_LOG)[0], "error: the compiler unexpectedly panicked. this is a bug."
        )

    def test_ordinary_type_error_is_not_an_ice(self):
        log = "error[E0308]: mismatched types\nerror: could not compile `x` (lib) due to 1 previous error\n"
        self.assertEqual(canary.find_ice(log), [])

    def test_llvm_and_signal_crashes_count(self):
        self.assertTrue(canary.find_ice("LLVM ERROR: out of memory"))
        self.assertTrue(
            canary.find_ice("error: could not compile `x` (lib) (signal: 11, SIGSEGV: invalid memory reference)")
        )

    def test_command_exit_codes(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad, good = Path(tmp, "build.log"), Path(tmp, "test.log")
            bad.write_text(ICE_LOG)
            good.write_text("Finished dev\n")
            code, text = run("ice", str(good), str(bad), str(Path(tmp, "missing.log")))
            self.assertEqual(code, 1)
            self.assertIn("build.log", text)
            self.assertNotIn("test.log", text)
            self.assertEqual(run("ice", str(good))[0], 0)


class ClippyTests(unittest.TestCase):
    def test_counts_diagnostics_not_summary_lines(self):
        log = (
            "warning: unused variable: `x`\nwarning[clippy::foo]: bar\n"
            "warning: `fission-core` (lib) generated 2 warnings\n"
            "error: could not compile `fission-core` (lib) due to 1 previous error\n"
            "error[E0432]: unresolved import\n"
        )
        self.assertEqual(canary.count_diagnostics(log), {"warning": 2, "error": 1})


class HashTests(unittest.TestCase):
    def test_unstable_functions_are_dropped(self):
        stable, unstable = canary.stable_hashes(
            {"0x1": "a", "0x2": "b"}, {"0x1": "a", "0x2": "c", "0x3": "d"}
        )
        self.assertEqual(stable, {"0x1": "a"})
        self.assertEqual(unstable, ["0x2", "0x3"])

    def test_identical_sides_pass_and_a_changed_function_fails(self):
        side = {"f.elf": {"0x10": "aa", "0x20": "bb"}}
        self.assertEqual(canary.diff_hashes(side, json.loads(json.dumps(side))), [])
        changed = {"f.elf": {"0x10": "aa", "0x20": "zz"}}
        self.assertEqual(canary.diff_hashes(side, changed), ["f.elf 0x20: output text differs"])

    def test_missing_function_or_fixture_is_a_difference(self):
        base = {"f.elf": {"0x10": "aa"}, "g.elf": {"0x1": "x"}}
        head = {"f.elf": {}}
        self.assertEqual(
            canary.diff_hashes(base, head),
            ["f.elf 0x10: present on only one side", "g.elf: decompiled on only one side"],
        )

    def test_compare_command_reports_differing_functions(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp, "a.json"), Path(tmp, "b.json")
            a.write_text(json.dumps({"f.elf": {"0x10": "aa"}}))
            b.write_text(json.dumps({"f.elf": {"0x10": "bb"}}))
            code, text = run("compare", str(a), str(b))
            self.assertEqual(code, 1)
            self.assertIn("f.elf 0x10", text)
            self.assertEqual(run("compare", str(a), str(a))[0], 0)


if __name__ == "__main__":
    unittest.main()
