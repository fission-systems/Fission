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
    _derive_dynamic_row_targets,
    _resolve_binary_watchlist,
    build_corpus_assessment,
    load_corpus_manifest,
)


class CorpusBenchmarkTests(unittest.TestCase):
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

        self.assertEqual(
            targets[:2],
            [("0x140008900", "secondary"), ("0x140008090", "canary")],
        )

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
            targets,
            [
                ("0x140006c20", "dynamic_low_similarity"),
                ("0x140008090", "dynamic_low_similarity"),
            ],
        )

    def test_resolve_binary_watchlist_labels_mixed_sources(self) -> None:
        manifest_entry = {
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
        self.assertEqual(len(watchlist["bootstrap_row_targets"]), 1)
        self.assertEqual(len(watchlist["dynamic_watchlist_rows"]), 1)
        self.assertEqual(
            watchlist["rows"],
            [("0x140001160", "primary"), ("0x140008900", "secondary")],
        )

    def test_build_corpus_assessment_marks_advisory_promotion_blocked(self) -> None:
        benchmark = {
            "summary": {
                "quality": {"pyghidra_vs_fission": {"avg_normalized_similarity": 40.0}},
                "coverage": {"pyghidra_vs_fission": {"coverage_ratio_pct": 100.0, "shared_count": 10, "left_total_count": 10, "right_total_count": 10}},
                "engines": {
                    "pyghidra": {"seeded_function_count": 10, "function_count": 10},
                    "fission": {"seeded_function_count": 10, "function_count": 10, "direct_success_count": 10},
                },
                "row_fidelity_targets": {
                    "pyghidra_vs_fission": {
                        "watchlist_source": "dynamic",
                        "bootstrap_row_targets": [],
                        "dynamic_watchlist_rows": [{"address": "0x140008900", "role": "dynamic_low_similarity"}],
                        "rows": [],
                    }
                },
            }
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
                    "binary_path": "/tmp/sample.exe",
                    "ghidra_project_key": "sample",
                    "tags": ["flag-heavy"],
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

        self.assertEqual(corpus["suite_tier"], "parity")
        self.assertEqual(corpus["gate_mode"], "advisory")
        self.assertFalse(corpus["release_promotion_allowed"])
        self.assertIn("advisory_gate_mode", corpus["promotion_blockers"])


if __name__ == "__main__":
    unittest.main()
