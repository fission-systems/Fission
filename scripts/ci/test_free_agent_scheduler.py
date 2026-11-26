import datetime
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import free_agent_scheduler as sched

NOW = datetime.datetime(2026, 11, 26, 12, 0, tzinfo=datetime.timezone.utc)
POLICY = {"max_daily_tasks": 10, "max_concurrent": 1, "max_pending_draft_prs": 10}


def issue(number, created="2026-11-20T00:00:00Z", labels=("agent:free",)):
    return {"number": number, "title": "t", "createdAt": created, "labels": [{"name": n} for n in labels]}


def pr(number, branch="x", body="", labels=()):
    return {"number": number, "headRefName": branch, "body": body, "labels": [{"name": n} for n in labels]}


def run(status="completed", created="2026-11-25T10:00:00Z"):
    return {"databaseId": 1, "status": status, "createdAt": created, "event": "workflow_dispatch"}


class PolicyFileTests(unittest.TestCase):
    def test_the_checked_in_policy_is_valid_and_has_the_agreed_limits(self):
        self.assertEqual(sched.load_policy(sched.POLICY_FILE),
                         {"max_daily_tasks": 10, "max_concurrent": 1, "max_pending_draft_prs": 10})

    def test_bad_policies_are_refused(self):
        for text in ('{"max_daily_tasks": 10}',
                     '{"max_daily_tasks": -1, "max_concurrent": 1, "max_pending_draft_prs": 1}',
                     '{"max_daily_tasks": 1, "max_concurrent": 0, "max_pending_draft_prs": 1}',
                     '{"max_daily_tasks": true, "max_concurrent": 1, "max_pending_draft_prs": 1}'):
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / "p.json"
                path.write_text(text)
                with self.assertRaises(SystemExit):
                    sched.load_policy(path)


class DuplicateGuardTests(unittest.TestCase):
    def test_a_labelled_pr_with_fixes_in_the_body_is_a_draft(self):
        found = sched.find_open_draft([pr(5, body="Fixes #12\n\ntext", labels=("agent:free-draft",))], 12)
        self.assertEqual(found["number"], 5)

    def test_fixes_12_is_not_fixes_123_and_the_label_is_required(self):
        self.assertIsNone(sched.find_open_draft([pr(5, body="Fixes #123", labels=("agent:free-draft",))], 12))
        self.assertIsNone(sched.find_open_draft([pr(5, body="Fixes #12")], 12))

    def test_the_branch_name_alone_is_enough_and_is_exact(self):
        self.assertEqual(sched.find_open_draft([pr(6, branch="agent-free/issue-12-999")], 12)["number"], 6)
        self.assertIsNone(sched.find_open_draft([pr(6, branch="agent-free/issue-123-999")], 12))
        self.assertIsNone(sched.find_open_draft([pr(6, branch="agent-fix/abc")], 12))

    def test_it_comments_once_per_pr(self):
        prs = [pr(5, branch="agent-free/issue-12-1")]
        with mock.patch.object(sched, "gh_open_prs", return_value=prs), \
                mock.patch.object(sched, "gh_comments", return_value=[]) as comments, \
                mock.patch.object(sched, "gh_comment") as comment, \
                tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            self.assertTrue(sched.guard("o/r", 12, str(out)))
            self.assertEqual(comment.call_count, 1)
            self.assertIn("exists=true", out.read_text())
            comments.return_value = [comment.call_args[0][2]]
            self.assertTrue(sched.guard("o/r", 12, None))
            self.assertEqual(comment.call_count, 1)  # no repeated comment

    def test_no_pr_means_no_comment_and_exists_false(self):
        with mock.patch.object(sched, "gh_open_prs", return_value=[]), \
                mock.patch.object(sched, "gh_comment") as comment, tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            self.assertFalse(sched.guard("o/r", 12, str(out)))
            self.assertIn("exists=false", out.read_text())
            comment.assert_not_called()


