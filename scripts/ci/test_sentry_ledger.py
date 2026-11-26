import datetime as dt
import json
import tempfile
import unittest
from pathlib import Path

import sentry_ledger

COMMIT = "a" * 40
DSN = "https://" + "0" * 32 + "@o1.ingest.sentry.io/42"
TRUSTED = {"GITHUB_REPOSITORY": "fission-systems/Fission", "GITHUB_EVENT_NAME": "push",
           "GITHUB_REF": "refs/heads/main", "SENTRY_DSN": DSN}
NOW = dt.datetime(2026, 11, 26, tzinfo=dt.timezone.utc)


def entry(**over):
    e = {"commit": COMMIT, "cpu": "AMD EPYC 7763 64-Core Processor", "dirty": False,
         "summary": {"rows": 10, "rows_with_output": 9, "failed_calls": 0, "perfect": 4, "distance": 12.5,
                     "types_perfect": 3},
         "speed": {"instructions": 123456789, "binaries": {"stripped/gcc-O2/p/secret_binary": 1}},
         "outputs": {"x86_64/gcc-O2/p/secret_binary@10": "deadbeefdeadbeef"}}
    e.update(over)
    return e


class FakeResponse:
    status = 200

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class EventTests(unittest.TestCase):
    def test_the_event_carries_totals_and_nothing_about_functions_or_binaries(self):
        event = sentry_ledger.build_event(entry(), NOW, event_id="e" * 32, trace_id="t" * 32)
        self.assertEqual(event["type"], "transaction")
        self.assertEqual(event["measurements"]["perfect"], {"value": 4, "unit": "none"})
        self.assertEqual(event["measurements"]["instructions"]["value"], 123456789)
        self.assertEqual(event["tags"]["commit"], COMMIT)
        text = json.dumps(event)
        self.assertNotIn("secret_binary", text)
        self.assertNotIn("deadbeef", text)

    def test_no_speed_means_no_instructions(self):
        self.assertNotIn("instructions", sentry_ledger.build_event(entry(speed=None), NOW)["measurements"])

    def test_values_that_fail_their_check_are_rejected(self):
        for bad in (entry(commit="HEAD"), entry(cpu="x" * 101), entry(cpu="cpu\nline"),
                    entry(summary={**entry()["summary"], "perfect": "4"}),
                    entry(speed={"instructions": 1.5})):
            with self.assertRaises(ValueError):
                sentry_ledger.build_event(bad, NOW)

    def test_the_envelope_item_is_a_transaction_of_the_stated_length(self):
        event = sentry_ledger.build_event(entry(), NOW)
        header, item, payload, _ = sentry_ledger.envelope(event).split(b"\n")
        self.assertEqual(json.loads(header)["event_id"], event["event_id"])
        self.assertEqual(json.loads(item)["type"], "transaction")
        self.assertEqual(json.loads(item)["length"], len(payload))


class RunTests(unittest.TestCase):
    def run_with(self, env, dry_run=False, e=None):
        sent = []

        def opener(request, timeout):
            sent.append(request)
            return FakeResponse()

        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "entry.json"
            path.write_text(json.dumps(e or entry()))
            outcome, text = sentry_ledger.run(path, dry_run, env, opener, NOW)
        return outcome, text, sent

    def test_a_trusted_main_push_sends_one_transaction(self):
        outcome, _, sent = self.run_with(TRUSTED)
        self.assertEqual(outcome, "sent (HTTP 200)")
        self.assertEqual(len(sent), 1)
        self.assertTrue(sent[0].full_url.endswith("/api/42/envelope/"))

    def test_anything_else_is_a_dry_run(self):
        for env in ({**TRUSTED, "GITHUB_EVENT_NAME": "pull_request"}, {**TRUSTED, "GITHUB_REF": "refs/heads/x"},
                    {**TRUSTED, "GITHUB_REPOSITORY": "someone/Fission"}, {**TRUSTED, "SENTRY_DSN": ""}):
            outcome, _, sent = self.run_with(env)
            self.assertEqual((outcome, sent), ("dry run", []), env)
        self.assertEqual(self.run_with(TRUSTED, dry_run=True)[0], "dry run")

    def test_a_rejected_entry_is_reported_not_sent(self):
        outcome, text, sent = self.run_with(TRUSTED, e=entry(commit="HEAD"))
        self.assertEqual((outcome, sent), ("rejected", []))
        self.assertIn("Not sent", text)

    def test_main_exits_zero_even_when_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "entry.json"
            path.write_text("{}")
            self.assertEqual(sentry_ledger.main([str(path)], env={}), 0)


if __name__ == "__main__":
    unittest.main()
