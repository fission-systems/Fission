"""scripts/ci/sentry_triage.py against fake Sentry and GitHub APIs (no network)."""

import dataclasses
import io
import json
import random
import re
import sys
import tempfile
import unittest
import urllib.parse
from contextlib import redirect_stderr, redirect_stdout
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import sentry_triage as st

REPO = st.DEFAULT_REPO
MAIN_SHA = "a" * 40
OTHER_SHA = "b" * 40
NOW = datetime(2026, 11, 25, 12, 0, tzinfo=timezone.utc)
POISON = "POISON-7f3c"


def tags_for(**override):
    tags = {
        "environment": "nightly",
        "kind": "verify_diverged",
        "component": "fission-pcode",
        "arch": "x86_64",
        "stage": "normalize",
        "invariant": "ground_truth.return_value",
        "category": "confirmed_semantic_defect",
        "commit": MAIN_SHA,
        "repro_id": "fixture:tests/fixtures/x64_password.elf",
        "run_url": f"https://github.com/{REPO}/actions/runs/1001",
    }
    tags.update(override)
    return {k: v for k, v in tags.items() if v is not None}


class World:
    """Both APIs, routed by method and path; records every call."""

    def __init__(self):
        self.calls = []
        self.sentry_issues = []
        self.events = {}
        self.committers = {}
        self.gh_issues = []
        self.labels = {item["name"] for item in json.loads((st.ROOT / ".github/labels.json").read_text())}
        self.runs = {"1001": self.run(1001, MAIN_SHA, "schedule"), "1002": self.run(1002, MAIN_SHA, "push")}
        self.on_main = {MAIN_SHA}
        self.issue_pages = 1

    @staticmethod
    def run(run_id, sha, event, repo=REPO):
        return {"id": run_id, "head_sha": sha, "event": event, "repository": {"full_name": repo}}

    def add(self, sentry_id, tags=None, *, first_seen="2026-11-24T01:00:00Z", **event_extra):
        tags = tags_for() if tags is None else tags
        self.sentry_issues.append(
            {
                "id": str(sentry_id),
                "shortId": f"FISSION-{sentry_id}",
                "project": {"slug": "fission", "id": "42"},
                "count": "3",
                "userCount": 0,
                "firstSeen": first_seen,
                "lastSeen": "2026-11-25T06:56:09.353000Z",
                "permalink": f"https://evil.example/{POISON}",
                "title": f"{POISON} title",
                "culprit": f"{POISON} culprit",
                "metadata": {"value": f"{POISON} metadata"},
            }
        )
        event = {
            "eventID": f"{sentry_id:032x}",
            "groupID": str(sentry_id),
            "tags": [{"key": k, "value": v} for k, v in tags.items()]
            + [{"key": "os", "value": f"{POISON} os"}, {"key": "level", "value": "error"}],
            "message": f"{POISON} message",
            "title": f"{POISON} event title",
            "entries": [{"type": "exception", "data": {"values": [{"type": "x", "value": f"{POISON} exc"}]}}],
            "user": {"email": f"{POISON}@example.com"},
        }
        event.update(event_extra)
        self.events[str(sentry_id)] = event

    def gh_issue(self, number, sentry_id, state="open", created_at="2026-11-20T00:00:00Z", title="t"):
        self.gh_issues.append(
            {
                "number": number,
                "state": state,
                "created_at": created_at,
                "title": title,
                "body": f"x\n<!-- sentry-issue: {sentry_id} -->\n" if sentry_id is not None else "no marker",
                "user": {"login": st.ISSUE_AUTHOR},
            }
        )

    def writes(self):
        return [c for c in self.calls if c[0] != "GET"]

    def __call__(self, method, url, headers, body):
        self.calls.append((method, url))
        parsed = urllib.parse.urlsplit(url)
        path, query = parsed.path, urllib.parse.parse_qs(parsed.query)
        ok = lambda data, status=200, hdrs=None: (status, hdrs or {}, json.dumps(data).encode())
        if url.startswith(st.SENTRY_API):
            assert headers["Authorization"] == "Bearer sentry-token"
            assert method == "GET"
            path = path[len("/api/0"):]
            if path == "/projects/fission-systems/fission/":
                return ok({"id": "42", "slug": "fission"})
            if path == "/organizations/fission-systems/issues/":
                return ok(self.sentry_issues)
            match = re.fullmatch(r"/organizations/fission-systems/issues/(\d+)/events/latest/", path)
            if match:
                event = self.events.get(match.group(1))
                return ok(event) if event else ok({"detail": "not found"}, 404)
            match = re.fullmatch(r"/projects/fission-systems/fission/events/([0-9a-f]+)/committers/", path)
            if match:
                data = self.committers.get(match.group(1))
                return ok(data) if data else ok({"detail": "No committers found"}, 404)
            raise AssertionError(f"unrouted Sentry call {path}")
        assert url.startswith(st.GITHUB_API), url
        assert headers["Authorization"] == "Bearer gh-token"
        if method == "GET" and path == f"/repos/{REPO}/issues":
            assert query["state"] == ["all"] and query["creator"] == [st.ISSUE_AUTHOR]
            return ok([i for i in self.gh_issues if i["user"]["login"] == st.ISSUE_AUTHOR])
        if method == "POST" and path == f"/repos/{REPO}/issues":
            payload = json.loads(body)
            number = 2000 + len(self.gh_issues)
            self.gh_issues.append(
                {
                    "number": number,
                    "state": "open",
                    "created_at": NOW.strftime("%Y-%m-%dT%H:%M:%SZ"),
                    "user": {"login": st.ISSUE_AUTHOR},
                    **payload,
                }
            )
            return ok({"number": number}, 201)
        if method == "GET" and path == f"/repos/{REPO}/labels":
            return ok([{"name": n} for n in sorted(self.labels)])
        match = re.fullmatch(rf"/repos/{REPO}/actions/runs/(\d+)", path)
        if method == "GET" and match:
            run = self.runs.get(match.group(1))
            return ok(run) if run else ok({"message": "Not Found"}, 404)
        match = re.fullmatch(rf"/repos/{REPO}/compare/main\.\.\.([0-9a-f]+)", path)
        if method == "GET" and match:
            return ok({"status": "behind" if match.group(1) in self.on_main else "ahead"})
        raise AssertionError(f"unrouted GitHub call {method} {path}")


