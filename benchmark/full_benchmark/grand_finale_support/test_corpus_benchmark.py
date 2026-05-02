from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_ROOT = Path(__file__).resolve().parent.parent
if str(SCRIPT_ROOT) not in sys.path:
    sys.path.insert(0, str(SCRIPT_ROOT))

from grand_finale_support.benchmark_core import (
    _annotate_target_structuring_rows,
    _build_target_structuring_rows,
    _build_giant_function_diagnostics,
    _default_binary_output_name,
    _default_corpus_output_name,
    _derive_binary_arch,
    _derive_dynamic_row_targets,
    _extract_alias_interleave_metrics,
    _extract_blockgraph_region_metrics,
    _extract_ghidra_action_metrics,
    _extract_mir_metrics,
    _extract_owner_metrics_from_engine_summary,
    _extract_selected_normalize_pass_metrics,
    _extract_shape_drift_metrics_from_engine_summary,
    _refresh_single_summary_target_rows_from_row_gate,
    _resolve_binary_watchlist,
    build_pairwise_engine_comparison,
    build_seeded_function_set,
    build_corpus_assessment,
    compare_with_previous_summary,
    load_corpus_manifest,
)
from grand_finale_support.compact_summary import build_corpus_compact_summary
from grand_finale_support.metrics import collect_code_metrics
from grand_finale_support.render_console import print_corpus_benchmark_console
from grand_finale_support.render_markdown import render_corpus_benchmark_markdown
from rich.console import Console


