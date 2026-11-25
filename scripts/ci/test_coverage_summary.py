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
        lcov = "\n".join(
            [
                "TN:",
                "SF:/repo/crates/fission-pcode-ir/src/lib.rs",
                "LF:100",
                "LH:100",
                "end_of_record",
                "SF:/usr/include/stdlib.h",
                "LF:50",
                "LH:0",
                "end_of_record",
                "SF:/repo/crates/fission-pcode/src/midend/a.rs",
                "LF:20",
                "LH:8",
                "end_of_record",
                "",
            ]
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
        lcov = "\n".join(
            [
                "SF:crates/fission-pcode/src/lib.rs",
                "LF:10",
                "LH:7",
                "end_of_record",
            ]
        )
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

        report_wrong_crate = self._eval_text(
            "SF:crates/fission-pcode-ir/src/lib.rs\nLF:10\nLH:10\nend_of_record\n"
        )
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
            "SF:crates/fission-pcode/src/lib.rs\nLF:10\nLH:10\nend_of_record\n",
            threshold="${FISSION_IN_WARN_THRESHOLD_PCT}",
        )
        self.assertEqual(report_bad_cfg.status, cs.EvidenceStatus.CHECKER_ERROR)
        self.assertEqual(report_bad_cfg.exit_code(), 2)

        with tempfile.TemporaryDirectory() as tmp:
            lcov = Path(tmp) / "lcov.info"
            lcov.write_text(
                "SF:crates/fission-pcode/src/lib.rs\nLF:10\nLH:10\nend_of_record\n",
                encoding="utf-8",
            )
            report_gen_fail = cs.evaluate_coverage(
                lcov, "fission-pcode", "60", generate_outcome="failure"
            )
            self.assertEqual(report_gen_fail.status, cs.EvidenceStatus.CHECKER_ERROR)
            self.assertEqual(report_gen_fail.exit_code(), 2)

    def test_cli_reads_env_and_writes_summary_json_and_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            lcov = root / "lcov.info"
            summary = root / "summary.md"
            gh_out = root / "gh_output.txt"
            json_out = root / "report.json"
            lcov.write_text(
                "\n".join(
                    [
                        "SF:crates/fission-pcode/src/lib.rs",
                        "LF:40",
                        "LH:30",
                        "end_of_record",
                        "SF:crates/fission-loader/src/lib.rs",
                        "LF:20",
                        "LH:10",
                        "end_of_record",
                    ]
                ),
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
            self.assertIn("| `fission-pcode` | 30 | 40 | 75.0% |", md)
            self.assertIn("| `fission-loader` | 10 | 20 | 50.0% |", md)


if __name__ == "__main__":
    unittest.main()