def clients(world, create=False):
    sentry = st.Sentry(st.Api(st.SENTRY_API, {"Authorization": "Bearer sentry-token"}, world, False), "fission-systems", "fission")
    github = st.GitHub(st.Api(st.GITHUB_API, {"Authorization": "Bearer gh-token"}, world, create), REPO)
    return sentry, github


def run_triage(world, create=False, run_cap=5, daily_cap=10, now=NOW):
    sentry, github = clients(world, create)
    return st.triage(sentry, github, create=create, now=now, run_cap=run_cap, daily_cap=daily_cap)


ENV = {"SENTRY_AUTH_TOKEN": "sentry-token", "GITHUB_TOKEN": "gh-token"}


def run_main(world, argv, env=ENV):
    with tempfile.TemporaryDirectory() as tmp:
        out, summary = Path(tmp) / "out.json", Path(tmp) / "summary.md"
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            code = st.main([*argv, "--out", str(out), "--summary", str(summary)], transport=world, env=env)
        return code, (json.loads(out.read_text()) if out.exists() else None), (
            summary.read_text() if summary.exists() else ""
        )


class ModeTests(unittest.TestCase):
    def test_dry_run_is_the_default(self):
        world = World()
        world.add(11)
        code, out, summary = run_main(world, [])
        self.assertEqual(code, 0)
        self.assertEqual(out["mode"], "dry-run")
        self.assertEqual([d["action"] for d in out["decisions"]], ["would_create"])
        self.assertEqual(world.writes(), [])
        self.assertIn("Mode: **dry run**", summary)
        self.assertIn("<!-- sentry-issue: 11 -->", summary)

    def test_create_flag_alone_files_nothing(self):
        world = World()
        world.add(11)
        for env in (ENV, {**ENV, "SENTRY_TRIAGE_ENABLED": "false"}, {**ENV, "SENTRY_TRIAGE_ENABLED": "True"}):
            code, out, _ = run_main(world, ["--create"], env)
            self.assertEqual(code, 2)
            self.assertIsNone(out)
        self.assertEqual(world.calls, [])

    def test_enabled_path_files_the_issue(self):
        world = World()
        world.add(11)
        code, out, _ = run_main(world, ["--create"], {**ENV, "SENTRY_TRIAGE_ENABLED": "true"})
        self.assertEqual(code, 0)
        self.assertEqual(out["decisions"][0]["action"], "created")
        self.assertEqual(world.writes(), [("POST", f"{st.GITHUB_API}/repos/{REPO}/issues")])
        filed = world.gh_issues[-1]
        self.assertTrue(filed["title"].startswith("[sentry FISSION-11] verify_diverged in fission-pcode/normalize"))
        self.assertIn("<!-- sentry-issue: 11 -->", filed["body"])
        self.assertEqual(filed["labels"], ["bug", "status:needs-repro", "priority:P1", "area:midend"])

    def test_read_only_api_refuses_writes(self):
        world = World()
        api = st.Api(st.GITHUB_API, {"Authorization": "Bearer gh-token"}, world, allow_write=False)
        with self.assertRaises(st.ApiError):
            api.call("POST", f"/repos/{REPO}/issues", body={})
        self.assertEqual(world.calls, [])

    def test_no_sentry_token_queries_nothing(self):
        world = World()
        code, _, summary = run_main(world, [], {"GITHUB_TOKEN": "gh-token"})
        self.assertEqual(code, 0)
        self.assertEqual(world.calls, [])
        self.assertIn("SENTRY_AUTH_TOKEN is not set", summary)

    def test_daily_cap_cannot_exceed_ten(self):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            st.main(["--daily-cap", "11"], transport=World(), env=ENV)

    def test_pagination_stays_on_the_api_host(self):
        api = st.Api(st.GITHUB_API, {}, World(), False)
        self.assertIsNone(api.next_page({"Link": '<https://evil.example/repos?page=2>; rel="next"'}))
        self.assertEqual(
            api.next_page({"link": f'<{st.GITHUB_API}/x?page=2>; rel="next"'}), f"{st.GITHUB_API}/x?page=2"
        )
        sentry_api = st.Api(st.SENTRY_API, {}, World(), False)
        self.assertIsNone(
            sentry_api.next_page({"Link": f'<{st.SENTRY_API}/x?cursor=1>; rel="next"; results="false"; cursor="1"'})
        )


