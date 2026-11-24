"""scripts/report/dev_metrics.py against a canned collection.

The fixture (scripts/report/testdata/dev_metrics_fixture.json) holds one PR
per triage state plus a precedence pair for each rule, merged PRs with and
without a merge-queue event, and CI runs with known job timestamps.
"""

import json
import sys
import unittest
from pathlib import Path

REPORT = Path(__file__).resolve().parents[1] / "report"
sys.path.insert(0, str(REPORT))

import dev_metrics  # noqa: E402

FIXTURE = REPORT / "testdata" / "dev_metrics_fixture.json"


def load():
    return json.loads(FIXTURE.read_text())


def numbers(items):
    return [i["number"] for i in items]


class TriageTests(unittest.TestCase):
    def setUp(self):
        self.m = dev_metrics.analyze(load())

    def test_every_open_non_draft_pr_has_exactly_one_state(self):
        states = {state: numbers(items) for state, items in self.m["triage"].items()}
        self.assertEqual(states, {
            "queued": [10],
            "waiting_ci": [11],
            "changes_requested": [12, 13, 18],
            "waiting_dependency": [14],
            "ready_to_enqueue": [15, 19],
            "waiting_review": [16],
        })
        everyone = sorted(n for ns in states.values() for n in ns)
        self.assertEqual(everyone, [10, 11, 12, 13, 14, 15, 16, 18, 19], "draft #17 is left out")

    def test_precedence(self):
        raw = load()
        prs = {p["number"]: p for p in raw["open"]}
        deps = raw["dependency_states"]
        # #10 has a failing check but auto-merge is on: queued wins.
        self.assertEqual(dev_metrics.classify(prs[10], deps)[0], "queued")
        prs[10]["autoMergeRequest"] = None
        self.assertEqual(dev_metrics.classify(prs[10], deps)[0], "changes_requested")
        # #11 is conflicting while checks run: waiting for CI wins.
        self.assertEqual(dev_metrics.classify(prs[11], deps)[0], "waiting_ci")
        # #12 is stacked and has changes requested: the author acts first.
        self.assertEqual(dev_metrics.classify(prs[12], deps)[0], "changes_requested")
        prs[12]["reviewDecision"] = None
        self.assertEqual(dev_metrics.classify(prs[12], deps)[0], "waiting_dependency")
        # Positive control for the dependency rule: #14 waits on open #99,
        # and is ready once #99 is closed.
        self.assertEqual(dev_metrics.classify(prs[14], deps)[0], "waiting_dependency")
        self.assertEqual(dev_metrics.classify(prs[14], {"99": "MERGED"})[0], "ready_to_enqueue")
        # One unresolved thread blocks enqueue; resolving it makes #13 ready.
        prs[13]["reviewThreads"]["nodes"][1]["isResolved"] = True
        self.assertEqual(dev_metrics.classify(prs[13], deps)[0], "ready_to_enqueue")
        # Review required is not ready.
        self.assertEqual(dev_metrics.classify(prs[16], deps)[0], "waiting_review")

    def test_action_required_lists_only_actionable_items(self):
        a = self.m["action"]
        self.assertEqual(numbers(a["ready_to_enqueue"]), [15, 19])
        self.assertEqual(numbers(a["out_of_date"]), [11, 18, 19])
        self.assertEqual([i["behind"] for i in a["out_of_date"]], [3, 0, 108])
        self.assertEqual(numbers(a["changes_requested"]), [12, 13, 18])
        self.assertEqual(a["main"], "failure")
        self.assertEqual(numbers(a["stale"]), [16])
        text = dev_metrics.render(self.m, "example/repo", {"action"})
        self.assertIn("main is not green", text)
        self.assertNotIn("awaiting", text.lower().split("## who acts next")[0])

    def test_nothing_to_do_says_so(self):
        raw = load()
        raw["open"] = []
        raw["runs"] = [r for r in raw["runs"] if r["id"] != 200]
        raw["main_runs"] = []
        text = dev_metrics.render(dev_metrics.analyze(raw), "example/repo", {"action"})
        self.assertIn("## Action required\n\nNothing.", text)


