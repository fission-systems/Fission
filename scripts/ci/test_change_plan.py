import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from change_plan import event_paths, plan


class ChangePlanTests(unittest.TestCase):
    def test_workflow_changes_run_lint_and_a_real_test_job(self):
        lane, jobs = plan([".github/workflows/ci.yml"], "pull_request")
        self.assertEqual(lane, "ci")
        self.assertTrue(jobs["run_actionlint"])
        self.assertTrue(jobs["run_tests"])
        self.assertTrue(jobs["run_cli_smoke"])

    def test_merge_candidate_selects_runtime_jobs(self):
        lane, jobs = plan(
            ["crates/fission-dynamic/src/lib.rs", "crates/fission-solver/src/solver.rs",
             "crates/fission-loader/src/loader/elf/mod.rs", "crates/fission-sleigh/src/lib.rs"],
            "merge_group",
        )
        self.assertEqual(lane, "rust")
        for job in ("run_platform_debugger", "run_solver_diff", "run_loader_fixtures", "run_decomp_corpus"):
            self.assertTrue(jobs[job], job)

    def test_premerge_definition_changes_run_every_premerge_lane(self):
        lanes = ("run_platform_debugger", "run_solver_diff", "run_loader_fixtures", "run_decomp_corpus")
        for path in (".github/workflows/reusable-premerge.yml", "scripts/ci/decompiler_smoke.py",
                     "scripts/macos/sign-debugger.sh"):
            _, jobs = plan([path], "pull_request")
            for job in lanes:
                self.assertTrue(jobs[job], (path, job))
        # After merge there is nothing left to gate.
        _, jobs = plan([".github/workflows/reusable-premerge.yml"], "push")
        for job in lanes:
            self.assertFalse(jobs[job], job)

    def test_sweep_changes_run_the_sweep_smoke_before_merge(self):
        for path in (".github/workflows/decbench-sweep.yml", "scripts/quality/decbench_full_sweep.py",
                     "scripts/quality/decbench-ci-requirements.txt"):
            self.assertTrue(plan([path], "pull_request")[1]["run_sweep_smoke"], path)
            self.assertTrue(plan([path], "merge_group")[1]["run_sweep_smoke"], path)
            self.assertFalse(plan([path], "push")[1]["run_sweep_smoke"], path)
        self.assertFalse(plan(["crates/fission-pcode/src/lib.rs"], "pull_request")[1]["run_sweep_smoke"])

    def test_advisories_gate_a_pr_only_when_it_changes_dependencies(self):
        for path in ("Cargo.lock", "Cargo.toml", "crates/fission-cli/Cargo.toml",
                     "deny.toml", ".cargo/audit.toml", ".cargo/config.toml"):
            for event in ("pull_request", "merge_group"):
                _, jobs = plan([path], event)
                self.assertTrue(jobs["run_security"], (path, event))
                self.assertTrue(jobs["run_advisories"], (path, event))
        # Ordinary code changes still run bans/licenses/sources, not advisories:
        # 113 of 137 failed PR runs in 30 days were one advisory published the
        # day 105 PRs were open.
        _, jobs = plan(["crates/fission-loader/src/lib.rs"], "pull_request")
        self.assertTrue(jobs["run_security"])
        self.assertFalse(jobs["run_advisories"])

    def test_every_push_to_main_compiles_the_workspace(self):
        # Two green PRs broke main's build; the next push changed only docs.
        self.assertTrue(plan(["docs/CLI.md"], "push")[1]["run_lint"])
        self.assertFalse(plan(["docs/CLI.md"], "pull_request")[1]["run_lint"])

    def test_docs_do_not_select_runtime_jobs(self):
        for path in ("docs/CLI.md", "mkdocs.yml"):
            lane, jobs = plan([path], "pull_request")
            self.assertEqual(lane, "docs")
            self.assertEqual({name for name, enabled in jobs.items() if enabled}, {"run_doc_checks"})

    def test_cli_contract_and_replay_gates_are_selected_for_their_owners(self):
        self.assertTrue(plan(["crates/fission-cli/src/cli/contract/schema.rs"], "merge_group")[1]["run_doc_checks"])
        self.assertTrue(plan(["crates/fission-quotient/src/report.rs"], "pull_request")[1]["run_doc_checks"])
        self.assertTrue(plan(["crates/fission-dir/src/eval.rs"], "pull_request")[1]["run_replay_checks"])
        self.assertFalse(plan(["docs/CLI.md"], "merge_group")[1]["run_replay_checks"])

    def test_stacked_pr_merge_group_and_rename_use_event_shas(self):
        with tempfile.TemporaryDirectory() as temp:
            prior = Path.cwd()
            try:
                os.chdir(temp)
                self.git("init", "-q")
                self.git("config", "user.name", "Test")
                self.git("config", "user.email", "test@example.invalid")
                Path("old.txt").touch()
                self.git("add", "old.txt")
                self.git("commit", "-qm", "stack parent")
                base = self.git("rev-parse", "HEAD")
                self.git("mv", "old.txt", "new.txt")
                self.git("commit", "-qm", "stack child")
                head = self.git("rev-parse", "HEAD")
                expected = {"old.txt", "new.txt"}
                self.assertEqual(
                    set(event_paths("pull_request", {"pull_request": {
                        "base": {"sha": base}, "head": {"sha": head},
                    }}, head)), expected,
                )
                self.assertEqual(
                    set(event_paths("merge_group", {"merge_group": {
                        "base_sha": base, "head_sha": head,
                    }}, head)), expected,
                )
                with self.assertRaises(ValueError):
                    event_paths("merge_group", {"merge_group": {
                        "base_sha": base, "head_sha": base,
                    }}, head)
            finally:
                os.chdir(prior)

    def test_a_retargeted_pr_with_a_stale_merge_ref_is_still_classified(self):
        # GitHub fires `edited` for a base change before it recomputes the
        # merge ref, so the checkout is the merge with the *old* base and the
        # new base is not among its ancestors.
        with tempfile.TemporaryDirectory() as temp:
            prior = Path.cwd()
            try:
                os.chdir(temp)
                self.git("init", "-q")
                self.git("config", "user.name", "Test")
                self.git("config", "user.email", "test@example.invalid")
                Path("root.txt").touch()
                self.git("add", "root.txt")
                self.git("commit", "-qm", "root")
                root = self.git("rev-parse", "HEAD")
                self.git("checkout", "-qb", "old-base")
                Path("old.txt").touch()
                self.git("add", "old.txt")
                self.git("commit", "-qm", "old base")
                self.git("checkout", "-qb", "feature")
                Path("feature.txt").touch()
                self.git("add", "feature.txt")
                self.git("commit", "-qm", "feature")
                head = self.git("rev-parse", "HEAD")
                stale_merge = head  # the merge ref still built on old-base
                self.git("checkout", "-q", root)
                Path("new.txt").touch()
                self.git("add", "new.txt")
                self.git("commit", "-qm", "new base (main moved)")
                new_base = self.git("rev-parse", "HEAD")
                self.git("checkout", "-q", stale_merge)
                payload = {"pull_request": {"base": {"sha": new_base}, "head": {"sha": head}}}
                with contextlib.redirect_stderr(io.StringIO()) as err:
                    paths = event_paths("pull_request", payload, stale_merge)
                self.assertIn("feature.txt", paths)
                self.assertIn("Stale merge ref", err.getvalue())
            finally:
                os.chdir(prior)

    @staticmethod
    def git(*args):
        return subprocess.check_output(["git", *args], text=True).strip()


if __name__ == "__main__":
    unittest.main()