class DedupTests(unittest.TestCase):
    def test_running_twice_files_once(self):
        world = World()
        world.add(11)
        world.add(12, tags_for(kind="fuzz_crash", category="crash", component="fission-loader"))
        first = run_triage(world, create=True)
        second = run_triage(world, create=True)
        self.assertEqual([d.action for d in first], ["created", "created"])
        self.assertEqual([d.action for d in second], ["tracked", "tracked"])
        self.assertEqual(len(world.writes()), 2)
        markers = [st.MARKER_RE.search(i["body"]).group(1) for i in world.gh_issues]
        self.assertEqual(sorted(markers), ["11", "12"])

    def test_closed_issue_is_left_closed(self):
        world = World()
        world.add(11)
        world.gh_issue(900, 11, state="closed")
        decisions = run_triage(world, create=True)
        self.assertEqual([(d.action, d.reason, d.github_issue) for d in decisions], [("tracked", "closed", 900)])
        self.assertEqual(world.writes(), [])
        # No event fetch either: a tracked issue costs no further calls.
        self.assertFalse(any("/events/" in url for _, url in world.calls))

    def test_title_tag_dedups_when_the_marker_was_edited_out(self):
        world = World()
        world.add(11)
        world.gh_issue(901, None, title="[sentry FISSION-11] verify_diverged in fission-pcode/normalize: x")
        self.assertEqual(run_triage(world, create=True)[0].action, "tracked")
        self.assertEqual(world.writes(), [])

    def test_issues_by_other_authors_are_not_counted(self):
        world = World()
        world.add(11)
        world.gh_issue(902, 11)
        world.gh_issues[-1]["user"]["login"] = "someone"
        self.assertEqual(run_triage(world)[0].action, "would_create")


