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

    def test_docs_do_not_select_runtime_jobs(self):
        self.assertEqual(plan(["docs/CLI.md"], "pull_request"), ("docs", {
            name: False for name in plan(["docs/CLI.md"], "pull_request")[1]
        }))

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

    @staticmethod
    def git(*args):
        return subprocess.check_output(["git", *args], text=True).strip()


if __name__ == "__main__":
    unittest.main()