class FlowTests(unittest.TestCase):
    def setUp(self):
        self.m = dev_metrics.analyze(load())

    def test_lead_time_split_excludes_prs_that_skipped_the_queue(self):
        lt = self.m["lead_time_h"]
        self.assertEqual(lt["push_to_checks"]["n"], 2)
        self.assertEqual(self.m["merged_without_queue"], [2])
        # #1: push 10:00, checks 10:30, queued 11:00, merged 11:40.
        # #3: push 06:00, checks 07:00, re-queued 09:00, merged 10:00.
        self.assertEqual(lt["push_to_checks"]["p50"], 0.5)
        self.assertEqual(lt["push_to_checks"]["p95"], 1.0)
        self.assertEqual(lt["checks_to_queue"]["p95"], 2.0)
        self.assertAlmostEqual(lt["queue_to_merged"]["p50"], 40 / 60)
        # Positive control: give #2 a queue event and it is counted.
        raw = load()
        two = next(p for p in raw["merged"] if p["number"] == 2)
        two["timelineItems"]["nodes"].append(
            {"__typename": "AddedToMergeQueueEvent", "createdAt": "2026-11-24T11:10:00Z", "actor": {"login": "x"}})
        again = dev_metrics.analyze(raw)
        self.assertEqual(again["lead_time_h"]["push_to_checks"]["n"], 3)
        self.assertEqual(again["merged_without_queue"], [])

    def test_queue_ejections_exclude_merges(self):
        self.assertEqual(self.m["queue"]["ejections"], 1)
        self.assertEqual(self.m["queue"]["by_reason"], {"failed_checks": 1})
        self.assertEqual(self.m["queue"]["requeues_after_ejection"], 1)

    def test_ci_failure_rate_uses_the_required_summary_in_the_window(self):
        self.assertEqual(self.m["ci"]["pull_request"]["runs"], 2)
        self.assertEqual(self.m["ci"]["pull_request"]["failed"], 1)
        self.assertEqual(self.m["ci"]["merge_group"]["runs"], 1, "the cancelled group never ran CI Summary")

    def test_intervention_proxy(self):
        i = self.m["intervention"]
        self.assertEqual(i["merge_main_commits"], 1, "a merged sub-PR is not a catch-up merge")
        self.assertEqual(i["force_pushes"], 1)
        self.assertEqual(i["workflow_reruns"], 1, "the re-run outside the window is not counted")
        self.assertEqual(i["queue_adds_by_actor"], {"agent": 1, "maintainer": 2})


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.r = dev_metrics.analyze(load())["runners"]

    def test_queue_and_run_time_by_event(self):
        pr = self.r["by_event"]["pull_request"]
        # Waits 2, 10, 0 (run 100) and 1, 0 (run 103) minutes.
        self.assertEqual(pr["jobs"], 5)
        self.assertEqual(pr["queue_min"], {"p50": 1.0, "p95": 10.0, "max": 10.0})
        # 5 jobs over the 7-day window.
        self.assertAlmostEqual(pr["jobs_per_hour"], 5 / 168)
        group = self.r["by_event"]["merge_group"]
        self.assertEqual(group["queue_min"]["p95"], 20.0, "the 20-minute wait for a runner")
        self.assertEqual(group["jobs"], 5, "the skipped job is not a runner job")
        # Run 101 was created 11:00 and its first job started 11:20; run 99
        # started at once. The run-level wait sees the 20 minutes too.
        self.assertEqual(group["run_start_min"], {"p50": 0.0, "p95": 20.0, "max": 20.0})
        # Positive control for the censored count: a run still queued.
        raw = load()
        raw["runs"].append({"id": 105, "event": "merge_group", "status": "queued", "conclusion": None,
                            "created_at": "2026-11-24T11:20:00Z", "run_attempt": 1, "head_sha": "g5",
                            "head_branch": "gh-readonly-queue/main/pr-9-c", "pull_requests": [], "jobs": []})
        self.assertEqual(group["queued_now"], 0)
        waiting = dev_metrics.analyze(raw)["runners"]["by_event"]["merge_group"]
        self.assertEqual((waiting["queued_now"], waiting["oldest_queued_min"]), (1, 40.0))

    def test_peak_concurrency_by_label(self):
        def job(start, end, label="ubuntu-latest"):
            return {"name": "j", "status": "completed", "conclusion": "success", "created_at": start,
                    "started_at": start, "completed_at": end, "labels": [label], "runner_name": "r"}
        h = "2026-11-24T10:"
        raw = {"runs": [{"jobs": [
            job(h + "00:00Z", h + "30:00Z"),
            job(h + "10:00Z", h + "20:00Z"),
            job(h + "15:00Z", h + "16:00Z"),             # three at once at 10:15
            job(h + "20:00Z", h + "40:00Z"),             # starts as one ends: still three
            job(h + "15:10Z", h + "15:13Z"),             # 3 s inside the peak: a skip, left out
            job(h + "05:00Z", h + "50:00Z", "macos-latest"),
            {"name": "never started", "started_at": None, "completed_at": None, "labels": ["ubuntu-latest"]},
        ]}]}
        since, now = dev_metrics.ts("2026-11-24T00:00:00Z"), dev_metrics.ts("2026-11-24T12:00:00Z")
        c = dev_metrics.concurrency(raw, since, now)
        self.assertEqual(c["ubuntu-latest"]["per_hour"], {"2026-11-24 10": 3})
        self.assertEqual(c["macos-latest"]["peak"], 1)
        self.assertEqual(c["all"]["peak"], 4)
        # A job running across hours counts in each hour it runs through,
        # and an hour in which a job only ends is not inflated.
        across = {"runs": [{"jobs": [job("2026-11-24T05:50:00Z", "2026-11-24T08:00:00Z")]}]}
        self.assertEqual(dev_metrics.concurrency(across, since, now)["all"]["per_hour"],
                         {"2026-11-24 05": 1, "2026-11-24 06": 1, "2026-11-24 07": 1})
        # Positive control for the 5-second rule: the same job at 5 s overlaps
        # the three others and counts (peak 3 -> 4); at 3 s above it did not.
        raw["runs"][0]["jobs"][4]["completed_at"] = h + "15:15Z"
        self.assertEqual(dev_metrics.concurrency(raw, since, now)["ubuntu-latest"]["peak"], 4)

    def test_runner_minutes_are_attributed_to_the_pr(self):
        per_pr = {p["number"]: p for p in self.r["per_pr"]}
        # #1: PR run 3+10+1, merge group 2+10+5+1, cancelled group 1.
        self.assertEqual(per_pr[1]["runner_min"], 33.0)
        self.assertEqual(per_pr[1]["merge_group_runs"], 2)
        self.assertEqual(per_pr[3]["runner_min"], 14.0)
        # Positive control: #2 had no runs, so nothing is attributed to it.
        self.assertEqual(per_pr[2]["runner_min"], 0)
        self.assertEqual(self.r["merge_group_regenerations"], 1)
        self.assertEqual(self.r["merge_group_cancelled"], 1)

    def test_branch_fallback_only_inside_the_prs_lifetime(self):
        # Run 103 lists no PR (the API drops it once the branch is gone); it
        # is attributed to #3 by head branch. A run on the same branch from
        # before #3 was opened belongs to some earlier PR, not to #3.
        raw = load()
        raw["runs"].append({
            "id": 104, "event": "pull_request", "status": "completed", "conclusion": "success",
            "created_at": "2026-11-24T04:00:00Z", "run_attempt": 1, "head_sha": "old3",
            "head_branch": "fix/three", "pull_requests": [],
            "jobs": [{"name": "Lint", "status": "completed", "conclusion": "success",
                      "created_at": "2026-11-24T04:00:00Z", "started_at": "2026-11-24T04:00:00Z",
                      "completed_at": "2026-11-24T04:07:00Z"}]})
        per_pr = {p["number"]: p for p in dev_metrics.analyze(raw)["runners"]["per_pr"]}
        self.assertEqual(per_pr[3]["runner_min"], 14.0)
        self.assertEqual(per_pr[3]["pr_runs"], 1)

    def test_duplicate_cost_counts_only_jobs_in_both_runs(self):
        self.assertEqual(self.r["duplicate_min_by_job"], {"Lint": 10.0, "Detect changes": 2.0, "CI Summary": 1.0})
        self.assertNotIn("Package rehearsal", self.r["duplicate_min_by_job"])


class WindowTests(unittest.TestCase):
    def test_an_explicit_window_bounds_every_run_metric(self):
        raw = load()
        raw["since"] = "2026-11-24T09:00:00+00:00"
        raw["now"] = "2026-11-24T11:00:00+00:00"
        m = dev_metrics.analyze(raw)
        self.assertAlmostEqual(m["window"]["days"], 2 / 24)
        # Runs 100 (10:00) and 99 (10:50) are inside; run 103 (06:00) and
        # run 101 (11:00: the window end is exclusive) are not.
        self.assertEqual(m["ci"]["pull_request"]["runs"], 1)
        self.assertEqual(m["runners"]["by_event"]["merge_group"]["jobs"], 1)
        self.assertEqual(m["runners"]["by_event"]["pull_request"]["jobs"], 3)


class CompareTests(unittest.TestCase):
    def test_deltas_against_a_baseline(self):
        m = dev_metrics.analyze(load())
        before = json.loads(json.dumps(m))
        before["open_prs"]["count"] = 30
        text = dev_metrics.compare(m, before)
        self.assertIn("| open PRs | 30 | 10 | -20.0 |", text)


if __name__ == "__main__":
    unittest.main()