class DecideTests(unittest.TestCase):
    def decide(self, runs=(), prs=(), issues=(issue(1),), policy=POLICY):
        return sched.decide(policy, list(runs), NOW, list(prs), list(issues))

    def test_it_starts_the_oldest_eligible_issue(self):
        d = self.decide(issues=[issue(3, "2026-11-22T00:00:00Z"), issue(2, "2026-11-21T00:00:00Z"),
                                issue(4, "2026-11-23T00:00:00Z")])
        self.assertEqual((d.action, d.issue), ("start", 2))

    def test_blocked_deferred_and_already_drafted_issues_are_skipped_with_a_reason(self):
        issues = [issue(1, "2026-11-01T00:00:00Z", ("agent:free", "agent:blocked")),
                  issue(2, "2026-11-02T00:00:00Z", ("agent:free", "agent:deferred")),
                  issue(3, "2026-11-03T00:00:00Z"), issue(4, "2026-11-04T00:00:00Z")]
        d = self.decide(issues=issues, prs=[pr(9, branch="agent-free/issue-3-77")])
        self.assertEqual(d.issue, 4)
        self.assertEqual([n for n, _ in d.considered], [1, 2, 3])
        self.assertIn("open draft PR #9", d.considered[2][1])

    def test_concurrent_limit(self):
        for status in ("queued", "in_progress"):
            d = self.decide(runs=[run(status, "2026-11-25T10:00:00Z")])
            self.assertEqual(d.action, "skip")
            self.assertIn("max_concurrent", d.reason)
        self.assertEqual(self.decide(runs=[run("completed")]).action, "start")

    def test_daily_limit_counts_todays_utc_runs_only(self):
        today = [run("completed", "2026-11-26T01:00:00Z") for _ in range(10)]
        d = self.decide(runs=today)
        self.assertEqual(d.action, "skip")
        self.assertIn("max_daily_tasks", d.reason)
        self.assertEqual(d.facts["runs_today"], 10)
        yesterday = [run("completed", "2026-11-25T23:59:59Z") for _ in range(10)]
        self.assertEqual(self.decide(runs=yesterday).action, "start")

    def test_skipped_runs_for_other_labels_do_not_count(self):
        # Every issue label starts the workflow; runs for other labels end "skipped".
        skipped = [dict(run("completed", "2026-11-26T01:00:00Z"), conclusion="skipped") for _ in range(10)]
        d = self.decide(runs=skipped)
        self.assertEqual(d.facts["runs_today"], 0)
        self.assertEqual(d.action, "start")
        failed = [dict(run("completed", "2026-11-26T01:00:00Z"), conclusion="failure") for _ in range(10)]
        self.assertEqual(self.decide(runs=failed).action, "skip")

    def test_pending_draft_pr_limit(self):
        prs = [pr(n, labels=("agent:free-draft",)) for n in range(100, 110)]
        d = self.decide(prs=prs)
        self.assertEqual(d.action, "skip")
        self.assertIn("review is the bottleneck", d.reason)
        self.assertEqual(self.decide(prs=prs[:9]).action, "start")

    def test_no_eligible_issue(self):
        d = self.decide(issues=[])
        self.assertEqual((d.action, d.issue), ("skip", None))


class DeferredTests(unittest.TestCase):
    def labeled(self, when, name="agent:deferred"):
        return {"event": "labeled", "label": {"name": name}, "created_at": when}

    def test_it_expires_after_six_hours_from_the_latest_labeled_event(self):
        timeline = [self.labeled("2026-11-26T00:00:00Z"), self.labeled("2026-11-26T08:00:00Z"),
                    self.labeled("2026-11-20T00:00:00Z", "agent:free"), {"event": "commented"}]
        self.assertFalse(sched.deferred_expired(timeline, NOW)[0])  # 4 h
        self.assertTrue(sched.deferred_expired(timeline[:1] + timeline[2:], NOW)[0])  # 12 h
        self.assertTrue(sched.deferred_expired([self.labeled("2026-11-26T06:00:00Z")], NOW)[0])  # exactly 6 h

    def test_an_undated_label_is_removed_rather_than_stuck_for_good(self):
        self.assertTrue(sched.deferred_expired([], NOW)[0])

    def tick(self, dry_run, timeline):
        issues = [issue(1, labels=("agent:free", "agent:deferred"))]
        with mock.patch.object(sched, "gh_free_issues", return_value=issues), \
                mock.patch.object(sched, "gh_timeline", return_value=timeline), \
                mock.patch.object(sched, "gh_runs", return_value=[]), \
                mock.patch.object(sched, "gh_open_prs", return_value=[]), \
                mock.patch.object(sched, "usage_today", return_value="usage today: none"), \
                mock.patch.object(sched, "gh_remove_deferred") as remove, \
                mock.patch.object(sched, "gh_dispatch") as dispatch:
            decision = sched.tick("o/r", POLICY, dry_run, NOW)
        return decision, remove, dispatch

    def test_an_expired_deferral_is_removed_and_the_issue_starts_in_the_same_tick(self):
        decision, remove, dispatch = self.tick(False, [self.labeled("2026-11-25T00:00:00Z")])
        remove.assert_called_once_with("o/r", 1)
        dispatch.assert_called_once_with("o/r", 1)
        self.assertEqual(decision.issue, 1)

    def test_a_fresh_deferral_keeps_the_issue_out(self):
        decision, remove, dispatch = self.tick(False, [self.labeled("2026-11-26T11:00:00Z")])
        remove.assert_not_called()
        dispatch.assert_not_called()
        self.assertEqual(decision.action, "skip")

    def test_a_dry_run_changes_nothing_but_reports_what_it_would_do(self):
        decision, remove, dispatch = self.tick(True, [self.labeled("2026-11-25T00:00:00Z")])
        remove.assert_not_called()
        dispatch.assert_not_called()
        self.assertEqual(decision.action, "start")


class TickTests(unittest.TestCase):
    def test_one_task_per_tick(self):
        with mock.patch.object(sched, "gh_free_issues", return_value=[issue(1), issue(2)]), \
                mock.patch.object(sched, "gh_runs", return_value=[]), \
                mock.patch.object(sched, "gh_open_prs", return_value=[]), \
                mock.patch.object(sched, "usage_today", return_value=""), \
                mock.patch.object(sched, "gh_dispatch") as dispatch:
            sched.tick("o/r", POLICY, False, NOW)
        dispatch.assert_called_once_with("o/r", 1)

    def test_the_dispatch_command_names_the_issue(self):
        with mock.patch.object(sched, "gh") as gh:
            sched.gh_dispatch("o/r", 7)
        gh.assert_called_once_with("workflow", "run", "free-model-draft.yml", "--repo", "o/r",
                                   "-f", "issue=7", "-f", "open_pr=true")


if __name__ == "__main__":
    unittest.main()
