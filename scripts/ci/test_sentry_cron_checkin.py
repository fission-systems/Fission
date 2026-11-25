import contextlib
import io
import json
import re
import unittest
import urllib.error
import uuid
from pathlib import Path

import sentry_cron_checkin as cron

DSN = "https://abc123@o4512491021991936.ingest.us.sentry.io/4512491021991936"
ENV = {
    "SENTRY_DSN": DSN,
    "GITHUB_SERVER_URL": "https://github.com",
    "GITHUB_REPOSITORY": "fission-systems/Fission",
    "GITHUB_RUN_ID": "42",
    "GITHUB_RUN_ATTEMPT": "1",
}
WORKFLOW = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "ci-heavy.yml"


class FakeResponse:
    status = 202

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class Recorder:
    def __init__(self, error=None):
        self.requests = []
        self.error = error

    def __call__(self, request, timeout):
        self.requests.append(request)
        if self.error is not None:
            raise self.error
        return FakeResponse()


def run(argv, env, opener):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = cron.main(argv, env=env, opener=opener)
    return code, out.getvalue()


class CheckinUrlTests(unittest.TestCase):
    def test_saas_dsn(self):
        self.assertEqual(
            cron.checkin_url(DSN),
            "https://o4512491021991936.ingest.us.sentry.io/api/4512491021991936"
            "/cron/ci-heavy-nightly/abc123/",
        )

    def test_path_prefix_and_port_are_kept(self):
        self.assertEqual(
            cron.checkin_url("http://key@sentry.local:9000/prefix/7"),
            "http://sentry.local:9000/prefix/api/7/cron/ci-heavy-nightly/key/",
        )

    def test_malformed_dsns_are_rejected(self):
        for dsn in ("not a url", "https://o1.ingest.sentry.io/1", "https://k@host/abc", "ftp://k@h/1"):
            with self.subTest(dsn=dsn), self.assertRaises(cron.DsnError):
                cron.checkin_url(dsn)


class PayloadTests(unittest.TestCase):
    def test_every_checkin_upserts_the_monitor_config(self):
        body = cron.payload("in_progress", "id")
        self.assertEqual(body["status"], "in_progress")
        self.assertEqual(body["monitor_config"], {
            "schedule": {"type": "crontab", "value": "30 2 * * *"},
            "checkin_margin": cron.CHECKIN_MARGIN_MINUTES,
            "max_runtime": cron.MAX_RUNTIME_MINUTES,
            "timezone": "UTC",
            "failure_issue_threshold": 1,
            "recovery_threshold": 1,
        })

    def test_unknown_status_is_refused(self):
        with self.assertRaises(ValueError):
            cron.payload("missed", "id")

    def test_result_mapping(self):
        self.assertEqual(cron.status_for_result("success"), "ok")
        for result in ("failure", "cancelled", "skipped", ""):
            self.assertEqual(cron.status_for_result(result), "error")

    def test_checkin_id_pairs_one_attempt_and_separates_reruns(self):
        first = cron.checkin_id("https://github.com", "o/r", "42", "1")
        self.assertEqual(first, cron.checkin_id("https://github.com", "o/r", "42", "1"))
        self.assertNotEqual(first, cron.checkin_id("https://github.com", "o/r", "42", "2"))
        self.assertEqual(uuid.UUID(first).version, 5)

    def test_schedule_matches_the_workflow(self):
        crons = re.findall(r'-\s*cron:\s*"([^"]+)"', WORKFLOW.read_text())
        self.assertEqual(crons, [cron.SCHEDULE])


class MainTests(unittest.TestCase):
    def test_start_and_finish_post_the_same_checkin(self):
        opener = Recorder()
        self.assertEqual(run(["--status", "in_progress"], ENV, opener)[0], 0)
        self.assertEqual(run(["--result", "failure"], ENV, opener)[0], 0)
        start, finish = (json.loads(r.data) for r in opener.requests)
        self.assertEqual(start["status"], "in_progress")
        self.assertEqual(finish["status"], "error")
        self.assertEqual(start["check_in_id"], finish["check_in_id"])
        request = opener.requests[0]
        self.assertEqual(request.get_method(), "POST")
        self.assertEqual(request.get_header("Content-type"), "application/json")
        self.assertEqual(request.full_url, cron.checkin_url(DSN))

    def test_missing_dsn_skips_without_a_request(self):
        opener = Recorder()
        for env in ({}, {**ENV, "SENTRY_DSN": "  "}):
            code, out = run(["--status", "ok"], env, opener)
            self.assertEqual(code, 0)
            self.assertIn("::notice", out)
        self.assertEqual(opener.requests, [])

    def test_malformed_dsn_warns_and_succeeds(self):
        opener = Recorder()
        code, out = run(["--status", "ok"], {**ENV, "SENTRY_DSN": "garbage"}, opener)
        self.assertEqual((code, opener.requests), (0, []))
        self.assertIn("::warning", out)

    def test_send_failures_never_fail_the_job_or_leak_the_key(self):
        errors = (
            urllib.error.HTTPError(cron.checkin_url(DSN), 400, "Bad Request", {}, None),
            urllib.error.URLError("timed out"),
            ConnectionResetError(104, "reset"),
            ValueError(f"unknown url type: {cron.checkin_url(DSN)}"),
        )
        for error in errors:
            with self.subTest(error=type(error).__name__):
                code, out = run(["--status", "ok"], ENV, Recorder(error))
                self.assertEqual(code, 0)
                self.assertIn("::warning", out)
                self.assertNotIn("abc123", out)

    def test_status_and_result_are_exclusive(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            cron.main(["--status", "ok", "--result", "success"], env=ENV, opener=Recorder())


if __name__ == "__main__":
    unittest.main()