class CapTests(unittest.TestCase):
    def setUp(self):
        self.world = World()
        for n in range(1, 6):
            self.world.add(100 + n, first_seen=f"2026-11-2{n}T00:00:00Z")

    def test_daily_cap_counts_todays_marked_issues(self):
        for n in range(8):
            self.world.gh_issue(500 + n, 900 + n, created_at="2026-11-25T01:00:00Z", state="closed" if n % 2 else "open")
        self.world.gh_issue(600, 990, created_at="2026-11-24T23:59:59Z")  # yesterday
        decisions = run_triage(self.world, create=True)
        self.assertEqual([d.action for d in decisions], ["created", "created", "capped", "capped", "capped"])
        self.assertEqual({d.reason for d in decisions if d.action == "capped"}, {"daily_cap"})
        self.assertEqual(len(self.world.writes()), 2)

    def test_daily_cap_holds_across_runs(self):
        for _ in range(4):
            run_triage(self.world, create=True, daily_cap=3)
        self.assertEqual(len(self.world.writes()), 3)

    def test_daily_cap_is_clamped_to_ten(self):
        for n in range(10):
            self.world.gh_issue(500 + n, 900 + n, created_at="2026-11-25T01:00:00Z")
        decisions = run_triage(self.world, create=True, daily_cap=50)
        self.assertEqual({d.action for d in decisions}, {"capped"})

    def test_run_cap(self):
        decisions = run_triage(self.world, run_cap=2)
        self.assertEqual([d.action for d in decisions], ["would_create"] * 2 + ["capped"] * 3)
        self.assertEqual(decisions[-1].reason, "run_cap")

    def test_dry_run_respects_the_caps_it_previews(self):
        for n in range(10):
            self.world.gh_issue(500 + n, 900 + n, created_at="2026-11-25T01:00:00Z")
        self.assertEqual({d.action for d in run_triage(self.world)}, {"capped"})


