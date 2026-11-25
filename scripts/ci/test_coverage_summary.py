#!/usr/bin/env python3
"""Deterministic unit tests for scripts/ci/coverage_summary.py."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import coverage_summary as cs  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


def record(path: str, hit: int, total: int, *, summary: bool = True) -> str:
    """One LCOV record with `total` DA lines of which the first `hit` executed.

    This is the shape `llvm-cov export -format=lcov` writes: one DA per
    instrumented line, then LF/LH equal to the unique DA line counts.
    """
    assert 0 <= hit <= total
    lines = [f"SF:{path}"]
    lines += [f"DA:{n},{1 if n <= hit else 0}" for n in range(1, total + 1)]
    if summary:
        lines += [f"LF:{total}", f"LH:{hit}"]
    lines.append("end_of_record")
    return "\n".join(lines) + "\n"


# Verbatim `llvm-cov export -format=lcov` output (Apple LLVM 17, clang
# -fprofile-instr-generate -fcoverage-mapping), with only the SF paths
# rewritten into the workspace. It carries every non-line directive the
# producer emits (FN, FNDA, FNF, FNH, BRDA, BRF, BRH).
REAL_LLVM_COV_LCOV = """\
SF:/home/runner/work/Fission/Fission/crates/fission-pcode/src/inl.h
FN:1,main.c:g
FN:2,main.c:unused_h
FNDA:1,main.c:g
FNDA:0,main.c:unused_h
FNF:2
FNH:1
DA:1,1
DA:2,0
BRF:0
BRH:0
LF:2
LH:1
end_of_record
SF:/home/runner/work/Fission/Fission/crates/fission-pcode/src/main.c
FN:8,main
FN:3,main.c:f
FNDA:1,main
FNDA:1,main.c:f
FNF:2
FNH:2
DA:3,1
DA:4,1
DA:5,0
DA:6,1
DA:7,1
DA:8,1
DA:9,1
DA:10,1
DA:11,1
BRDA:4,0,0,0
BRDA:4,0,1,1
BRF:2
BRH:1
LF:9
LH:8
end_of_record
"""


class ThresholdConfigTests(unittest.TestCase):
    def test_valid_thresholds(self) -> None:
        self.assertEqual(cs.parse_threshold("0"), 0.0)
        self.assertEqual(cs.parse_threshold("60"), 60.0)
        self.assertEqual(cs.parse_threshold(" 62.5 "), 62.5)
        self.assertEqual(cs.parse_threshold("100"), 100.0)

    def test_invalid_thresholds_rejected(self) -> None:
        for bad in [
            None,
            "",
            "   ",
            "${FISSION_IN_WARN_THRESHOLD_PCT}",
            "abc",
            "-0.1",
            "100.1",
            "nan",
            "inf",
            "-inf",
        ]:
            with self.subTest(bad=bad):
                with self.assertRaises(cs.ConfigError):
                    cs.parse_threshold(bad)


class CratePathMatchingTests(unittest.TestCase):
    def test_exact_crate_matching_distinguishes_prefix_crates(self) -> None:
        self.assertEqual(
            cs.crate_from_source_path("/work/Fission/crates/fission-pcode/src/lib.rs"),
            "fission-pcode",
        )
        self.assertEqual(
            cs.crate_from_source_path("/work/Fission/crates/fission-pcode-ir/src/op.rs"),
            "fission-pcode-ir",
        )
        self.assertEqual(
            cs.crate_from_source_path(r"C:\repo\crates\fission-loader\src\pe.rs"),
            "fission-loader",
        )
        self.assertIsNone(cs.crate_from_source_path("/usr/include/stdint.h"))
        self.assertIsNone(
            cs.crate_from_source_path("/root/.cargo/registry/src/foo/fission-pcode/lib.rs")
        )


class LcovEvaluationTests(unittest.TestCase):
    def _eval_text(self, lcov_text: str, threshold: str = "60") -> cs.CoverageReport:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "lcov.info"
            path.write_text(lcov_text, encoding="utf-8")
            return cs.evaluate_coverage(path, "fission-pcode", threshold)

    def test_fission_pcode_ir_does_not_bleed_into_fission_pcode(self) -> None:
        lcov = (
            "TN:\n"
            + record("/repo/crates/fission-pcode-ir/src/lib.rs", 100, 100)
            + record("/usr/include/stdlib.h", 0, 50)
            + record("/repo/crates/fission-pcode/src/midend/a.rs", 8, 20)
        )
        report = self._eval_text(lcov, threshold="60")
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_BELOW_THRESHOLD)
        self.assertEqual(report.exit_code(), 0)
        self.assertEqual(report.target_hit, 8)
        self.assertEqual(report.target_total, 20)
        self.assertAlmostEqual(report.target_pct or 0.0, 40.0)
        self.assertEqual(report.crates["fission-pcode-ir"].hit, 100)
        self.assertEqual(report.crates["fission-pcode-ir"].total, 100)

    def test_measured_pass_above_threshold(self) -> None:
        lcov = record("crates/fission-pcode/src/lib.rs", 7, 10)
        report = self._eval_text(lcov, threshold="60")
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS)
        self.assertEqual(report.exit_code(), 0)
        self.assertAlmostEqual(report.target_pct or 0.0, 70.0)

    def test_da_fallback_without_lf_lh(self) -> None:
        lcov = "\n".join(
            [
                "SF:crates/fission-pcode/src/lib.rs",
                "DA:1,3",
                "DA:2,0",
                "DA:3,1",
                "DA:4,2",
                "end_of_record",
            ]
        )
        report = self._eval_text(lcov, threshold="75")
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS)
        self.assertEqual(report.target_hit, 3)
        self.assertEqual(report.target_total, 4)

    def test_missing_file_empty_file_and_missing_target_crate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "does_not_exist.info"
            report_missing = cs.evaluate_coverage(missing, "fission-pcode", "60")
            self.assertEqual(report_missing.status, cs.EvidenceStatus.MISSING_EVIDENCE)
            self.assertEqual(report_missing.exit_code(), 2)

        report_empty = self._eval_text("   \n\n")
        self.assertEqual(report_empty.status, cs.EvidenceStatus.MISSING_EVIDENCE)
        self.assertEqual(report_empty.exit_code(), 2)

        report_wrong_crate = self._eval_text(record("crates/fission-pcode-ir/src/lib.rs", 10, 10))
        self.assertEqual(report_wrong_crate.status, cs.EvidenceStatus.MISSING_EVIDENCE)
        self.assertEqual(report_wrong_crate.exit_code(), 2)

        report_zero_lines = self._eval_text(
            "SF:crates/fission-pcode/src/lib.rs\nLF:0\nLH:0\nend_of_record\n"
        )
        self.assertEqual(report_zero_lines.status, cs.EvidenceStatus.MISSING_EVIDENCE)
        self.assertEqual(report_zero_lines.exit_code(), 2)

    def test_malformed_lcov_rejected(self) -> None:
        malformed_cases = [
            "SF:crates/fission-pcode/src/lib.rs\nLF:10\nLH:5\n",  # unterminated
            "SF:crates/fission-pcode/src/a.rs\nSF:crates/fission-pcode/src/b.rs\nend_of_record\n",
            "SF:crates/fission-pcode/src/a.rs\nLF:not_an_int\nend_of_record\n",
            "SF:crates/fission-pcode/src/a.rs\nLF:5\nLH:6\nend_of_record\n",  # hit > total
            "LF:5\n",  # outside record
            "end_of_record\n",
        ]
        for body in malformed_cases:
            with self.subTest(body=body):
                report = self._eval_text(body)
                self.assertEqual(report.status, cs.EvidenceStatus.MALFORMED_EVIDENCE)
                self.assertEqual(report.exit_code(), 2)

    def test_checker_error_on_bad_threshold_or_failed_generate_step(self) -> None:
        report_bad_cfg = self._eval_text(
            record("crates/fission-pcode/src/lib.rs", 10, 10),
            threshold="${FISSION_IN_WARN_THRESHOLD_PCT}",
        )
        self.assertEqual(report_bad_cfg.status, cs.EvidenceStatus.CHECKER_ERROR)
        self.assertEqual(report_bad_cfg.exit_code(), 2)

        with tempfile.TemporaryDirectory() as tmp:
            lcov = Path(tmp) / "lcov.info"
            lcov.write_text(record("crates/fission-pcode/src/lib.rs", 10, 10), encoding="utf-8")
            # A failed measurement is not a checker fault and not merely absent
            # evidence: the file may exist, but it came from a failed run.
            for outcome in ("failure", "cancelled", "skipped"):
                with self.subTest(outcome=outcome):
                    report_gen_fail = cs.evaluate_coverage(
                        lcov, "fission-pcode", "60", generate_outcome=outcome
                    )
                    self.assertEqual(
                        report_gen_fail.status, cs.EvidenceStatus.MEASUREMENT_FAILED
                    )
                    self.assertEqual(report_gen_fail.exit_code(), 2)
                    self.assertEqual(report_gen_fail.target_total, 0)
                    self.assertIsNone(report_gen_fail.target_pct)

    def test_cli_reads_env_and_writes_summary_json_and_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lcov = root / "lcov.info"
            summary = root / "summary.md"
            gh_out = root / "gh_output.txt"
            json_out = root / "report.json"
            lcov.write_text(
                record("crates/fission-pcode/src/lib.rs", 30, 40)
                + record("crates/fission-loader/src/lib.rs", 10, 20),
                encoding="utf-8",
            )
            with patch.dict(
                os.environ,
                {
                    "FISSION_IN_WARN_THRESHOLD_PCT": "60",
                    "GITHUB_STEP_SUMMARY": str(summary),
                    "GITHUB_OUTPUT": str(gh_out),
                },
                clear=False,
            ):
                rc = cs.main(
                    [
                        "--lcov",
                        str(lcov),
                        "--target-crate",
                        "fission-pcode",
                        "--json-out",
                        str(json_out),
                    ]
                )
            self.assertEqual(rc, 0)
            payload = json.loads(json_out.read_text(encoding="utf-8"))
            self.assertEqual(payload["status"], cs.EvidenceStatus.MEASURED_PASS)
            self.assertEqual(payload["target_hit"], 30)
            self.assertEqual(payload["target_total"], 40)
            gh_text = gh_out.read_text(encoding="utf-8")
            self.assertIn("coverage_status=measured_pass\n", gh_text)
            self.assertIn("target_pct=75.00\n", gh_text)
            md = summary.read_text(encoding="utf-8")
            self.assertIn("`measured_pass`", md)
            self.assertIn("| `fission-pcode` | 1 | 30 | 40 | 75.00% |", md)
            self.assertIn("| `fission-loader` | 1 | 10 | 20 | 50.00% |", md)


class LcovConsistencyTests(unittest.TestCase):
    """LF/LH must agree with DA; duplicates and summary-only records are defined."""

    def _eval_text(self, lcov_text: str, threshold: str = "60") -> cs.CoverageReport:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "lcov.info"
            path.write_text(lcov_text, encoding="utf-8")
            return cs.evaluate_coverage(path, "fission-pcode", threshold)

    def assertMalformed(self, lcov_text: str, needle: str) -> None:
        report = self._eval_text(lcov_text)
        self.assertEqual(report.status, cs.EvidenceStatus.MALFORMED_EVIDENCE, report.detail)
        self.assertEqual(report.exit_code(), 2)
        self.assertIsNone(report.target_pct)
        self.assertIn(needle, report.detail)

    def test_review_case_lf_lh_contradicting_da_is_rejected(self) -> None:
        # The reviewed input: its only line is unhit, its summary claims 1/1.
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,0\nLF:1\nLH:1\nend_of_record\n",
            "LH:1",
        )

    def test_lf_contradicting_da_is_rejected(self) -> None:
        # LF counts more lines than DA lists (also what a truncated `LF:12`
        # -> `LF:1` looks like when DA was intact, in the other direction).
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,1\nDA:2,0\nLF:5\nLH:1\nend_of_record\n",
            "LF:5",
        )
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,1\nDA:2,1\nDA:3,0\nLF:1\nend_of_record\n",
            "LF:1",
        )

    def test_lh_alone_contradicting_da_is_rejected(self) -> None:
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,0\nDA:2,0\nLH:2\nend_of_record\n",
            "LH:2",
        )

    def test_duplicate_da_lines_count_once_and_hits_sum(self) -> None:
        # Line 1 listed twice. Counting entries gave 2/3 = 66.7% (pass at 60);
        # the file has two lines and one executed: 1/2 = 50% (below).
        report = self._eval_text(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,1\nDA:1,1\nDA:2,0\nend_of_record\n"
        )
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_BELOW_THRESHOLD)
        self.assertEqual((report.target_hit, report.target_total), (1, 2))
        self.assertEqual(report.duplicate_da_merged, 1)

        # An unhit and a hit entry for the same line merge to "hit".
        report = self._eval_text(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,0\nDA:1,4\nDA:2,1\nLF:2\nLH:2\nend_of_record\n"
        )
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS, report.detail)
        self.assertEqual((report.target_hit, report.target_total), (2, 2))

    def test_lf_counting_duplicate_da_entries_is_rejected(self) -> None:
        # LF/LH describe unique lines; a summary that counted the duplicate
        # entry disagrees with the line data.
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,1\nDA:1,1\nDA:2,0\nLF:3\nLH:2\nend_of_record\n",
            "LF:3",
        )

    def test_repeated_source_file_records_merge_by_line(self) -> None:
        # Two records for one file (e.g. two TN sections) describe the same
        # two lines, each executed by one of them: 2/2, not 2/4.
        lcov = (
            "TN:a\nSF:crates/fission-pcode/src/a.rs\nDA:1,0\nDA:2,1\nLF:2\nLH:1\nend_of_record\n"
            "TN:b\nSF:crates/fission-pcode/src/a.rs\nDA:1,3\nDA:2,0\nLF:2\nLH:1\nend_of_record\n"
        )
        report = self._eval_text(lcov)
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS, report.detail)
        self.assertEqual((report.target_hit, report.target_total), (2, 2))
        self.assertEqual(report.record_count, 2)
        self.assertEqual(report.duplicate_source_records, 1)
        self.assertEqual(report.crates["fission-pcode"].files, 1)

    def test_summary_only_record_with_lines_is_rejected(self) -> None:
        # No DA means nothing to check LF/LH against; llvm-cov never writes it.
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nLF:10\nLH:7\nend_of_record\n",
            "without DA",
        )

    def test_lf_without_lh_or_da_is_not_measured_as_zero(self) -> None:
        # Previously LF came from the summary and the hit count from (absent)
        # DA, publishing a fabricated 0/10 measurement.
        self.assertMalformed(
            "SF:crates/fission-pcode/src/a.rs\nLF:10\nend_of_record\n",
            "without DA",
        )

    def test_summary_only_zero_line_record_is_accepted(self) -> None:
        lcov = (
            "SF:crates/fission-pcode/src/empty.rs\nLF:0\nLH:0\nend_of_record\n"
            + record("crates/fission-pcode/src/a.rs", 3, 4)
        )
        report = self._eval_text(lcov)
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS, report.detail)
        self.assertEqual((report.target_hit, report.target_total), (3, 4))

    def test_malformed_directives_are_rejected(self) -> None:
        sf = "SF:crates/fission-pcode/src/a.rs\n"
        cases = {
            "garbage line": (sf + "DA:1,1\nGARBAGE LINE\nend_of_record\n", "line 3"),
            "unknown directive": (sf + "DA:1,1\nXYZ:1\nend_of_record\n", "XYZ"),
            "DA one field": (sf + "DA:1\nend_of_record\n", "DA"),
            "DA four fields": (sf + "DA:1,1,abc,extra\nend_of_record\n", "DA"),
            "DA line zero": (sf + "DA:0,1\nend_of_record\n", "DA line"),
            "DA negative hits": (sf + "DA:1,-1\nend_of_record\n", "DA hits"),
            "empty SF path": ("SF:\nDA:1,1\nend_of_record\n", "SF"),
            "truncated end_of_record": (sf + "DA:1,1\nend_of_rec", "end_of_rec"),
            "truncated DA at EOF": (sf + "DA:1,1\nDA:2", "DA"),
            "DA outside a record": ("DA:1,1\n", "outside"),
        }
        for name, (body, needle) in cases.items():
            with self.subTest(name=name):
                self.assertMalformed(body, needle)

    def test_da_checksum_field_is_accepted(self) -> None:
        # geninfo's optional third DA field (line checksum) is valid LCOV.
        report = self._eval_text(
            "SF:crates/fission-pcode/src/a.rs\nDA:1,1,abcd\nDA:2,1,ef01\nLF:2\nLH:2\nend_of_record\n"
        )
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS, report.detail)

    def test_real_llvm_cov_export_is_accepted_with_exact_counts(self) -> None:
        report = self._eval_text(REAL_LLVM_COV_LCOV)
        self.assertEqual(report.status, cs.EvidenceStatus.MEASURED_PASS, report.detail)
        self.assertEqual((report.target_hit, report.target_total), (9, 11))
        self.assertEqual(report.crates["fission-pcode"].files, 2)
        self.assertEqual(report.duplicate_da_merged, 0)
        self.assertEqual(report.duplicate_source_records, 0)


class EvidenceSingleSourceTests(unittest.TestCase):
    """The step summary, GITHUB_OUTPUT and JSON evidence are one payload."""

    def _run_cli(self, lcov_text: str | None, *extra: str) -> tuple[int, dict, str, dict]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lcov = root / "lcov.info"
            if lcov_text is not None:
                lcov.write_text(lcov_text, encoding="utf-8")
            summary, gh_out, json_out = root / "s.md", root / "o.txt", root / "r.json"
            rc = cs.main(
                [
                    "--lcov", str(lcov),
                    "--threshold", "60",
                    "--json-out", str(json_out),
                    "--step-summary", str(summary),
                    "--github-output", str(gh_out),
                    *extra,
                ]
            )
            payload = json.loads(json_out.read_text(encoding="utf-8"))
            outputs = dict(
                line.split("=", 1)
                for line in gh_out.read_text(encoding="utf-8").splitlines()
            )
            return rc, payload, summary.read_text(encoding="utf-8"), outputs

    def _assert_agree(self, payload: dict, md: str, outputs: dict) -> None:
        self.assertEqual(outputs["coverage_status"], payload["status"])
        self.assertEqual(outputs["coverage_detail"], payload["detail"])
        self.assertEqual(outputs["target_pct"], cs.format_pct(payload["target_pct"]))
        self.assertIn(f"`{payload['status']}`", md)
        self.assertIn(payload["detail"], md)
        self.assertIn(
            f"{payload['target_hit']}/{payload['target_total']} lines "
            f"({cs.format_pct(payload['target_pct'])}",
            md,
        )
        for name, row in payload["crates"].items():
            self.assertIn(
                f"| `{name}` | {row['files']} | {row['hit']} | {row['total']} "
                f"| {cs.format_pct_cell(row['pct'])} |",
                md,
            )

    def test_measured_payload_matches_summary_and_outputs(self) -> None:
        rc, payload, md, outputs = self._run_cli(
            record("crates/fission-pcode/src/lib.rs", 2, 3)
            + record("crates/fission-loader/src/lib.rs", 1, 3)
            + "SF:crates/fission-cspec/src/empty.rs\nLF:0\nLH:0\nend_of_record\n"
        )
        self.assertEqual(rc, 0)
        self.assertEqual(payload["status"], cs.EvidenceStatus.MEASURED_PASS)
        self.assertEqual(payload["schema"], cs.SCHEMA)
        # The JSON keeps every crate, including the zero-line one; the summary
        # table prints exactly those rows, so the two cannot drift.
        self.assertEqual(
            sorted(payload["crates"]), ["fission-cspec", "fission-loader", "fission-pcode"]
        )
        self._assert_agree(payload, md, outputs)
        self.assertEqual(outputs["target_pct"], "66.67")

    def test_failure_payloads_match_summary_and_outputs(self) -> None:
        cases = {
            cs.EvidenceStatus.MISSING_EVIDENCE: (None, ()),
            cs.EvidenceStatus.MALFORMED_EVIDENCE: (
                "SF:crates/fission-pcode/src/a.rs\nDA:1,0\nLF:1\nLH:1\nend_of_record\n",
                (),
            ),
            cs.EvidenceStatus.MEASUREMENT_FAILED: (
                record("crates/fission-pcode/src/lib.rs", 3, 3),
                ("--generate-outcome", "failure"),
            ),
        }
        for status, (body, extra) in cases.items():
            with self.subTest(status=status):
                rc, payload, md, outputs = self._run_cli(body, *extra)
                self.assertEqual(rc, 2)
                self.assertEqual(payload["status"], status)
                self.assertIsNone(payload["target_pct"])
                self.assertEqual(outputs["target_pct"], "")
                self._assert_agree(payload, md, outputs)

    def test_unexpected_checker_exception_still_publishes_checker_error(self) -> None:
        with patch.object(cs, "evaluate_coverage", side_effect=RuntimeError("boom")):
            rc, payload, md, outputs = self._run_cli(record("crates/fission-pcode/src/a.rs", 1, 1))
        self.assertEqual(rc, 2)
        self.assertEqual(payload["status"], cs.EvidenceStatus.CHECKER_ERROR)
        self.assertIn("RuntimeError: boom", payload["detail"])
        self._assert_agree(payload, md, outputs)


class HeavySummaryContractTests(unittest.TestCase):
    """ci-heavy.yml consumes the reusable workflow's coverage outputs."""

    def setUp(self) -> None:
        self.heavy = (REPO_ROOT / ".github/workflows/ci-heavy.yml").read_text(encoding="utf-8")

    def test_every_status_is_handled_by_heavy_summary(self) -> None:
        for status in cs.EvidenceStatus.ALL:
            with self.subTest(status=status):
                self.assertRegex(self.heavy, rf"(^|[\s|]){status}(\)|\|)")

    def test_coverage_outputs_reach_the_shell_only_through_env(self) -> None:
        # Interpolating `${{ ... }}` into `run:` splices text (the detail can
        # quote LCOV content) into the script before bash parses it.
        for line in self.heavy.splitlines():
            if "needs.coverage.outputs." in line:
                with self.subTest(line=line.strip()):
                    self.assertRegex(line, r"^\s+COVERAGE_[A-Z_]+: \$\{\{ needs\.coverage\.outputs\.")


if __name__ == "__main__":
    unittest.main()
