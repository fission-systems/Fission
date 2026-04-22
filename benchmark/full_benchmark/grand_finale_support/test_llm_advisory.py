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

from grand_finale_support.compact_summary import (
    COMPACT_SUMMARY_FILENAME,
    build_single_compact_summary,
)
from grand_finale_support.llm_advisory import (
    ENV_ENABLE,
    build_benchmark_llm_input,
    maybe_generate_benchmark_llm_advisory,
)
from grand_finale_support.render_console import print_single_benchmark_console
from grand_finale_support.render_markdown import render_single_benchmark_markdown
from rich.console import Console


class BenchmarkLlmAdvisoryTests(unittest.TestCase):
    def _single_summary_payload(self) -> dict[str, object]:
        return {
            "summary": {
                "binary": "/tmp/putty.exe",
                "generated_at": "2026-04-20 12:00:00",
                "public_summary_line": "ok",
                "quality": {
                    "pyghidra_vs_fission": {
                        "avg_normalized_similarity": 38.82,
                        "aggregate_normalized_similarity": 40.11,
                        "both_success_count": 50,
                        "shared_count": 50,
                    }
                },
                "kpi": {
                    "intersection": {
                        "pyghidra_vs_fission": {
                            "both_success_rate_pct": 95.0,
                            "high_divergence_pct": 0.0,
                        }
                    }
                },
                "owner_metrics": {"fission": {"alias_unsafe": 4.0}},
                "shape_drift_metrics": {"fission": {"generic_local_name_sum": 3.0}},
                "normalize_pass_metrics": {
                    "fission": {"wide_dead_assignment_total_time_ms": 12.0}
                },
                "giant_function_speed_family_counts": {"RenderHeavy": 1},
                "max_pathological_examples": [
                    {
                        "address": "0x140002d40",
                        "name": "register_frame_ctor",
                        "giant_function_speed_family": "RenderHeavy",
                        "rendered_code_len": 452822,
                    }
                ],
                "target_structuring_rows": [
                    {
                        "address": "0x140001470",
                        "name": "fibonacci",
                        "structuring_duration_ms": 33.0,
                        "forced_linear_structuring_count": 1,
                        "rendered_code_len": 40935,
                        "current_normalized_similarity": 11.65,
                    }
                ],
                "engines": {"fission": {"function_count": 50}},
                "samples": {"pyghidra_vs_fission_lowest_similarity": []},
                "row_fidelity_targets": {
                    "pyghidra_vs_fission": {
                        "watchlist_source": "dynamic",
                        "watchlist_diagnostics": {"selected_because_counts": {"baseline_low_similarity": 2}},
                    }
                },
            }
        }

    def test_build_single_benchmark_llm_input_keeps_compact_metrics(self) -> None:
        summary_payload = self._single_summary_payload()
        summary_payload["summary"]["engines"]["fission"].update(
            {
                "goto_total": 12,
                "top_level_label_total": 4,
                "materialization_stabilized_count": 3,
                "proof_payload_direct_emit_count": 2,
                "guarded_tail_promoted_count": 1,
            }
        )
        summary_payload["summary"]["samples"]["pyghidra_vs_fission_lowest_similarity"] = [
            {
                "address": "0x140008090",
                "fission_name": "FUN_0x140008090",
                "normalized_similarity": 35.26,
                "fission_has_dispatcher_recovery": True,
            }
        ]
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
        summary_payload = self._single_summary_payload()
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
        summary_payload = self._single_summary_payload()
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

    def test_build_single_compact_summary_caps_and_keeps_expected_fields(self) -> None:
        compact = build_single_compact_summary(self._single_summary_payload())
        payload = compact.model_dump(mode="json")
        self.assertEqual(payload["summary_kind"], "compact_single_benchmark")
        self.assertEqual(payload["owner_metrics"]["alias_unsafe"], 4.0)
        self.assertEqual(payload["shape_drift_metrics"]["generic_local_name_sum"], 3.0)
        self.assertEqual(payload["normalize_pass_metrics"]["wide_dead_assignment_total_time_ms"], 12.0)
        self.assertEqual(payload["giant_function_speed_family_counts"]["RenderHeavy"], 1)
        self.assertEqual(payload["max_pathological_examples"][0]["address"], "0x140002d40")

    def test_build_single_compact_summary_attaches_target_row_delta_from_regression_gate(self) -> None:
        compact = build_single_compact_summary(
            self._single_summary_payload(),
            regression_gate_payload={
                "row_fidelity_gate": {
                    "rows": [
                        {
                            "address": "0x140001470",
                            "role": "dynamic_low_similarity",
                            "status": "unchanged",
                            "previous_normalized_similarity": 11.65,
                            "current_normalized_similarity": 11.65,
                            "normalized_similarity_delta": 0.0,
                            "failure_reasons": [],
                        }
                    ]
                }
            },
        )
        payload = compact.model_dump(mode="json")
        row = payload["target_structuring_rows"][0]
        self.assertEqual(row["watchlist_role"], "dynamic_low_similarity")
        self.assertEqual(row["row_gate_status"], "unchanged")
        self.assertEqual(row["previous_normalized_similarity"], 11.65)
        self.assertEqual(row["current_normalized_similarity"], 11.65)
        self.assertEqual(row["normalized_similarity_delta"], 0.0)
        self.assertEqual(payload["unchanged_target_rows"][0]["name"], "fibonacci")

    def test_maybe_generate_benchmark_llm_advisory_prefers_compact_summary_when_present(self) -> None:
        summary_payload = self._single_summary_payload()
        compact_payload = build_single_compact_summary(summary_payload).model_dump(mode="json")
        with tempfile.TemporaryDirectory() as tmp_dir:
            output_dir = Path(tmp_dir)
            summary_path = output_dir / "benchmark_summary.json"
            compact_path = output_dir / COMPACT_SUMMARY_FILENAME
            summary_path.write_text(json.dumps(summary_payload), encoding="utf-8")
            compact_path.write_text(json.dumps(compact_payload), encoding="utf-8")

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

            llm_input = json.loads((output_dir / "benchmark_llm_input.json").read_text(encoding="utf-8"))
            self.assertEqual(llm_input["summary_kind"], "compact_single_benchmark")
            self.assertEqual(metadata["compact_summary_source"], str(compact_path.resolve()))

    def test_render_single_markdown_and_console_smoke(self) -> None:
        payload = self._single_summary_payload()
        payload["summary"]["unchanged_target_rows"] = [
            {
                "address": "0x140001470",
                "name": "fibonacci",
                "current_normalized_similarity": 11.65,
                "previous_normalized_similarity": 11.65,
                "normalized_similarity_delta": 0.0,
            }
        ]
        markdown = render_single_benchmark_markdown(payload)
        self.assertIn("## Owner Metrics", markdown)
        self.assertIn("## Normalize Pass Metrics", markdown)
        self.assertIn("## Giant Function Diagnostics", markdown)
        self.assertIn("### Unchanged Target Rows", markdown)
        self.assertIn("benchmark_compact_summary.json", markdown)

        console = Console(record=True, width=120)
        print_single_benchmark_console(payload["summary"], Path("/tmp/out"), console=console)
        rendered = console.export_text()
        self.assertIn("Whole Decomp Benchmark", rendered)
        self.assertIn("Giant Function Families", rendered)


if __name__ == "__main__":
    unittest.main()