class RejectionTests(unittest.TestCase):
    CASES = {
        "ineligible_environment": [tags_for(environment="development"), tags_for(environment="production")],
        "ineligible_category": [tags_for(category="flaky"), tags_for(category="infra")],
        "ineligible_kind": [tags_for(kind="flaky_test", category="crash")],
        "missing_tag": [tags_for(**{key: None}) for key in [*st.TAG_CHECKS, "run_url"]],
        "invalid_tag": [
            tags_for(kind="Verify_Diverged"),
            tags_for(kind="segfault"),
            tags_for(component="pcode"),
            tags_for(component="fission-PCODE"),
            tags_for(component="fission-pcode`<script>"),
            tags_for(arch="amd64"),
            tags_for(stage="decompile"),
            tags_for(invariant="x" * 65),
            tags_for(invariant="see [here](https://evil.example)"),
            tags_for(invariant="@someone"),
            tags_for(category="Crash"),
            tags_for(commit=MAIN_SHA[:39]),
            tags_for(commit=MAIN_SHA.upper()),
            tags_for(repro_id="fixture:tests/fixtures/x64_password.elf@0x401000"),
            tags_for(repro_id="/home/user/corpus/secret binary"),
            tags_for(repro_id="r" * 129),
            tags_for(run_url="https://github.com/fission-systems/Fission/actions/runs/1001/job/2"),
            tags_for(run_url="http://github.com/fission-systems/Fission/actions/runs/1001"),
        ],
        "run_in_other_repo": [tags_for(run_url="https://github.com/someone/Fission/actions/runs/1001")],
        "run_not_found": [tags_for(run_url=f"https://github.com/{REPO}/actions/runs/999")],
        "run_commit_mismatch": [tags_for(commit=OTHER_SHA)],
        "run_event_mismatch": [
            tags_for(environment="nightly", run_url=f"https://github.com/{REPO}/actions/runs/1002"),
            tags_for(environment="ci", run_url=f"https://github.com/{REPO}/actions/runs/1001"),
        ],
    }

    def test_each_invalid_or_ineligible_event_is_ignored(self):
        for expected, cases in self.CASES.items():
            for tags in cases:
                with self.subTest(expected=expected, tags=tags):
                    world = World()
                    world.add(11, tags)
                    decisions = run_triage(world, create=True)
                    self.assertEqual(decisions[0].action, "rejected")
                    self.assertTrue(decisions[0].reason.startswith(expected), decisions[0].reason)
                    self.assertEqual(world.writes(), [])

    def test_commit_must_be_on_main(self):
        world = World()
        world.runs["1001"] = world.run(1001, OTHER_SHA, "schedule")
        world.add(11, tags_for(commit=OTHER_SHA))
        self.assertEqual(run_triage(world)[0].reason, "commit_not_on_main")

    def test_ci_environment_with_a_push_run_is_eligible(self):
        world = World()
        world.add(11, tags_for(environment="ci", run_url=f"https://github.com/{REPO}/actions/runs/1002"))
        self.assertEqual(run_triage(world)[0].action, "would_create")

    def test_malformed_events_and_issues(self):
        cases = [
            ("duplicate_tag", {"tags": [{"key": "kind", "value": "verify_diverged"}] * 2}),
            ("malformed_tag", {"tags": ["kind=verify_diverged"]}),
            ("no_tags", {"tags": None}),
            ("event_not_in_issue", {"groupID": "12"}),
            ("invalid_tag:kind", {"tags": [{"key": "kind", "value": ["verify_diverged"]}]}),
        ]
        for expected, extra in cases:
            with self.subTest(expected=expected):
                world = World()
                world.add(11)
                event = world.events["11"]
                if "tags" in extra and expected != "no_tags":
                    event["tags"] = extra["tags"] + event["tags"]
                else:
                    event.update(extra)
                decision = run_triage(world)[0]
                self.assertEqual(decision.action, "rejected")
                self.assertTrue(decision.reason.startswith(expected), decision.reason)
        issue_cases = [
            ("invalid_short_id", "shortId", "fission-11"),
            ("wrong_project", "project", {"slug": "freshbench"}),
            ("invalid_count", "count", "many"),
            ("invalid_timestamp", "firstSeen", "yesterday"),
            ("invalid_issue_id", "id", "11; rm"),
        ]
        for expected, key, value in issue_cases:
            with self.subTest(expected=expected):
                world = World()
                world.add(11)
                world.sentry_issues[0][key] = value
                decision = run_triage(world)[0]
                self.assertEqual((decision.action, decision.reason), ("rejected", expected))
                if key == "id":
                    self.assertEqual(decision.sentry_id, "invalid")


ALPHA = "abcdefghijklmnopqrstuvwxyz0123456789"


def random_tags(rng):
    env = rng.choice(["ci", "nightly"])
    word = lambda n, extra="": "".join(rng.choice(ALPHA + extra) for _ in range(rng.randint(1, n)))
    return {
        "environment": env,
        "kind": rng.choice(sorted(st.KINDS - st.INELIGIBLE_KINDS)),
        "component": rng.choice(["fission-" + word(20, "-"), "xtask"]),
        "arch": rng.choice(sorted(st.ARCHES)),
        "stage": rng.choice(sorted(st.STAGES)),
        "invariant": word(64, "_.:-"),
        "category": rng.choice(sorted(st.ELIGIBLE_CATEGORIES)),
        "commit": "".join(rng.choice("0123456789abcdef") for _ in range(40)),
        "repro_id": word(128, "_.:/-"),
        "run_url": f"https://github.com/{REPO}/actions/runs/{rng.randint(1, 10**12)}",
    }


