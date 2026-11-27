#!/usr/bin/env python3
"""Deterministic scheduler & reconciler tests for scripts/ci/mutants_cohort.py (Issue #1251)."""

from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import sys
import tempfile
import unittest

CI_DIR = Path(__file__).resolve().parent
REPO_ROOT = CI_DIR.parent.parent
if str(CI_DIR) not in sys.path:
    sys.path.insert(0, str(CI_DIR))

import mutants_cohort as mc


def _make_entry(pkg: str, file_path: str, line: int, name: str, replacement: str = "()") -> dict:
    return {
        "package": pkg,
        "file": file_path,
        "genre": "FnValue",
        "name": name,
        "replacement": replacement,
        "span": {
            "start": {"line": line, "column": 5},
            "end": {"line": line + 3, "column": 6},
        },
    }


def _t(year: int, month: int, day: int, hour: int = 3) -> datetime:
    return datetime(year, month, day, hour, 0, tzinfo=timezone.utc)


class DisjointDiffsAndCohortIdentityTests(unittest.TestCase):
    def test_two_week_disjoint_diffs_cannot_complete_each_other(self) -> None:
        # Week 1 diff touches alpha.rs (2 shards); week 2 diff touches beta.rs (1 shard).
        c_alpha = mc.build_cohort(
            base_sha="1111111",
            head_sha="2222222",
            diff_sha256="aaaa" * 16,
            mutant_entries=[
                _make_entry("fission-core", "crates/fission-core/src/alpha.rs", 10, "alpha_f1"),
                _make_entry("fission-core", "crates/fission-core/src/alpha.rs", 20, "alpha_f2"),
                _make_entry("fission-core", "crates/fission-core/src/alpha.rs", 30, "alpha_f3"),
            ],
            created_at=_t(2026, 12, 6),
            shard_count_override=2,
        )
        c_beta = mc.build_cohort(
            base_sha="2222222",
            head_sha="3333333",
            diff_sha256="bbbb" * 16,
            mutant_entries=[
                _make_entry("fission-loader", "crates/fission-loader/src/beta.rs", 5, "beta_f1"),
            ],
            created_at=_t(2026, 12, 13),
            shard_count_override=1,
        )

        # Register week 1 cohort with only 1 shard run; complete shard 0, leave shard 1 pending.
        state = mc.empty_state(_t(2026, 12, 6))
        state, assign_w1 = mc.plan_tick(
            state, now=_t(2026, 12, 6), run_id="run-1", new_cohort=c_alpha, shards_per_run=1
        )
        self.assertEqual(len(assign_w1), 1)
        self.assertEqual(assign_w1[0]["cohort_id"], c_alpha["cohort_id"])
        self.assertEqual(assign_w1[0]["shard"], 0)

        with tempfile.TemporaryDirectory() as tmp:
            res = Path(tmp) / "results"
            # Run 1 outputs shard 0 of alpha only.
            s0 = res / f"{c_alpha['cohort_id']}-0"
            s0.mkdir(parents=True)
            (s0 / "shard-result.json").write_text(
                json.dumps(
                    {
                        "cohort_id": c_alpha["cohort_id"],
                        "shard": 0,
                        "diff_sha256": c_alpha["diff_sha256"],
                        "job_status": "success",
                        "exit_code": 0,
                    }
                )
            )
            shard0_entries = [c_alpha["mutants"][0], c_alpha["mutants"][2]]
            outcomes_payload = {
                "outcomes": [
                    {"scenario": "Baseline", "summary": "Success"},
                    *[
                        {
                            "scenario": {
                                "Mutant": _make_entry(
                                    m["package"], m["file"], m["line"], m["name"], m["replacement"]
                                )
                            },
                            "summary": "CaughtMutant",
                        }
                        for m in shard0_entries
                    ],
                ]
            }
            (s0 / "outcomes.json").write_text(json.dumps(outcomes_payload))
            state = mc.reconcile_run(
                state, assignments=assign_w1, results_dir=res, run_id="run-1", now=_t(2026, 12, 6)
            )

        # Even if someone injects beta's mutant outcome into alpha's shard 1, it is rejected!
        beta_id = c_beta["mutants"][0]["mutant_id"]
        mc.reconcile_shard(
            state["cohorts"][0],
            1,
            "run-bad",
            job_status="success",
            exit_code=0,
            observed_outcomes={beta_id: {"status": "caught", "detail": "CaughtMutant"}},
            baseline_failed=False,
            now=_t(2026, 12, 13),
        )
        alpha_summary = mc.summarize_cohort(state["cohorts"][0], now=_t(2026, 12, 13))
        self.assertFalse(alpha_summary["completion_claim_allowed"])
        self.assertEqual(alpha_summary["status"], "in_progress")
        self.assertEqual(alpha_summary["caught"], 2)
        self.assertEqual(alpha_summary["not_run"], 1)