def _minimal_single_binary_summary(
    *,
    avg_similarity: float = 40.0,
    owner_alias_unsafe: int = 0,
    generic_local_name_sum: int = 0,
    generic_param_name_sum: int = 0,
    heuristic_max_brace_nesting_mean: float = 0.0,
    synthetic_helper_call_total: int = 0,
) -> dict[str, object]:
    return {
        "summary": {
            "generated_at": "2026-04-21 00:00:00",
            "quality": {"pyghidra_vs_fission": {"avg_normalized_similarity": avg_similarity}},
            "speed": {"fission": {"wall_sec": 1.0, "wall_speedup_vs_pyghidra": 1.0}},
            "kpi": {
                "intersection": {
                    "pyghidra_vs_fission": {
                        "both_success_rate_pct": 100.0,
                        "high_divergence_pct": 0.0,
                    }
                },
                "engines": {
                    "fission": {
                        "quality_kpi": {
                            "success_rate_pct": 100.0,
                            "direct_success_rate_pct": 100.0,
                        },
                        "performance_kpi": {"throughput_func_per_sec": 1.0},
                        "cpu_kpi": {
                            "process_cpu_seconds": 0.75,
                            "process_cpu_user_sec": 0.6,
                            "process_cpu_system_sec": 0.15,
                            "process_cpu_utilization_pct": 75.0,
                            "process_effective_parallelism": 0.75,
                            "func_per_cpu_second": 13.333,
                            "worker_count": 4,
                            "available_parallelism": 14,
                        },
                    }
                },
            },
            "engines": {
                "fission": {
                    "success_count": 10,
                    "goto_total": 0,
                    "readability_control_flow_penalty": 0,
                    "undefined_return_type_total": 0,
                    "replacement_plan_rejected_alias_unsafe_count": owner_alias_unsafe,
                    "replacement_plan_rejected_missing_merge_count": 0,
                    "replacement_plan_rejected_representative_root_attribution_count": 0,
                    "replacement_plan_rejected_temp_only_representative_lifecycle_count": 0,
                    "replacement_plan_rejected_dead_temp_representative_count": 0,
                    "representative_downgrade_count": 0,
                    "representative_downgrade_no_aliassafe_source_count": 0,
                    "representative_downgrade_join_conflict_count": 0,
                    "materialization_stabilized_count": 0,
                    "goto_total": 0,
                    "top_level_label_total": 0,
                    "generic_local_name_sum": generic_local_name_sum,
                    "generic_param_name_sum": generic_param_name_sum,
                    "unknown_type_var_total": 0,
                    "ptr_offset_total": 0,
                    "index_expr_total": 0,
                    "heuristic_avg_line_length_mean": 0.0,
                    "heuristic_max_brace_nesting_mean": heuristic_max_brace_nesting_mean,
                    "synthetic_helper_call_total": synthetic_helper_call_total,
                    "preview_build_stats": {
                        "ghidra_action_stage_count": 6,
                        "ghidra_action_funcdata_build_count": 1,
                        "ghidra_action_heritage_value_recovery_count": 1,
                        "ghidra_action_normalize_count": 1,
                        "ghidra_action_prototype_types_count": 1,
                        "ghidra_action_blockgraph_structuring_count": 1,
                        "ghidra_action_printc_count": 1,
                        "ghidra_clean_room_pipeline_complete_count": 1,
                        "mir_enabled_count": 1,
                        "mir_function_count": 1,
                        "mir_block_count": 1,
                        "mir_value_count": 9,
                        "mir_memory_region_count": 2,
                        "mir_join_proof_count": 1,
                        "mir_region_proof_count": 1,
                        "mir_projection_duration_ms": 3,
                        "mir_blockgraph_admission_enabled_count": 1,
                        "mir_blockgraph_irreducible_budget_bypass_count": 1,
                        "mir_blockgraph_extreme_budget_blocked_count": 0,
                        "blockgraph_region_candidate_count": 5,
                        "blockgraph_region_complete_count": 2,
                        "blockgraph_region_rejected_missing_follow_count": 1,
                        "blockgraph_region_rejected_must_emit_label_count": 1,
                        "blockgraph_region_rejected_middle_ref_count": 1,
                        "blockgraph_region_rejected_external_ref_count": 0,
                        "blockgraph_region_rejected_join_owner_conflict_count": 0,
                        "blockgraph_region_rejected_nonterminal_join_count": 0,
                        "blockgraph_region_rejected_follow_owner_conflict_count": 0,
                        "blockgraph_region_rejected_emit_ready_count": 1,
                        "blockgraph_region_rejected_irreducible_count": 0,
                        "guarded_tail_rejected_alias_interleave_conflict_count": 5,
                        "canonicalization_failed_alias_has_nonlocal_ref_count": 4,
                        "canonicalization_failed_alias_has_nonlocal_ref_external_before_count": 1,
                        "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count": 2,
                        "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count": 1,
                        "canonicalization_failed_alias_not_fallthrough_count": 3,
                        "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count": 2,
                        "canonicalization_failed_alias_not_fallthrough_nested_after_label_count": 1,
                        "canonicalization_failed_alias_has_multiple_internal_predecessors_count": 1,
                        "canonicalization_failed_payload_crosses_join_count": 2,
                        "pass_metrics": {
                            "wide_dead_assignment": {
                                "total_time_ms": 12.0,
                                "total_invocations": 3,
                                "changed_count": 2,
                            }
                        }
                    },
                }
            },
            "owner_metrics": {"fission": {"alias_unsafe": float(owner_alias_unsafe)}},
            "shape_drift_metrics": {
                "fission": {
                    "generic_local_name_sum": float(generic_local_name_sum),
                    "generic_param_name_sum": float(generic_param_name_sum),
                    "heuristic_max_brace_nesting_mean": float(
                        heuristic_max_brace_nesting_mean
                    ),
                    "synthetic_helper_call_total": float(synthetic_helper_call_total),
                }
            },
            "normalize_pass_metrics": {
                "fission": {
                    "wide_dead_assignment_total_time_ms": 12.0,
                    "wide_dead_assignment_total_invocations": 3.0,
                    "wide_dead_assignment_changed_count": 2.0,
                }
            },
            "ghidra_action_metrics": {
                "fission": {
                    "stage_count": 6.0,
                    "funcdata_build": 1.0,
                    "heritage_value_recovery": 1.0,
                    "normalize": 1.0,
                    "prototype_types": 1.0,
                    "blockgraph_structuring": 1.0,
                    "printc": 1.0,
                    "pipeline_complete": 1.0,
                }
            },
            "mir_metrics": {
                "fission": {
                    "enabled": 1.0,
                    "function": 1.0,
                    "block": 1.0,
                    "value": 9.0,
                    "memory_region": 2.0,
                    "join_proof": 1.0,
                    "region_proof": 1.0,
                    "projection_duration_ms": 3.0,
                    "blockgraph_admission_enabled": 1.0,
                    "blockgraph_irreducible_budget_bypass": 1.0,
                    "blockgraph_extreme_budget_blocked": 0.0,
                }
            },
            "blockgraph_region_metrics": {
                "fission": {
                    "candidate": 5.0,
                    "complete": 2.0,
                    "rejected_missing_follow": 1.0,
                    "rejected_must_emit_label": 1.0,
                    "rejected_emit_ready": 1.0,
                    "rejected_irreducible": 0.0,
                }
            },
            "alias_interleave_metrics": {
                "fission": {
                    "alias_interleave_conflict": 5.0,
                    "alias_has_nonlocal_ref": 4.0,
                    "alias_has_nonlocal_ref_external_before": 1.0,
                    "alias_has_nonlocal_ref_nested_before": 2.0,
                    "alias_has_nonlocal_ref_post_segment_ref": 1.0,
                    "alias_not_fallthrough": 3.0,
                    "alias_not_fallthrough_top_level_after_label": 2.0,
                    "alias_not_fallthrough_nested_after_label": 1.0,
                    "alias_has_multiple_internal_predecessors": 1.0,
                    "payload_crosses_join": 2.0,
                }
            },
            "cpu_metrics": {
                "fission": {
                    "process_cpu_seconds": 0.75,
                    "process_cpu_user_sec": 0.6,
                    "process_cpu_system_sec": 0.15,
                    "process_cpu_utilization_pct": 75.0,
                    "process_effective_parallelism": 0.75,
                    "func_per_cpu_second": 13.333,
                    "worker_count": 4,
                    "available_parallelism": 14,
                }
            },
            "giant_function_candidates": 1,
            "giant_function_speed_family_counts": {"RenderHeavy": 1},
            "max_rendered_code_len": 123456,
            "max_structuring_scc_component_count": 222,
            "max_replacement_plan_candidate_count": 33333,
            "max_materialization_stabilized_count": 44444,
            "max_pathological_examples": [
                {
                    "binary_id": "sample",
                    "address": "0x140002d40",
                    "name": "register_frame_ctor",
                    "size": 0,
                    "build_duration_ms": 252142.0,
                    "normalize_duration_ms": 157759.0,
                    "structuring_duration_ms": 64000.0,
                    "render_duration_ms": 30000.0,
                    "rendered_code_len": 452822,
                    "forced_linear_structuring_count": 1,
                    "structuring_scc_component_count": 228,
                    "replacement_plan_candidate_count": 39381,
                    "materialization_stabilized_count": 33633,
                    "giant_function_speed_family": "MixedGiantFunction",
                }
            ],
            "target_structuring_rows": [
                {
                    "binary_id": "sample",
                    "address": "0x140001470",
                    "name": "fibonacci",
                    "current_normalized_similarity": avg_similarity,
                    "previous_normalized_similarity": avg_similarity,
                    "normalized_similarity_delta": 0.0,
                    "row_gate_status": "unchanged",
                    "code_changed": False,
                    "structuring_duration_ms": 33.0,
                    "forced_linear_structuring_count": 1,
                    "rendered_code_len": 40935,
                }
            ],
            "coverage": {
                "pyghidra_vs_fission": {
                    "coverage_ratio_pct": 100.0,
                    "shared_count": 10,
                    "left_total_count": 10,
                    "right_total_count": 10,
                }
            },
            "row_fidelity_targets": {
                "pyghidra_vs_fission": {
                    "watchlist_source": "dynamic",
                    "canonical_quality_rows": [
                        {
                            "address": "0x140001470",
                            "name": "fibonacci",
                            "role": "canonical_quality",
                            "selected_because": "canonical_quality",
                        }
                    ],
                    "bootstrap_row_targets": [],
                    "dynamic_watchlist_rows": [],
                    "watchlist_diagnostics": {
                        "watchlist_source": "dynamic",
                        "canonical_quality_row_count": 1,
                        "bootstrap_row_target_count": 0,
                        "dynamic_watchlist_row_count": 0,
                        "selected_because_counts": {"canonical_quality": 1},
                    },
                    "rows": [
                        {
                            "address": "0x140001470",
                            "role": "canonical_quality",
                            "status": "unchanged",
                            "previous_normalized_similarity": avg_similarity,
                            "current_normalized_similarity": avg_similarity,
                            "normalized_similarity_delta": 0.0,
                            "failure_reasons": [],
                            "previous_code_sha256": "baseline-hash",
                            "current_code_sha256": "baseline-hash",
                            "code_changed": False,
                        }
                    ],
                }
            },
        },
        "pairwise": {"pyghidra_vs_fission": {"comparisons": [], "summary": {}}},
        "engines": {"fission": {"entries": {}}},
    }