class BodyPropertyTests(unittest.TestCase):
    """The body is exactly a fixed template with validated values substituted."""

    @staticmethod
    def template(suspect: bool, labels):
        fields = {f.name: f"\x00{f.name}\x00" for f in dataclasses.fields(st.Defect)}
        if not suspect:
            fields["suspect_commit"] = None
        return st.render_body(st.Defect(**fields), labels)

    def test_random_events_render_only_contract_values(self):
        rng = random.Random(20261125)
        for i in range(300):
            world = World()
            tags = random_tags(rng)
            run_id = tags["run_url"].rsplit("/", 1)[1]
            world.runs[run_id] = world.run(int(run_id), tags["commit"], "schedule" if tags["environment"] == "nightly" else rng.choice(["push", "merge_group", "workflow_dispatch"]))
            world.on_main.add(tags["commit"])
            sentry_id = rng.randint(1, 10**12)
            world.add(sentry_id, tags)
            suspect = rng.random() < 0.5
            if suspect:
                world.committers[f"{sentry_id:032x}"] = {
                    "committers": [{"author": {"email": f"{POISON}@x"}, "commits": [{"id": OTHER_SHA, "message": POISON}]}]
                }
            [decision] = run_triage(world)
            with self.subTest(i=i):
                self.assertEqual(decision.action, "would_create", decision.reason)
                text = decision.title + decision.body
                self.assertNotIn(POISON, text)
                self.assertNotIn("evil.example", text)
                values = {
                    "sentry_id": str(sentry_id),
                    "short_id": f"FISSION-{sentry_id}",
                    "permalink": f"https://fission-systems.sentry.io/issues/{sentry_id}/",
                    "events": "3",
                    "first_seen": "2026-11-24T01:00:00Z",
                    "last_seen": "2026-11-25T06:56:09Z",
                    **tags,
                    "run_id": run_id,
                    "suspect_commit": OTHER_SHA,
                }
                expected = self.template(suspect, list(decision.labels))
                for name, value in values.items():
                    expected = expected.replace(f"\x00{name}\x00", value)
                self.assertNotIn("\x00", expected)
                self.assertEqual(decision.body, expected)
                self.assertEqual(
                    decision.title,
                    f"[sentry FISSION-{sentry_id}] {tags['kind']} in {tags['component']}/{tags['stage']}: {tags['invariant']}",
                )

    def test_labels_follow_the_rule(self):
        rules = st.labeler_rules((st.ROOT / ".github/labeler.yml").read_text())
        self.assertEqual(st.area_labels("fission-pcode", rules), ["area:midend"])
        self.assertEqual(st.area_labels("fission-midend-normalize", rules), ["area:midend"])
        self.assertEqual(st.area_labels("fission-dir", rules), ["area:verify"])
        self.assertEqual(st.area_labels("fission-pcode-ir", rules), ["area:midend"])
        self.assertEqual(st.area_labels("xtask", rules), [])
        self.assertEqual(st.area_labels("fission-no-such-crate", rules), [])
        managed = st.managed_labels()
        self.assertTrue(set(st.FIXED_LABELS) <= managed)
        self.assertNotIn("status:confirmed", st.FIXED_LABELS)
        self.assertFalse(any(label.startswith("priority:P0") for label in st.FIXED_LABELS))

    def test_labels_missing_from_the_repo_are_dropped_not_created(self):
        world = World()
        world.labels -= {"priority:P1", "area:midend"}
        world.add(11)
        [decision] = run_triage(world)
        self.assertEqual(decision.labels, ("bug", "status:needs-repro"))
        self.assertFalse(any("/labels" in url and method != "GET" for method, url in world.calls))


if __name__ == "__main__":
    unittest.main()