class RolloverReplayExpiryTests(unittest.TestCase):
    def test_iso_week_rollover_across_year_boundary_advances_deterministically(self) -> None:
        # Simulate Sunday runs across 2026-W52 -> 2026-W53/2027-W01 without ISO-week collision.
        entries = [
            _make_entry("fission-core", "crates/fission-core/src/lib.rs", 10 + i, f"fn_{i}")
            for i in range(6)
        ]
        cohort = mc.build_cohort(
            base_sha="aaa",
            head_sha="bbb",
            diff_sha256="ccc" * 10,
            mutant_entries=entries,
            created_at=_t(2026, 12, 27),
            shard_count_override=6,
        )
        state = mc.empty_state(_t(2026, 12, 27))

        run_dates = [_t(2026, 12, 27), _t(2027, 1, 3), _t(2027, 1, 10)]
        seen_shards: list[int] = []
        for idx, when in enumerate(run_dates):
            new_c = cohort if idx == 0 else None
            state, assigns = mc.plan_tick(
                state, now=when, run_id=f"run-{idx}", new_cohort=new_c, shards_per_run=2
            )
            self.assertEqual(len(assigns), 2)
            for a in assigns:
                seen_shards.append(a["shard"])
                mc.reconcile_shard(
                    state["cohorts"][0],
                    a["shard"],
                    f"run-{idx}",
                    job_status="success",
                    exit_code=0,
                    observed_outcomes={
                        m["mutant_id"]: {"status": "caught", "detail": "CaughtMutant"}
                        for m in state["cohorts"][0]["mutants"]
                        if m["shard"] == a["shard"]
                    },
                    baseline_failed=False,
                    now=when,
                )

        self.assertEqual(seen_shards, [0, 1, 2, 3, 4, 5])
        summary = mc.summarize_cohort(state["cohorts"][0], now=_t(2027, 1, 10))
        self.assertTrue(summary["completion_claim_allowed"])
        self.assertEqual(summary["not_run"], 0)

    def test_duplicate_mutants_and_replay_registration_deduplicate(self) -> None:
        e1 = _make_entry("fission-core", "crates/fission-core/src/a.rs", 12, "dup_fn")
        e2 = _make_entry("fission-core", "crates/fission-core/src/a.rs", 12, "dup_fn")
        cohort = mc.build_cohort(
            base_sha="a",
            head_sha="b",
            diff_sha256="c" * 64,
            mutant_entries=[e1, e2],
            created_at=_t(2026, 11, 1),
        )
        self.assertEqual(len(cohort["mutants"]), 1)

        state = mc.empty_state(_t(2026, 11, 1))
        state, a1 = mc.plan_tick(state, now=_t(2026, 11, 1), run_id="r1", new_cohort=cohort)
        self.assertEqual(len(state["cohorts"]), 1)
        self.assertEqual(len(a1), 1)
        # Re-submitting identical cohort_id must NOT duplicate cohort in backlog.
        state, a2 = mc.plan_tick(state, now=_t(2026, 11, 1, 4), run_id="r2", new_cohort=cohort)
        self.assertEqual(len(state["cohorts"]), 1)
        self.assertEqual(len(a2), 1)  # orphaned r1 recovered to failed_retryable and re-assigned

    def test_expiry_marks_unattempted_explicit_and_manifest_outlives_cohort(self) -> None:
        c = mc.build_cohort(
            base_sha="a",
            head_sha="b",
            diff_sha256="d" * 64,
            mutant_entries=[_make_entry("fission-core", "f.rs", 1, "m1")],
            created_at=_t(2026, 10, 1),
            max_cohort_age_days=42,
        )
        state = mc.empty_state(_t(2026, 10, 1))
        state, _ = mc.plan_tick(state, now=_t(2026, 10, 1), run_id="r1", new_cohort=c, shards_per_run=0)

        # At day 43 (> 42-day lifetime, < 90-day manifest retention): expired, still retained!
        day43 = _t(2026, 10, 1) + timedelta(days=43)
        state, assigns = mc.plan_tick(state, now=day43, run_id="r2")
        self.assertEqual(assigns, [])
        self.assertEqual(len(state["cohorts"]), 1)
        summary43 = mc.summarize_state(state, now=day43, run_id="r2")
        self.assertEqual(summary43["cohorts"][0]["status"], "expired")
        self.assertEqual(summary43["cohorts"][0]["not_run"], 1)
        self.assertFalse(summary43["cohorts"][0]["completion_claim_allowed"])

        # At day 91 (> 90-day retention): expired manifest pruned.
        day91 = _t(2026, 10, 1) + timedelta(days=91)
        state, _ = mc.plan_tick(state, now=day91, run_id="r3")
        self.assertEqual(len(state["cohorts"]), 0)