class CorpusBenchmarkTests(unittest.TestCase):
    def test_annotate_target_structuring_rows_attaches_row_gate_delta_fields(self) -> None:
        rows = [
            {
                "binary_id": "sample",
                "address": "0x140001470",
                "name": "fibonacci",
                "structuring_duration_ms": 33.0,
                "forced_linear_structuring_count": 1,
                "rendered_code_len": 40935,
            }
        ]
        row_gate = {
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

        annotated = _annotate_target_structuring_rows(rows, row_gate=row_gate)

        self.assertEqual(annotated[0]["watchlist_role"], "dynamic_low_similarity")
        self.assertEqual(annotated[0]["row_gate_status"], "unchanged")
        self.assertEqual(annotated[0]["previous_normalized_similarity"], 11.65)
        self.assertEqual(annotated[0]["current_normalized_similarity"], 11.65)
        self.assertEqual(annotated[0]["normalized_similarity_delta"], 0.0)

    def test_build_target_structuring_rows_does_not_overmatch_shared_addresses_across_binaries(self) -> None:
        entries = {
            "0x140001470": {
                "address": "0x140001470",
                "name": "compare_int_descending",
                "preview_build_stats": {"structuring_duration_ms": 0.0},
            }
        }
        rows = _build_target_structuring_rows(entries, binary_id="function-pointers-strings")
        self.assertEqual(rows, [])

    def test_refresh_single_summary_target_rows_from_row_gate_populates_unchanged_rows(self) -> None:
        benchmark = {
            "summary": {
                "target_structuring_rows": [
                    {
                        "address": "0x140001470",
                        "name": "fibonacci",
                        "structuring_duration_ms": 33.0,
                    }
                ]
            }
        }
        row_gate = {
            "rows": [
                {
                    "address": "0x140001470",
                    "role": "canonical_quality",
                    "status": "unchanged",
                    "previous_normalized_similarity": 11.65,
                    "current_normalized_similarity": 11.65,
                    "normalized_similarity_delta": 0.0,
                    "failure_reasons": [],
                    "previous_code_sha256": "same-hash",
                    "current_code_sha256": "same-hash",
                    "code_changed": False,
                }
            ]
        }

        _refresh_single_summary_target_rows_from_row_gate(benchmark, row_gate)

        row = benchmark["summary"]["target_structuring_rows"][0]
        self.assertEqual(row["row_gate_status"], "unchanged")
        self.assertFalse(row["code_changed"])
        self.assertEqual(benchmark["summary"]["unchanged_target_rows"][0]["name"], "fibonacci")

    def test_checked_in_corpus_manifests_include_required_suite_metadata(self) -> None:
        repo_root = Path(__file__).resolve().parents[3]
        manifest_dir = repo_root / "benchmark" / "config" / "benchmark_corpus"
        required_top = {"name", "suite_tier", "gate_mode", "dynamic_watchlist_limit", "notes", "entries"}

        for manifest_name in ("smoke_corpus.json", "release_corpus.json", "parity_corpus.json"):
            payload = json.loads((manifest_dir / manifest_name).read_text())
            self.assertEqual(required_top - set(payload), set(), manifest_name)
            self.assertIn(payload["suite_tier"], {"smoke", "release", "parity"})
            self.assertIn(payload["gate_mode"], {"advisory", "blocking"})
            self.assertGreater(int(payload["dynamic_watchlist_limit"]), 0)

    def test_checked_in_corpus_manifests_use_repo_relative_benchmark_or_samples_paths(self) -> None:
        manifest_dir = Path(__file__).resolve().parents[3] / "benchmark" / "config" / "benchmark_corpus"

        for manifest_name in ("smoke_corpus.json", "release_corpus.json", "parity_corpus.json"):
            payload = json.loads((manifest_dir / manifest_name).read_text())
            for entry in payload["entries"]:
                bp = Path(entry["binary_path"])
                self.assertFalse(
                    bp.is_absolute(),
                    f"{manifest_name}:{entry['id']} must use repo-relative binary_path",
                )
                posix = bp.as_posix()
                self.assertTrue(
                    posix.startswith("samples/windows/")
                    or posix.startswith("benchmark/binary/"),
                    f"{manifest_name}:{entry['id']} must live under samples/windows or benchmark/binary",
                )
                self.assertIn(
                    _derive_binary_arch(entry),
                    {"x86", "x64"},
                    f"{manifest_name}:{entry['id']} missing x86/x64 arch identity",
                )

    def test_default_output_naming_contract_uses_latest_suffix(self) -> None:
        binary_name = _default_binary_output_name(
            Path("/repo/benchmark/binary/x86-64/window/small/binary/c/test_functions.exe"),
            profile="balanced",
            timestamped=False,
        )
        corpus_name = _default_corpus_output_name(
            manifest_name="fission-smoke-windows-samples",
            manifest_path=Path("/repo/benchmark/config/benchmark_corpus/smoke_corpus.json"),
            profile="balanced",
            timestamped=False,
        )

        self.assertEqual(binary_name, "test_functions-balanced-latest")
        self.assertEqual(corpus_name, "fission-smoke-windows-samples-balanced-latest")

    def test_load_corpus_manifest_accepts_suite_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp = Path(tmp_dir)
            binary = tmp / "sample.exe"
            binary.write_bytes(b"MZ")
            manifest = tmp / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "name": "parity-suite",
                        "suite_tier": "parity",
                        "gate_mode": "advisory",
                        "dynamic_watchlist_limit": 3,
                        "notes": "reference-guided",
                        "entries": [
                            {
                                "id": "sample",
                                "binary_path": str(binary),
                                "ghidra_project_key": "sample",
                                "tags": ["flag-heavy"],
                                "seed_limit": 20,
                                "role": "release_candidate",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            loaded = load_corpus_manifest(manifest)

            self.assertEqual(loaded["suite_tier"], "parity")
            self.assertEqual(loaded["gate_mode"], "advisory")
            self.assertEqual(loaded["dynamic_watchlist_limit"], 3)
            self.assertEqual(loaded["notes"], "reference-guided")
            self.assertEqual(loaded["entries"][0]["suite_tier"], "parity")

    def test_load_corpus_manifest_keeps_canonical_quality_rows(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp = Path(tmp_dir)
            binary = tmp / "sample.exe"
            binary.write_bytes(b"MZ")
            manifest = tmp / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "entries": [
                            {
                                "id": "sample",
                                "binary_path": str(binary),
                                "ghidra_project_key": "sample",
                                "tags": ["x64"],
                                "seed_limit": 20,
                                "role": "release_candidate",
                                "canonical_quality_rows": [
                                    {
                                        "address": "0x140001470",
                                        "name": "fibonacci",
                                        "role": "canonical_quality",
                                    }
                                ],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            loaded = load_corpus_manifest(manifest)

            self.assertEqual(
                loaded["entries"][0]["canonical_quality_rows"],
                [
                    {
                        "address": "0x140001470",
                        "name": "fibonacci",
                        "role": "canonical_quality",
                        "selected_because": "canonical_quality",
                    }
                ],
            )

    def test_build_seeded_function_set_keeps_required_rows_beyond_limit(self) -> None:
        import grand_finale_support.benchmark_core as benchmark_core

        original = benchmark_core.list_functions_with_fission
        try:
            benchmark_core.list_functions_with_fission = lambda *_args, **_kwargs: [
                ("0x1000", "first"),
                ("0x1010", "second"),
                ("0x1020", "third"),
                ("0x2000", "canary"),
            ]

            seeded = build_seeded_function_set(
                binary_path=Path("sample.dll"),
                fission_bin=Path("fission_cli"),
                limit=2,
                timeout_sec=1,
                required_functions=[("0x2000", "call_target_canary")],
            )
        finally:
            benchmark_core.list_functions_with_fission = original

        self.assertEqual(
            seeded,
            [
                ("0x1000", "first"),
                ("0x1010", "second"),
                ("0x2000", "canary"),
            ],
        )

    def test_load_corpus_manifest_defaults_legacy_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp = Path(tmp_dir)
            binary = tmp / "sample.exe"
            binary.write_bytes(b"MZ")
            manifest = tmp / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "entries": [
                            {
                                "id": "sample",
                                "binary_path": str(binary),
                                "ghidra_project_key": "sample",
                                "tags": [],
                                "seed_limit": 20,
                                "role": "release_candidate",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            loaded = load_corpus_manifest(manifest)

            self.assertEqual(loaded["suite_tier"], "release")
            self.assertEqual(loaded["gate_mode"], "advisory")
            self.assertEqual(loaded["dynamic_watchlist_limit"], 5)

    def test_extract_owner_metrics_from_engine_summary(self) -> None:
        engine_summary = {
            "replacement_plan_rejected_alias_unsafe_count": 4,
            "replacement_plan_rejected_missing_merge_count": 5,
            "materialization_stabilized_count": 6,
        }
        metrics = _extract_owner_metrics_from_engine_summary(engine_summary)
        self.assertEqual(metrics["alias_unsafe"], 4.0)
        self.assertEqual(metrics["missing_merge"], 5.0)
        self.assertEqual(metrics["materialization_stabilized"], 6.0)
        self.assertEqual(metrics["call_target_import_resolved"], 0.0)
        self.assertEqual(metrics["call_target_unresolved_sub_fallback"], 0.0)
        self.assertEqual(metrics["dead_temp"], 0.0)

    def test_pairwise_similarity_attribution_adds_owner_buckets(self) -> None:
        pair = build_pairwise_engine_comparison(
            "pyghidra",
            {
                "entries": {
                    "0x1000": {
                        "present": True,
                        "success": True,
                        "name": "FUN_1000",
                        "code": "int FUN_1000(void) { return CloseHandle(1); }",
                        "normalized_code": "int fun void return closehandle 1",
                        "preview_build_stats": {},
                    }
                }
            },
            "fission",
            {
                "entries": {
                    "0x1000": {
                        "present": True,
                        "success": True,
                        "name": "FUN_1000",
                        "code": "undefined FUN_1000(void) { goto LAB_1; }",
                        "normalized_code": "undefined fun void goto lab_1",
                        "preview_build_stats": {
                            "call_target_unresolved_sub_fallback_count": 1,
                            "materialization_stabilized_count": 1,
                            "goto_total": 1,
                            "generic_local_name_sum": 1,
                        },
                    }
                }
            },
            pairwise_similarity_mode="shared-full",
            pairwise_sample_size=5,
            pairwise_auto_shared_full_max=100,
            compute_raw_similarity=False,
            aggregate_similarity_mode="weighted",
        )

        row = pair["comparisons"][0]
        self.assertIn("call_surface_score", row)
        self.assertIn("stack_local_score", row)
        self.assertIn("control_flow_score", row)
        self.assertIn("name_type_score", row)
        self.assertIn("literal_score", row)
        self.assertIn("call_target_missing", row["similarity_owner_buckets"])
        self.assertIn("control_flow_goto_heavy", row["similarity_owner_buckets"])
        self.assertEqual(row["similarity_owner_bucket"], "call_target_missing")
        self.assertGreaterEqual(
            pair["similarity_attribution"]["bucket_counts"]["call_target_missing"],
            1,
        )

    def test_extract_shape_drift_metrics_from_engine_summary(self) -> None:
        engine_summary = {
            "goto_total": 1,
            "generic_local_name_sum": 2,
            "synthetic_helper_call_total": 3,
        }
        metrics = _extract_shape_drift_metrics_from_engine_summary(engine_summary)
        self.assertEqual(metrics["goto_total"], 1.0)
        self.assertEqual(metrics["generic_local_name_sum"], 2.0)
        self.assertEqual(metrics["synthetic_helper_call_total"], 3.0)

    def test_build_giant_function_diagnostics_classifies_zero_size_runtime_wrapper(self) -> None:
        diagnostics = _build_giant_function_diagnostics(
            {
                "0x140002d40": {
                    "address": "0x140002d40",
                    "name": "register_frame_ctor",
                    "size": 0,
                    "preview_build_stats": {
                        "build_duration_ms": 1000,
                        "normalize_duration_ms": 400,
                        "structuring_duration_ms": 300,
                        "render_duration_ms": 300,
                        "rendered_code_len": 120000,
                        "forced_linear_structuring_count": 1,
                        "structuring_scc_component_count": 200,
                        "replacement_plan_candidate_count": 20000,
                        "materialization_stabilized_count": 20000,
                    },
                }
            }
        )
        self.assertEqual(diagnostics["giant_function_candidates"], 1)
        self.assertEqual(
            diagnostics["giant_function_speed_family_counts"]["ZeroSizeRuntimeWrapper"],
            1,
        )
        self.assertEqual(
            diagnostics["max_pathological_examples"][0]["giant_function_speed_family"],
            "ZeroSizeRuntimeWrapper",
        )

    def test_extract_selected_normalize_pass_metrics(self) -> None:
        stats = {
            "pass_metrics": {
                "wide_dead_assignment": {
                    "total_time_ms": 12.5,
                    "total_invocations": 4,
                    "changed_count": 3,
                },
                "jump_resolver": {
                    "total_time_ms": 2.0,
                    "total_invocations": 1,
                    "changed_count": 1,
                },
            }
        }
        metrics = _extract_selected_normalize_pass_metrics(stats)
        self.assertEqual(metrics["wide_dead_assignment"]["total_time_ms"], 12.5)
        self.assertEqual(metrics["wide_dead_assignment"]["total_invocations"], 4.0)
        self.assertEqual(metrics["jump_resolver"]["changed_count"], 1.0)
        self.assertEqual(metrics["sccp"]["total_time_ms"], 0.0)

    def test_extract_ghidra_action_metrics(self) -> None:
        metrics = _extract_ghidra_action_metrics(
            {
                "ghidra_action_stage_count": 6,
                "ghidra_action_funcdata_build_count": 1,
                "ghidra_action_heritage_value_recovery_count": 1,
                "ghidra_action_blockgraph_structuring_count": 1,
                "ghidra_clean_room_pipeline_complete_count": 1,
            }
        )
        self.assertEqual(metrics["stage_count"], 6.0)
        self.assertEqual(metrics["funcdata_build"], 1.0)
        self.assertEqual(metrics["heritage_value_recovery"], 1.0)
        self.assertEqual(metrics["blockgraph_structuring"], 1.0)
        self.assertEqual(metrics["pipeline_complete"], 1.0)
        self.assertEqual(metrics["printc"], 0.0)

    def test_extract_mir_metrics(self) -> None:
        metrics = _extract_mir_metrics(
            {
                "mir_enabled_count": 1,
                "mir_function_count": 1,
                "mir_block_count": 3,
                "mir_value_count": 21,
                "mir_memory_region_count": 4,
                "mir_join_proof_count": 2,
                "mir_region_proof_count": 5,
                "mir_projection_duration_ms": 7,
                "mir_blockgraph_admission_enabled_count": 1,
                "mir_blockgraph_irreducible_budget_bypass_count": 1,
                "mir_blockgraph_extreme_budget_blocked_count": 0,
            }
        )
        self.assertEqual(metrics["enabled"], 1.0)
        self.assertEqual(metrics["function"], 1.0)
        self.assertEqual(metrics["block"], 3.0)
        self.assertEqual(metrics["value"], 21.0)
        self.assertEqual(metrics["memory_region"], 4.0)
        self.assertEqual(metrics["join_proof"], 2.0)
        self.assertEqual(metrics["region_proof"], 5.0)
        self.assertEqual(metrics["projection_duration_ms"], 7.0)
        self.assertEqual(metrics["blockgraph_admission_enabled"], 1.0)
        self.assertEqual(metrics["blockgraph_irreducible_budget_bypass"], 1.0)
        self.assertEqual(metrics["blockgraph_extreme_budget_blocked"], 0.0)

    def test_extract_blockgraph_region_metrics(self) -> None:
        metrics = _extract_blockgraph_region_metrics(
            {
                "blockgraph_region_candidate_count": 7,
                "blockgraph_region_complete_count": 3,
                "blockgraph_region_rejected_missing_follow_count": 1,
                "blockgraph_region_rejected_must_emit_label_count": 2,
                "blockgraph_region_rejected_middle_ref_count": 1,
                "blockgraph_region_rejected_external_ref_count": 1,
                "blockgraph_region_rejected_join_owner_conflict_count": 1,
                "blockgraph_region_rejected_nonterminal_join_count": 0,
                "blockgraph_region_rejected_follow_owner_conflict_count": 0,
                "blockgraph_region_rejected_emit_ready_count": 1,
            }
        )
        self.assertEqual(metrics["candidate"], 7.0)
        self.assertEqual(metrics["complete"], 3.0)
        self.assertEqual(metrics["rejected_missing_follow"], 1.0)
        self.assertEqual(metrics["rejected_must_emit_label"], 2.0)
        self.assertEqual(metrics["rejected_middle_ref"], 1.0)
        self.assertEqual(metrics["rejected_external_ref"], 1.0)
        self.assertEqual(metrics["rejected_join_owner_conflict"], 1.0)
        self.assertEqual(metrics["rejected_nonterminal_join"], 0.0)
        self.assertEqual(metrics["rejected_follow_owner_conflict"], 0.0)
        self.assertEqual(metrics["rejected_emit_ready"], 1.0)
        self.assertEqual(metrics["rejected_irreducible"], 0.0)

    def test_extract_alias_interleave_metrics(self) -> None:
        metrics = _extract_alias_interleave_metrics(
            {
                "guarded_tail_rejected_alias_interleave_conflict_count": 5,
                "canonicalization_failed_alias_has_nonlocal_ref_count": 4,
                "canonicalization_failed_alias_has_nonlocal_ref_external_before_count": 1,
                "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count": 2,
                "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count": 1,
                "canonicalization_failed_alias_not_fallthrough_count": 3,
                "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count": 2,
                "canonicalization_failed_alias_not_fallthrough_nested_after_label_count": 1,
                "canonicalization_failed_alias_has_multiple_internal_predecessors_count": 1,
                "canonicalization_failed_payload_crosses_join_count": 2,
            }
        )
        self.assertEqual(metrics["alias_interleave_conflict"], 5.0)
        self.assertEqual(metrics["alias_has_nonlocal_ref"], 4.0)
        self.assertEqual(metrics["alias_has_nonlocal_ref_external_before"], 1.0)
        self.assertEqual(metrics["alias_has_nonlocal_ref_nested_before"], 2.0)
        self.assertEqual(metrics["alias_has_nonlocal_ref_post_segment_ref"], 1.0)
        self.assertEqual(metrics["alias_not_fallthrough"], 3.0)
        self.assertEqual(metrics["alias_not_fallthrough_top_level_after_label"], 2.0)
        self.assertEqual(metrics["alias_not_fallthrough_nested_after_label"], 1.0)
        self.assertEqual(metrics["alias_has_multiple_internal_predecessors"], 1.0)
        self.assertEqual(metrics["payload_crosses_join"], 2.0)

    def test_shape_drift_metric_counts_synthetic_helper_calls(self) -> None:
        metrics = collect_code_metrics(
            "int f(){ return __fission_merge2(x, y) + __fission_guard(tmp); }",
            {},
        )
        self.assertEqual(metrics["synthetic_helper_call_count"], 2)

    def test_derive_binary_arch_prefers_tags(self) -> None:
        self.assertEqual(
            _derive_binary_arch({"tags": ["x86"], "binary_path": "/tmp/unknown.exe"}),
            "x86",
        )
        self.assertEqual(
            _derive_binary_arch({"tags": ["x64"], "binary_path": "/tmp/unknown.exe"}),
            "x64",
        )

    def test_derive_binary_arch_falls_back_to_windows_path(self) -> None:
        self.assertEqual(
            _derive_binary_arch({"tags": [], "binary_path": "/repo/samples/windows/x86/foo.exe"}),
            "x86",
        )
        self.assertEqual(
            _derive_binary_arch({"tags": [], "binary_path": "/repo/samples/windows/x64/foo.exe"}),
            "x64",
        )
        self.assertEqual(
            _derive_binary_arch(
                {"tags": [], "binary_path": "benchmark/binary/x86-64/window/small/binary/c/foo.exe"}
            ),
            "x64",
        )
        self.assertEqual(
            _derive_binary_arch(
                {
                    "tags": [],
                    "binary_path": "benchmark/binary/x86/vendor_binaries/x86/windows/foo.exe",
                }
            ),
            "x86",
        )

    def test_derive_dynamic_row_targets_prefers_degraded_rows(self) -> None:
        baseline = {
            "baseline_regression_gate": {
                "row_fidelity_gate": {
                    "rows": [
                        {"address": "0x140008900", "role": "secondary", "status": "degraded"},
                        {"address": "0x140008090", "role": "canary", "status": "degraded"},
                        {"address": "0x140001160", "role": "primary", "status": "unchanged"},
                    ]
                }
            },
            "pairwise": {
                "pyghidra_vs_fission": {
                    "comparisons": [
                        {
                            "address": "0x140001160",
                            "fission_success": True,
                            "pyghidra_success": True,
                            "normalized_similarity": 10.0,
                        }
                    ]
                }
            },
        }

        targets = _derive_dynamic_row_targets(baseline, limit=3)

        self.assertEqual(targets[0]["address"], "0x140008900")
        self.assertEqual(targets[0]["selected_because"], "baseline_degraded")
        self.assertEqual(targets[1]["address"], "0x140008090")
        self.assertEqual(targets[1]["selected_because"], "baseline_degraded")

    def test_derive_dynamic_row_targets_falls_back_to_low_similarity(self) -> None:
        baseline = {
            "pairwise": {
                "pyghidra_vs_fission": {
                    "comparisons": [
                        {
                            "address": "0x140008090",
                            "fission_success": True,
                            "pyghidra_success": True,
                            "normalized_similarity": 35.0,
                        },
                        {
                            "address": "0x140006c20",
                            "fission_success": True,
                            "pyghidra_success": True,
                            "normalized_similarity": 20.0,
                        },
                    ]
                }
            }
        }

        targets = _derive_dynamic_row_targets(baseline, limit=2)

        self.assertEqual(
            [(row["address"], row["selected_because"]) for row in targets],
            [
                ("0x140006c20", "baseline_low_similarity"),
                ("0x140008090", "baseline_low_similarity"),
            ],
        )

    def test_resolve_binary_watchlist_labels_mixed_sources(self) -> None:
        manifest_entry = {
            "canonical_quality_rows": [
                {
                    "address": "0x140001470",
                    "name": "fibonacci",
                    "role": "canonical_quality",
                }
            ],
            "row_fidelity_targets": [("0x140001160", "primary")],
        }
        baseline = {
            "baseline_regression_gate": {
                "row_fidelity_gate": {
                    "rows": [
                        {"address": "0x140008900", "role": "secondary", "status": "degraded"},
                    ]
                }
            }
        }

        watchlist = _resolve_binary_watchlist(
            manifest_entry=manifest_entry,
            baseline_summary_json=baseline,
            default_row_targets=[],
            dynamic_watchlist_limit=3,
        )

        self.assertEqual(watchlist["watchlist_source"], "mixed")
        self.assertEqual(
            watchlist["canonical_quality_rows"][0]["selected_because"],
            "canonical_quality",
        )
        self.assertEqual(
            watchlist["bootstrap_row_targets"][0]["selected_because"],
            "bootstrap_explicit",
        )
        self.assertEqual(
            watchlist["dynamic_watchlist_rows"][0]["selected_because"],
            "baseline_degraded",
        )
        self.assertEqual(
            watchlist["watchlist_diagnostics"]["selected_because_counts"],
            {"baseline_degraded": 1, "bootstrap_explicit": 1, "canonical_quality": 1},
        )
        self.assertEqual(
            watchlist["watchlist_diagnostics"]["canonical_quality_row_count"],
            1,
        )

    def test_build_corpus_assessment_emits_arch_owner_shape_and_watchlist_fields(self) -> None:
        benchmark = _minimal_single_binary_summary(owner_alias_unsafe=7, generic_local_name_sum=4)
        benchmark["summary"]["row_fidelity_targets"]["pyghidra_vs_fission"] = {
            "watchlist_source": "mixed",
            "bootstrap_row_targets": [
                {"address": "0x140001160", "role": "primary", "selected_because": "bootstrap_explicit"}
            ],
            "dynamic_watchlist_rows": [
                {
                    "address": "0x140008900",
                    "role": "dynamic_low_similarity",
                    "selected_because": "baseline_low_similarity",
                }
            ],
            "watchlist_diagnostics": {
                "watchlist_source": "mixed",
                "bootstrap_row_target_count": 1,
                "dynamic_watchlist_row_count": 1,
                "selected_because_counts": {
                    "bootstrap_explicit": 1,
                    "baseline_low_similarity": 1,
                },
            },
            "rows": [],
        }
        manifest = {
            "name": "suite",
            "path": "/tmp/suite.json",
            "suite_tier": "parity",
            "gate_mode": "advisory",
            "dynamic_watchlist_limit": 5,
            "notes": "",
            "entries": [
                {
                    "id": "sample",
                    "binary_path": "/repo/samples/windows/x64/sample.exe",
                    "ghidra_project_key": "sample",
                    "tags": ["x64", "stack-heavy"],
                    "seed_limit": 10,
                    "role": "release_candidate",
                    "weight": 1,
                    "row_fidelity_targets": [],
                    "dynamic_watchlist_limit": 5,
                }
            ],
        }

        corpus = build_corpus_assessment(
            manifest,
            [
                {
                    "manifest_entry": manifest["entries"][0],
                    "benchmark": benchmark,
                    "summary_json_path": Path("/tmp/sample/benchmark_summary.json"),
                    "summary_md_path": Path("/tmp/sample/benchmark_summary.md"),
                    "output_dir": Path("/tmp/sample"),
                    "baseline_summary": None,
                }
            ],
            baseline_summary_json=None,
            baseline_artifact=None,
        )

        self.assertEqual(corpus["binaries"][0]["arch"], "x64")
        self.assertEqual(corpus["owner_metric_totals"]["alias_unsafe"], 7)
        self.assertEqual(corpus["shape_drift_totals"]["generic_local_name_sum"], 4)
        self.assertEqual(
            corpus["normalize_pass_metric_totals"]["wide_dead_assignment_total_time_ms"],
            12,
        )
        self.assertEqual(
            corpus["giant_function_speed_family_totals"]["RenderHeavy"],
            1,
        )
        self.assertEqual(corpus["blockgraph_region_metric_totals"]["candidate"], 5)
        self.assertEqual(corpus["alias_interleave_metric_totals"]["alias_has_nonlocal_ref"], 4)
        self.assertEqual(corpus["mir_metric_totals"]["value"], 9)
        self.assertEqual(corpus["mir_metric_totals"]["blockgraph_admission_enabled"], 1)
        self.assertEqual(corpus["mir_metric_totals"]["blockgraph_irreducible_budget_bypass"], 1)
        self.assertEqual(corpus["binaries"][0]["mir_metrics"]["memory_region"], 2)
        self.assertEqual(
            corpus["binaries"][0]["blockgraph_region_metrics"]["complete"],
            2,
        )
        self.assertEqual(
            corpus["binaries"][0]["alias_interleave_metrics"]["alias_has_nonlocal_ref_nested_before"],
            2,
        )
        self.assertEqual(corpus["cpu_metric_totals"]["process_cpu_seconds"], 0.75)
        self.assertEqual(corpus["binaries"][0]["cpu_metrics"]["worker_count"], 4)
        self.assertEqual(
            corpus["max_pathological_examples"][0]["address"],
            "0x140002d40",
        )
        self.assertIn("x64", corpus["arch_summary"])
        self.assertEqual(corpus["watchlist_source_per_binary"]["sample"], "mixed")
        self.assertEqual(corpus["watchlist_reason_counts"]["bootstrap_explicit"], 1)
        self.assertEqual(corpus["watchlist_reason_counts"]["baseline_low_similarity"], 1)
        self.assertFalse(corpus["release_promotion_allowed"])
        self.assertIn("advisory_gate_mode", corpus["promotion_blockers"])

    def test_build_corpus_assessment_adds_owner_shape_and_arch_blockers(self) -> None:
        baseline_summary = {
            "mode": "corpus",
            "corpus_summary": {"weighted_avg_normalized_similarity": 50.0},
            "failure_family_distribution": {},
            "owner_metric_totals": {"alias_unsafe": 1},
            "shape_drift_totals": {"generic_local_name_sum": 1, "synthetic_helper_call_total": 0},
            "arch_summary": {
                "x64": {"weighted_avg_normalized_similarity": 50.0},
                "x86": {"weighted_avg_normalized_similarity": 0.0},
            },
        }
        trial_benchmark = _minimal_single_binary_summary(
            avg_similarity=49.0,
            owner_alias_unsafe=2,
            generic_local_name_sum=3,
        )
        manifest = {
            "name": "suite",
            "path": "/tmp/suite.json",
            "suite_tier": "parity",
            "gate_mode": "advisory",
            "dynamic_watchlist_limit": 5,
            "notes": "",
            "entries": [
                {
                    "id": "sample",
                    "binary_path": "/repo/samples/windows/x64/sample.exe",
                    "ghidra_project_key": "sample",
                    "tags": ["x64"],
                    "seed_limit": 10,
                    "role": "release_candidate",
                    "weight": 1,
                    "row_fidelity_targets": [],
                    "dynamic_watchlist_limit": 5,
                }
            ],
        }

        corpus = build_corpus_assessment(
            manifest,
            [
                {
                    "manifest_entry": manifest["entries"][0],
                    "benchmark": trial_benchmark,
                    "summary_json_path": Path("/tmp/sample/benchmark_summary.json"),
                    "summary_md_path": Path("/tmp/sample/benchmark_summary.md"),
                    "output_dir": Path("/tmp/sample"),
                    "baseline_summary": _minimal_single_binary_summary(avg_similarity=50.0),
                }
            ],
            baseline_summary_json=baseline_summary,
            baseline_artifact="/tmp/baseline",
        )

        blockers = "\n".join(corpus["promotion_blockers"])
        self.assertTrue(corpus["comparable_to_baseline"])
        self.assertIn("owner_metric_totals alias_unsafe", blockers)
        self.assertIn("shape_drift_totals generic_local_name_sum", blockers)
        self.assertIn("arch_summary.x64.weighted_avg_normalized_similarity", blockers)
        self.assertFalse(corpus["release_promotion_allowed"])

    def test_compare_with_previous_summary_includes_owner_and_shape_metrics(self) -> None:
        previous = _minimal_single_binary_summary(
            owner_alias_unsafe=1,
            generic_local_name_sum=1,
            heuristic_max_brace_nesting_mean=1.0,
            synthetic_helper_call_total=0,
        )
        current = _minimal_single_binary_summary(
            owner_alias_unsafe=2,
            generic_local_name_sum=2,
            heuristic_max_brace_nesting_mean=2.0,
            synthetic_helper_call_total=1,
        )

        comparison = compare_with_previous_summary(current, previous)
        metrics = {row["key"]: row for row in comparison["metrics"]}

        self.assertEqual(metrics["owner_alias_unsafe"]["direction"], "lower_is_better")
        self.assertEqual(metrics["owner_alias_unsafe"]["status"], "degraded")
        self.assertEqual(
            metrics["shape_generic_local_name_sum"]["direction"],
            "lower_is_better",
        )
        self.assertEqual(metrics["shape_generic_local_name_sum"]["status"], "degraded")
        self.assertEqual(
            metrics["shape_heuristic_max_brace_nesting_mean"]["status"],
            "degraded",
        )
        self.assertEqual(
            metrics["shape_synthetic_helper_call_total"]["status"],
            "degraded",
        )

    def test_build_corpus_compact_summary_keeps_capped_binary_rows(self) -> None:
        benchmark = _minimal_single_binary_summary(owner_alias_unsafe=7, generic_local_name_sum=4)
        manifest = {
            "name": "suite",
            "path": "/tmp/suite.json",
            "suite_tier": "parity",
            "gate_mode": "advisory",
            "dynamic_watchlist_limit": 5,
            "notes": "",
            "entries": [
                {
                    "id": "sample",
                    "binary_path": "/repo/samples/windows/x64/sample.exe",
                    "ghidra_project_key": "sample",
                    "tags": ["x64"],
                    "seed_limit": 10,
                    "role": "release_candidate",
                    "weight": 1,
                    "row_fidelity_targets": [],
                    "dynamic_watchlist_limit": 5,
                }
            ],
        }
        corpus = build_corpus_assessment(
            manifest,
            [
                {
                    "manifest_entry": manifest["entries"][0],
                    "benchmark": benchmark,
                    "summary_json_path": Path("/tmp/sample/benchmark_summary.json"),
                    "summary_md_path": Path("/tmp/sample/benchmark_summary.md"),
                    "output_dir": Path("/tmp/sample"),
                    "baseline_summary": None,
                }
            ],
            baseline_summary_json=None,
            baseline_artifact=None,
        )
        compact = build_corpus_compact_summary(corpus)
        payload = compact.model_dump(mode="json")
        self.assertEqual(payload["summary_kind"], "compact_corpus_benchmark")
        self.assertEqual(payload["owner_metric_totals"]["alias_unsafe"], 7.0)
        self.assertEqual(
            payload["normalize_pass_metric_totals"]["wide_dead_assignment_total_time_ms"],
            12.0,
        )
        self.assertEqual(payload["ghidra_action_metric_totals"]["stage_count"], 6.0)
        self.assertEqual(payload["mir_metric_totals"]["value"], 9.0)
        self.assertEqual(payload["mir_metric_totals"]["blockgraph_admission_enabled"], 1.0)
        self.assertEqual(
            payload["mir_metric_totals"]["blockgraph_irreducible_budget_bypass"],
            1.0,
        )
        self.assertEqual(payload["blockgraph_region_metric_totals"]["candidate"], 5.0)
        self.assertEqual(payload["alias_interleave_metric_totals"]["alias_has_nonlocal_ref"], 4.0)
        self.assertEqual(payload["cpu_metric_totals"]["process_cpu_seconds"], 0.75)
        self.assertEqual(
            payload["per_binary_rows"][0]["ghidra_action_metrics"]["blockgraph_structuring"],
            1.0,
        )
        self.assertEqual(payload["per_binary_rows"][0]["mir_metrics"]["block"], 1.0)
        self.assertEqual(
            payload["per_binary_rows"][0]["blockgraph_region_metrics"]["complete"],
            2.0,
        )
        self.assertEqual(
            payload["per_binary_rows"][0]["alias_interleave_metrics"]["alias_not_fallthrough"],
            3.0,
        )
        self.assertEqual(payload["per_binary_rows"][0]["cpu_metrics"]["worker_count"], 4.0)
        self.assertEqual(payload["unchanged_target_rows"][0]["name"], "fibonacci")
        self.assertFalse(payload["unchanged_target_rows"][0]["code_changed"])
        self.assertEqual(
            payload["giant_function_speed_family_totals"]["RenderHeavy"],
            1,
        )
        self.assertEqual(
            payload["max_pathological_examples"][0]["address"],
            "0x140002d40",
        )
        self.assertEqual(
            payload["per_binary_rows"][0]["normalize_pass_metrics"]["wide_dead_assignment_total_invocations"],
            3.0,
        )
        self.assertEqual(payload["per_binary_rows"][0]["id"], "sample")

    def test_render_corpus_markdown_and_console_smoke(self) -> None:
        benchmark = _minimal_single_binary_summary(owner_alias_unsafe=1, generic_local_name_sum=2)
        manifest = {
            "name": "suite",
            "path": "/tmp/suite.json",
            "suite_tier": "parity",
            "gate_mode": "advisory",
            "dynamic_watchlist_limit": 5,
            "notes": "",
            "entries": [
                {
                    "id": "sample",
                    "binary_path": "/repo/samples/windows/x64/sample.exe",
                    "ghidra_project_key": "sample",
                    "tags": ["x64"],
                    "seed_limit": 10,
                    "role": "release_candidate",
                    "weight": 1,
                    "row_fidelity_targets": [],
                    "dynamic_watchlist_limit": 5,
                }
            ],
        }
        corpus = build_corpus_assessment(
            manifest,
            [
                {
                    "manifest_entry": manifest["entries"][0],
                    "benchmark": benchmark,
                    "summary_json_path": Path("/tmp/sample/benchmark_summary.json"),
                    "summary_md_path": Path("/tmp/sample/benchmark_summary.md"),
                    "output_dir": Path("/tmp/sample"),
                    "baseline_summary": None,
                }
            ],
            baseline_summary_json=None,
            baseline_artifact=None,
        )
        markdown = render_corpus_benchmark_markdown(corpus)
        self.assertIn("## x86 / x64 Split", markdown)
        self.assertIn("## Normalize Pass Totals", markdown)
        self.assertIn("## Giant Function Families", markdown)
        self.assertIn("### Unchanged Target Rows", markdown)
        self.assertIn("benchmark_compact_summary.json", markdown)

        console = Console(record=True, width=140)
        print_corpus_benchmark_console(corpus, Path("/tmp/out"), console=console)
        rendered = console.export_text()
        self.assertIn("Corpus Benchmark", rendered)
        self.assertIn("Giant Function Families", rendered)


if __name__ == "__main__":
    unittest.main()
