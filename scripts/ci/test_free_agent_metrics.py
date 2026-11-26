import json
import tempfile
import unittest
from pathlib import Path

import free_agent_metrics as fam
import free_agent_report as far

STEP = {"type": "step_finish", "timestamp": 1, "sessionID": "s",
        "part": {"type": "step-finish", "reason": "stop", "cost": 0,
                 "tokens": {"input": 100, "output": 20, "reasoning": 5, "cache": {"read": 7, "write": 1}}}}
ERR = {"type": "error", "timestamp": 2, "sessionID": "s",
       "error": {"name": "APIError", "data": {"message": "SECRET MODEL TEXT", "statusCode": 429, "isRetryable": True,
                                              "responseHeaders": {"Retry-After": "30"}}}}


def jsonl(*events):
    return "\n".join(json.dumps(e) for e in events) + "\n"


class UsageTests(unittest.TestCase):
    def test_step_finish_events_are_summed_and_errors_give_codes_and_retry_after(self):
        usage = fam.usage_of(jsonl(STEP, STEP, {"type": "text", "part": {"text": "hello"}}, ERR))
        self.assertEqual((usage["llm_requests"], usage["tokens_in"], usage["tokens_out"],
                          usage["tokens_reasoning"], usage["cache_read"]), (2, 200, 40, 10, 14))
        self.assertEqual((usage["status_codes"], usage["retry_after"]), ([429], 30))

    def test_missing_fields_are_unknown_not_zero(self):
        usage = fam.usage_of(jsonl({"type": "step_finish"}, {"type": "step_finish", "part": {"tokens": {"input": 4}}}))
        self.assertEqual((usage["llm_requests"], usage["tokens_in"], usage["tokens_out"]), (2, 4, None))
        self.assertIsNone(usage["cache_read"])
        empty = fam.usage_of("")
        self.assertEqual((empty["llm_requests"], empty["tokens_in"]), (0, None))

    def test_garbage_lines_and_retry_events_do_not_break_it(self):
        retry = {"type": "retry", "error": {"data": {"statusCode": 503}}}
        usage = fam.usage_of("junk\n" + jsonl(retry) + "{not json\n")
        self.assertEqual((usage["status_codes"], usage["retry_events"]), ([503], 1))


class MetricsFileTests(unittest.TestCase):
    def test_metrics_hold_only_safe_fields_no_model_or_issue_text(self):
        record = fam.invocation_record(jsonl(STEP, ERR), "opencode/beam-free", "draft", 42, "RATE_LIMITED", 1)
        metrics = fam.build_metrics([record], "123", "7", "RATE_LIMITED", 1000, 1042)
        text = json.dumps(metrics)
        self.assertNotIn("SECRET MODEL TEXT", text)
        self.assertEqual(metrics["outcome"], "RATE_LIMITED")
        self.assertEqual((metrics["run_id"], metrics["issue"]), (123, 7))
        inv = metrics["invocations"][0]
        self.assertEqual((inv["model"], inv["kind"], inv["seconds"], inv["llm_requests"], inv["status_codes"],
                          inv["retry_after"], inv["class"]),
                         ("opencode/beam-free", "draft", 42, 1, [429], 30, "RATE_LIMITED"))

    def test_hostile_values_are_replaced(self):
        raw = {"model": "x y; rm -rf", "kind": "<b>", "class": "free text", "status_codes": [429, "x", 99999, 503],
               "seconds": "9", "tokens_in": True}
        inv = fam.sanitize_invocation(raw)
        self.assertEqual((inv["model"], inv["kind"], inv["class"], inv["status_codes"], inv["seconds"],
                          inv["tokens_in"]), ("unknown", "draft", "NONE", [429, 503], 0, None))
        metrics = fam.build_metrics([], "not a number", "7;", "free text", 1, 2)
        self.assertEqual((metrics["run_id"], metrics["issue"], metrics["outcome"]), (None, None, "INCOMPLETE"))

    def test_the_commands_record_reclass_and_finish(self):
        with tempfile.TemporaryDirectory() as tmp:
            draft = Path(tmp)
            (draft / "metrics").mkdir()
            (draft / "state").mkdir()
            (draft / "metrics" / "1.jsonl").write_text(jsonl(STEP))
            (draft / "state" / "outcome").write_text("ACCEPTED\n")
            (draft / "state" / "start").write_text("1000\n")
            fam.main(["record", "--draft", tmp, "--n", "1", "--model", "opencode/beam-free", "--kind", "draft",
                      "--seconds", "9", "--class", "OK"])
            fam.main(["reclass", "--draft", tmp, "--class", "COMPILATION_ERROR"])
            fam.main(["finish", "--draft", tmp, "--run-id", "5", "--issue", "7"])
            data = json.loads((draft / "metrics" / "metrics.json").read_text())
            self.assertEqual(data["outcome"], "ACCEPTED")
            self.assertEqual(data["started"], "1970-01-01T00:16:40Z")
            self.assertEqual(data["invocations"][0]["class"], "COMPILATION_ERROR")
            self.assertEqual(data["invocations"][0]["tokens_in"], 100)

    def test_finish_without_any_state_still_writes_an_incomplete_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            fam.main(["finish", "--draft", tmp, "--run-id", "5", "--issue", "7"])
            data = json.loads((Path(tmp) / "metrics" / "metrics.json").read_text())
            self.assertEqual((data["outcome"], data["invocations"]), ("INCOMPLETE", []))

    def test_the_summary_table_shows_unknown_as_a_question_mark(self):
        record = fam.invocation_record(jsonl({"type": "step_finish"}), "opencode/beam-free", "repair", 3, "OK", 2)
        table = fam.summary_table(fam.build_metrics([record], "1", "2", "ACCEPTED", 0, 3))
        self.assertIn("| 2 | `opencode/beam-free` | repair | 3 | 1 | ? | ? | ? | ? | - | OK |", table)