class RetryCancelOverlapAndOutcomeTests(unittest.TestCase):
    def test_canceled_job_recovery_retry_and_permanent_failure(self) -> None:
        c = mc.build_cohort(
            base_sha="1",
            head_sha="2",
            diff_sha256="e" * 64,
            mutant_entries=[_make_entry("fission-core", "f.rs", 1, "m1")],
            created_at=_t(2026, 11, 1),
        )
        state = mc.empty_state(_t(2026, 11, 1))
        # Run 1 assigns shard 0 -> gets canceled cleanly in reconcile
        state, assigns1 = mc.plan_tick(
            state, now=_t(2026, 11, 1), run_id="r1", new_cohort=c, max_shard_retries=2
        )
        self.assertEqual(len(assigns1), 1)
        mc.reconcile_shard(
            state["cohorts"][0],
            0,
            "r1",
            job_status="cancelled",
            exit_code=None,
            observed_outcomes={},
            baseline_failed=False,
            now=_t(2026, 11, 1, 4),
            max_shard_retries=2,
        )
        self.assertEqual(state["cohorts"][0]["shards"]["0"]["status"], "failed_retryable")

        # Run 2 retries attempt 2, crashes without reconcile; Run 3 recovers and marks failed_permanent.
        state, assigns2 = mc.plan_tick(
            state, now=_t(2026, 11, 8), run_id="r2", max_shard_retries=2
        )
        self.assertEqual(len(assigns2), 1)
        self.assertEqual(assigns2[0]["attempt"], 2)

        state, assigns3 = mc.plan_tick(
            state, now=_t(2026, 11, 15), run_id="r3", max_shard_retries=2
        )
        self.assertEqual(assigns3, [])
        self.assertEqual(state["cohorts"][0]["shards"]["0"]["status"], "failed_permanent")
        self.assertEqual(state["cohorts"][0]["status"], "incomplete_failed")
        summary = mc.summarize_cohort(state["cohorts"][0], now=_t(2026, 11, 15))
        self.assertEqual(summary["infra_error"], 1)
        self.assertFalse(summary["completion_claim_allowed"])

    def test_bounded_backlog_evicts_oldest_cohort_and_tracks_age(self) -> None:
        state = mc.empty_state(_t(2026, 11, 1))
        for i in range(4):
            when = _t(2026, 11, 1) + timedelta(days=7 * i)
            cohort = mc.build_cohort(
                base_sha=f"base_{i}",
                head_sha=f"head_{i}",
                diff_sha256=f"{i}" * 64,
                mutant_entries=[
                    _make_entry("fission-core", f"f_{i}.rs", 1, f"fn_{i}_a"),
                    _make_entry("fission-core", f"f_{i}.rs", 10, f"fn_{i}_b"),
                ],
                created_at=when,
                shard_count_override=2,
            )
            state, _ = mc.plan_tick(
                state,
                now=when,
                run_id=f"r-{i}",
                new_cohort=cohort,
                shards_per_run=1,
                max_active_cohorts=3,
            )

        summary = mc.summarize_state(state, now=_t(2026, 11, 22), run_id="r-3")
        self.assertEqual(summary["backlog_summary"]["active_cohorts"], 3)
        self.assertEqual(summary["backlog_summary"]["evicted_cohorts"], 1)
        self.assertEqual(summary["cohorts"][0]["status"], "evicted")
        self.assertFalse(summary["cohorts"][0]["completion_claim_allowed"])
        self.assertGreaterEqual(summary["backlog_summary"]["oldest_active_age_days"], 14.0)

    def test_timeouts_and_unviable_never_count_as_caught_and_baseline_failure_blocks_completion(
        self,
    ) -> None:
        entries = [
            _make_entry("fission-core", "f.rs", 1, "f1"),
            _make_entry("fission-core", "f.rs", 10, "f2"),
            _make_entry("fission-core", "f.rs", 20, "f3"),
            _make_entry("fission-core", "f.rs", 30, "f4"),
        ]
        cohort = mc.build_cohort(
            base_sha="a",
            head_sha="b",
            diff_sha256="f" * 64,
            mutant_entries=entries,
            created_at=_t(2026, 11, 1),
            shard_count_override=1,
        )
        mids = [m["mutant_id"] for m in cohort["mutants"]]
        cohort["shards"]["0"]["attempts"] = 1

        mc.reconcile_shard(
            cohort,
            0,
            "r1",
            job_status="success",
            exit_code=3,
            observed_outcomes={
                mids[0]: {"status": "caught", "detail": "CaughtMutant"},
                mids[1]: {"status": "missed", "detail": "MissedMutant"},
                mids[2]: {"status": "timeout", "detail": "Timeout"},
                mids[3]: {"status": "unviable", "detail": "Unviable"},
            },
            baseline_failed=False,
            now=_t(2026, 11, 1),
        )
        s = mc.summarize_cohort(cohort, now=_t(2026, 11, 1))
        self.assertEqual(s["total_mutants"], 4)
        self.assertEqual(s["eligible_mutants"], 3)  # unviable excluded
        self.assertEqual(s["attempted_mutants"], 3)
        self.assertEqual(s["caught"], 1)
        self.assertEqual(s["missed"], 1)
        self.assertEqual(s["timeout"], 1)
        self.assertEqual(s["unviable"], 1)
        self.assertAlmostEqual(s["killed_ratio_of_eligible"], round(1 / 3, 4))
        self.assertTrue(s["completion_claim_allowed"])

        # Now check baseline failure prevents completion even if exit code looks benign.
        c2 = mc.build_cohort(
            base_sha="x",
            head_sha="y",
            diff_sha256="1" * 64,
            mutant_entries=[entries[0]],
            created_at=_t(2026, 11, 1),
            shard_count_override=1,
        )
        c2["shards"]["0"]["attempts"] = 1
        mc.reconcile_shard(
            c2,
            0,
            "r2",
            job_status="success",
            exit_code=0,
            observed_outcomes={c2["mutants"][0]["mutant_id"]: {"status": "caught", "detail": "ok"}},
            baseline_failed=True,
            now=_t(2026, 11, 1),
        )
        self.assertEqual(c2["shards"]["0"]["status"], "failed_retryable")
        self.assertNotEqual(c2["status"], "completed")


