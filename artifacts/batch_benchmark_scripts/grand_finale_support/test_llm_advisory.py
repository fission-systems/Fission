from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT_ROOT = Path(__file__).resolve().parent.parent
if str(SCRIPT_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPT_ROOT))

from grand_finale_support.llm_advisory import (
    ENV_ENABLE,
    build_benchmark_llm_input,
    maybe_generate_benchmark_llm_advisory,
)


class BenchmarkLlmAdvisoryTests(unittest.TestCase):
    def test_build_single_benchmark_llm_input_keeps_compact_metrics(self) -> None:
        summary_payload = {
            "summary": {
                "binary": "/tmp/putty.exe",
                "generated_at": "2026-04-20 12:00:00",
                "quality": {
                    "pyghidra_vs_fission": {
                        "avg_normalized_similarity": 38.82,
                        "aggregate_normalized_similarity": 40.11,
                        "both_success_count": 50,
                        "shared_count": 50,
                    }
                },
                "engines": {
                    "fission": {
                        "function_count": 50,
                        "goto_total": 12,
                        "top_level_label_total": 4,
                        "materialization_stabilized_count": 3,
                        "proof_payload_direct_emit_count": 2,
                        "guarded_tail_promoted_count": 1,
                    }
                },
                "samples": {
                    "pyghidra_vs_fission_lowest_similarity": [
                        {
                            "address": "0x140008090",
                            "fission_name": "FUN_0x140008090",
                            "normalized_similarity": 35.26,
                            "fission_has_dispatcher_recovery": True,
                        }
                    ]
                },
            }
        }
        delta_payload = {
            "metrics": [
                {
                    "key": "avg_normalized_similarity_pct",
                    "previous": 38.82,
                    "current": 38.66,
                    "delta": -0.16,
                    "status": "degraded",
                }
            ],
            "degraded_functions": {
                "top_degraded": [
                    {
                        "address": "0x140008900",
                        "fission_name": "FUN_0x140008900",
                        "previous_normalized_similarity": 23.62,
                        "current_normalized_similarity": 20.82,
                        "normalized_similarity_delta": -2.8,
                        "reason_tags": ["materialization_drift"],
                    }
                ]
            },
        }
        gate_payload = {
            "regressions": ["row_fidelity_gate failed"],
            "row_fidelity_gate": {
                "failed_targets": ["0x140008900"],
                "rows": [
                    {
                        "address": "0x140008900",
                        "role": "secondary",
                        "status": "degraded",
                        "previous_normalized_similarity": 23.62,
                        "current_normalized_similarity": 20.82,
                        "normalized_similarity_delta": -2.8,
                        "failure_reasons": ["materialization_drift"],
                    },
                    {
                        "address": "0x140001160",
                        "role": "primary",
                        "status": "improved",
                        "previous_normalized_similarity": 55.0,
                        "current_normalized_similarity": 56.0,
                        "normalized_similarity_delta": 1.0,
                        "failure_reasons": [],
                    },
                ],
            },
            "top_degraded_functions": delta_payload["degraded_functions"],
        }

        llm_input = build_benchmark_llm_input(
            summary_payload=summary_payload,
            summary_json_path=Path("/tmp/benchmark_summary.json"),
            delta_payload=delta_payload,
            delta_json_path=Path("/tmp/benchmark_delta_vs_previous.json"),
            regression_gate_payload=gate_payload,
            regression_gate_json_path=Path("/tmp/benchmark_regression_gate.json"),
        )

        self.assertEqual(llm_input["summary_kind"], "single_benchmark")
        self.assertTrue(llm_input["policy_context"]["same_axis_comparable"])
        self.assertEqual(
            llm_input["core_metrics"]["avg_normalized_similarity"]["current"],
            38.82,
        )
        self.assertEqual(
            llm_input["core_metrics"]["avg_normalized_similarity"]["previous"],
            38.82,
        )
        self.assertEqual(llm_input["top_regressions"][0]["address"], "0x140008900")
        self.assertEqual(llm_input["top_recoveries"][0]["address"], "0x140001160")

    def test_maybe_generate_benchmark_llm_advisory_writes_success_artifacts(self) -> None:
        summary_payload = {
            "summary": {
                "binary": "/tmp/putty.exe",
                "generated_at": "2026-04-20 12:00:00",
                "quality": {"pyghidra_vs_fission": {"avg_normalized_similarity": 38.82}},
                "engines": {"fission": {"function_count": 50}},
                "samples": {"pyghidra_vs_fission_lowest_similarity": []},
            }
        }
        with tempfile.TemporaryDirectory() as tmp_dir:
            output_dir = Path(tmp_dir)
            summary_path = output_dir / "benchmark_summary.json"
            summary_path.write_text(json.dumps(summary_payload), encoding="utf-8")

            with mock.patch.dict(os.environ, {ENV_ENABLE: "1"}, clear=False):
                with mock.patch(
                    "grand_finale_support.llm_advisory._generate_llm_markdown",
                    return_value="# Advisory\n\n## Same-Axis Comparability\n",
                ):
                    metadata = maybe_generate_benchmark_llm_advisory(
                        output_dir=output_dir,
                        summary_json_path=summary_path,
                        delta_json_path=None,
                        regression_gate_json_path=None,
                    )

            self.assertIsNotNone(metadata)
            self.assertTrue(metadata["success"])
            self.assertTrue((output_dir / "benchmark_llm_input.json").is_file())
            self.assertTrue((output_dir / "benchmark_llm_summary.md").is_file())
            self.assertTrue((output_dir / "benchmark_llm_summary.json").is_file())

    def test_maybe_generate_benchmark_llm_advisory_is_non_blocking_on_failure(self) -> None:
        summary_payload = {
            "summary": {
                "binary": "/tmp/putty.exe",
                "generated_at": "2026-04-20 12:00:00",
                "quality": {"pyghidra_vs_fission": {"avg_normalized_similarity": 38.82}},
                "engines": {"fission": {"function_count": 50}},
                "samples": {"pyghidra_vs_fission_lowest_similarity": []},
            }
        }
        with tempfile.TemporaryDirectory() as tmp_dir:
            output_dir = Path(tmp_dir)
            summary_path = output_dir / "benchmark_summary.json"
            summary_path.write_text(json.dumps(summary_payload), encoding="utf-8")

            with mock.patch.dict(os.environ, {ENV_ENABLE: "1"}, clear=False):
                with mock.patch(
                    "grand_finale_support.llm_advisory._generate_llm_markdown",
                    side_effect=RuntimeError("LM Studio down"),
                ):
                    metadata = maybe_generate_benchmark_llm_advisory(
                        output_dir=output_dir,
                        summary_json_path=summary_path,
                        delta_json_path=None,
                        regression_gate_json_path=None,
                    )

            self.assertIsNotNone(metadata)
            self.assertFalse(metadata["success"])
            md_text = (output_dir / "benchmark_llm_summary.md").read_text(encoding="utf-8")
            self.assertIn("Advisory generation failed", md_text)


if __name__ == "__main__":
    unittest.main()