def run_metrics(run_id, day, outcome, invocations):
    return {"run_id": run_id, "issue": 1, "outcome": outcome, "started": f"{day}T10:00:00Z",
            "finished": f"{day}T10:30:00Z", "invocations": invocations}


def inv(model, requests, tin, seconds, cls="OK", codes=()):
    return {"model": model, "kind": "draft", "seconds": seconds, "llm_requests": requests, "tokens_in": tin,
            "tokens_out": 10, "tokens_reasoning": None, "cache_read": 5, "status_codes": list(codes),
            "class": cls}


class ReportTests(unittest.TestCase):
    def fixtures(self):
        return [
            run_metrics(1, "2026-11-25", "ACCEPTED", [inv("m/a", 3, 100, 60), inv("m/a", 2, 50, 30)]),
            run_metrics(2, "2026-11-25", "RATE_LIMITED", [inv("m/a", 1, None, 10, "RATE_LIMITED", [429])]),
            run_metrics(3, "2026-11-26", "ACCEPTED", [inv("m/b", 4, 400, 100)]),
            run_metrics(4, "2026-11-26", "NO_ACCEPTABLE_DRAFT", [inv("m/b", 1, 10, 20, "NO_CHANGES"), inv("m/c", 0, None, 5)]),
        ]

    def test_totals_per_day_and_model(self):
        rows = far.aggregate(self.fixtures())
        a = rows[("2026-11-25", "m/a")]
        self.assertEqual((len(a["runs"]), a["invocations"], a["requests"], a["tokens_in"], a["tokens_out"],
                          a["cache_read"], a["c429"], len(a["rate_limited"]), a["accepted"], a["seconds"]),
                         (2, 3, 6, 150, 30, 15, 1, 1, 1, 100))
        self.assertIsNone(a["tokens_reasoning"])  # never reported: unknown, not 0
        day = rows[("2026-11-25", far.TOTAL)]
        self.assertEqual((len(day["runs"]), day["accepted"], len(day["rate_limited"])), (2, 1, 1))
        self.assertEqual(rows[("2026-11-26", "m/b")]["accepted"], 1)
        self.assertEqual(rows[("2026-11-26", "m/c")]["tokens_in"], None)

    def test_the_table_has_a_row_per_day_and_model_with_a_mean(self):
        text = far.render(far.aggregate(self.fixtures()))
        self.assertIn("| 2026-11-25 | m/a | 2 | 3 | 6 | 150 | 30 | ? | 15 | 1 | 1 | 1 | 33 |", text)
        self.assertIn("| 2026-11-26 | (all models) |", text)
        self.assertLess(text.index("2026-11-25"), text.index("2026-11-26"))

    def test_files_are_found_under_downloaded_artifact_dirs(self):
        with tempfile.TemporaryDirectory() as tmp:
            for m in self.fixtures():
                path = Path(tmp) / str(m["run_id"])
                path.mkdir()
                (path / "metrics.json").write_text(json.dumps(m))
            (Path(tmp) / "9").mkdir()
            (Path(tmp) / "9" / "metrics.json").write_text("not json")
            self.assertEqual(len(far.load_metrics(Path(tmp))), 4)
            self.assertEqual(far.main(["--metrics-dir", tmp]), 0)

    def test_an_empty_report_says_so(self):
        self.assertIn("no metrics found", far.render({}))


if __name__ == "__main__":
    unittest.main()