class CliAndWorkflowContractTests(unittest.TestCase):
    def test_cli_plan_and_reconcile_end_to_end(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            diff_file = root / "week.diff"
            diff_file.write_text("diff --git a/x.rs b/x.rs\n")
            mutants_json = root / "mutants.json"
            entry = _make_entry("fission-core", "x.rs", 42, "fn_x")
            mutants_json.write_text(json.dumps([entry]))

            state_1 = root / "state1.json"
            assigns_1 = root / "assigns1.json"
            gh_out = root / "gh_output.txt"

            rc = mc.main(
                [
                    "plan",
                    "--state-out",
                    str(state_1),
                    "--assignments-out",
                    str(assigns_1),
                    "--diff-file",
                    str(diff_file),
                    "--mutants-json",
                    str(mutants_json),
                    "--base-sha",
                    "base123",
                    "--head-sha",
                    "head456",
                    "--run-id",
                    "run-e2e-1",
                    "--now",
                    "2026-12-27T03:15:00Z",
                    "--github-output",
                    str(gh_out),
                ]
            )
            self.assertEqual(rc, 0)
            self.assertIn("has_assignments=true", gh_out.read_text())

            assigns = json.loads(assigns_1.read_text())
            self.assertEqual(len(assigns), 1)
            cid = assigns[0]["cohort_id"]

            results_dir = root / "results" / f"mutants-shard-{cid}-0"
            (results_dir / "mutants.out").mkdir(parents=True)
            (results_dir / "shard-result.json").write_text(
                json.dumps(
                    {
                        "cohort_id": cid,
                        "shard": 0,
                        "diff_sha256": assigns[0]["diff_sha256"],
                        "job_status": "success",
                        "exit_code": 2,
                    }
                )
            )
            (results_dir / "mutants.out" / "outcomes.json").write_text(
                json.dumps(
                    {
                        "outcomes": [
                            {"scenario": "Baseline", "summary": "Success"},
                            {"scenario": {"Mutant": entry}, "summary": "MissedMutant"},
                        ]
                    }
                )
            )

            state_2 = root / "state2.json"
            summary_json = root / "summary.json"
            summary_md = root / "summary.md"
            rc2 = mc.main(
                [
                    "reconcile",
                    "--state-in",
                    str(state_1),
                    "--assignments-in",
                    str(assigns_1),
                    "--results-dir",
                    str(root / "results"),
                    "--state-out",
                    str(state_2),
                    "--summary-out",
                    str(summary_json),
                    "--markdown-out",
                    str(summary_md),
                    "--run-id",
                    "run-e2e-1",
                    "--now",
                    "2026-12-27T04:00:00Z",
                ]
            )
            self.assertEqual(rc2, 0)
            summary = json.loads(summary_json.read_text())
            self.assertTrue(summary["report_only"])
            self.assertEqual(summary["cohorts"][0]["missed"], 1)
            self.assertEqual(summary["cohorts"][0]["not_run"], 0)
            self.assertTrue(summary["cohorts"][0]["completion_claim_allowed"])
            self.assertIn("Immutable Mutation Cohort Report", summary_md.read_text())

    def test_mutants_workflow_contract(self) -> None:
        wf = (REPO_ROOT / ".github" / "workflows" / "mutants.yml").read_text(encoding="utf-8")
        self.assertIn(f'CARGO_MUTANTS_VERSION: "{mc.CARGO_MUTANTS_VERSION}"', wf)
        self.assertIn(f'CARGO_NEXTEST_VERSION: "{mc.CARGO_NEXTEST_VERSION}"', wf)
        self.assertIn("scripts/ci/mutants_cohort.py plan", wf)
        self.assertIn("scripts/ci/mutants_cohort.py reconcile", wf)
        self.assertIn("--sharding round-robin", wf)
        self.assertIn("retention-days: 90", wf)
        self.assertIn("case \"$status\" in 0|2|3)", wf)
        self.assertNotIn("date -u +%V", wf)
        self.assertNotIn("every slice within 12 weeks", wf)


if __name__ == "__main__":
    unittest.main()
