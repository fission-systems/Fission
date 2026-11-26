"""Simulation of free_model_draft_loop.sh with a fake `opencode`, `cargo` and `timeout` on PATH.

Nothing here reaches a model, the network or GitHub. The fake opencode prints the JSON
events of `opencode run --format json` (a `step_finish` line, an `error` line with a status
code) according to FAKE_MODE; the fake cargo fails according to CARGO_MODE.
"""

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
LOOP = HERE / "free_model_draft_loop.sh"
METRICS = HERE / "free_agent_metrics.py"

STEP = ('{"type":"step_finish","timestamp":1,"sessionID":"s","part":{"type":"step-finish","cost":0,'
        '"tokens":{"input":100,"output":20,"reasoning":5,"cache":{"read":7,"write":0}}}}')
RATE = ('{"type":"error","timestamp":2,"sessionID":"s","error":{"name":"APIError","data":{"message":"Rate limit",'
        '"statusCode":429,"isRetryable":true,"responseHeaders":{"retry-after":"30"}}}}')

FAKE_OPENCODE = f"""#!/usr/bin/env bash
if [ "$1" = models ]; then
  printf 'opencode/deepseek-v4.1-flash-free\\nopencode/hy4-flash-free\\nopencode/beam-free\\nopencode/big-pickle\\n'
  exit 0
fi
model=""
while [ $# -gt 0 ]; do
  case "$1" in --model) model="$2"; shift 2 ;; *) shift ;; esac
done
echo "opencode run $model" >> "$FAKE_LOG"
echo '{STEP}'
case "$FAKE_MODE" in
  ratelimit) echo '{RATE}'; exit 1 ;;
  nochange) exit 0 ;;
  docs) echo "more" >> docs/a.md; exit 0 ;;
  rust) echo "// edit" >> crates/x/src/lib.rs; exit 0 ;;
esac
"""

FAKE_CARGO = """#!/usr/bin/env bash
echo "cargo $*" >> "$FAKE_LOG"
[ "$1" = fmt ] && exit 0
case "$CARGO_MODE" in
  compile) echo "error[E0308]: mismatched types"; echo " --> crates/x/src/lib.rs:1:1"; exit 101 ;;
  infra) echo "warning: spurious network error (2 tries remaining): timed out"
         echo "error: failed to download from https://index.crates.io/x"; exit 101 ;;
esac
exit 0
"""

FAKE_TIMEOUT = """#!/usr/bin/env bash
echo "timeout $1" >> "$FAKE_LOG"
shift
exec "$@"
"""


def sh(cwd, *cmd):
    subprocess.run(cmd, cwd=cwd, check=True, capture_output=True)


class LoopSimulation(unittest.TestCase):
    def setUp(self):
        base = Path(tempfile.mkdtemp())
        self.repo, self.draft, self.bin = base / "repo", base / "draft", base / "bin"
        for d in (self.repo / "docs", self.repo / "crates/x/src", self.draft, self.bin):
            d.mkdir(parents=True)
        (self.repo / "docs/a.md").write_text("hello\n")
        (self.repo / "crates/x/Cargo.toml").write_text('[package]\nname = "px"\nversion = "0.1.0"\n')
        (self.repo / "crates/x/src/lib.rs").write_text("pub fn f() {}\n")
        for cmd in (("init", "-q"), ("config", "user.email", "t@t"), ("config", "user.name", "t"),
                    ("add", "-A"), ("commit", "-qm", "x")):
            sh(self.repo, "git", *cmd)
        (self.draft / "prompt.txt").write_text("fix it\n")
        (self.draft / "permissions.json").write_text("{}")
        self.issue = base / "issue.json"
        self.issue.write_text(json.dumps({"number": 7, "title": "t", "body": "fix", "state": "OPEN",
                                          "labels": [{"name": "agent:free"}]}))
        for name, text in (("opencode", FAKE_OPENCODE), ("cargo", FAKE_CARGO), ("timeout", FAKE_TIMEOUT)):
            path = self.bin / name
            path.write_text(text)
            path.chmod(path.stat().st_mode | stat.S_IEXEC)
        self.log = base / "fake.log"
        self.out, self.summary = base / "out", base / "summary"

    def run_loop(self, fake_mode, cargo_mode="ok", **env):
        full = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "DRAFT": str(self.draft),
                "ISSUE_JSON": str(self.issue), "SEED": "0", "RUST_READY": "1", "FAKE_MODE": fake_mode,
                "CARGO_MODE": cargo_mode, "FAKE_LOG": str(self.log), "GITHUB_OUTPUT": str(self.out),
                "GITHUB_STEP_SUMMARY": str(self.summary), **env}
        return subprocess.run(["bash", str(LOOP)], cwd=self.repo, env=full, capture_output=True, text=True)

    def calls(self, prefix):
        return [line for line in self.log.read_text().splitlines() if line.startswith(prefix)] if self.log.exists() else []

    def outputs(self):
        return self.out.read_text() if self.out.exists() else ""

    def metrics(self):
        subprocess.run([sys.executable, str(METRICS), "finish", "--draft", str(self.draft),
                        "--run-id", "42", "--issue", "7"], check=True, capture_output=True)
        return json.loads((self.draft / "metrics/metrics.json").read_text())

    def test_the_shared_budget_stops_the_task_at_five_opencode_calls(self):
        # Every draft changes Rust and cargo always fails: 1 draft + 2 repairs on the first model,
        # then the second model drafts and repairs once; the sixth call is refused.
        run = self.run_loop("rust", "compile")
        self.assertEqual(run.returncode, 78, run.stdout + run.stderr)
        self.assertEqual(len(self.calls("opencode run")), 5)
        self.assertEqual((self.draft / "state/invocations").read_text().strip(), "5")
        self.assertIn("stop_class=BUDGET_EXHAUSTED", self.outputs())
        self.assertIn("Task stopped: BUDGET_EXHAUSTED", self.summary.read_text())
        models = [c.split()[-1] for c in self.calls("opencode run")]
        self.assertEqual(len(set(models)), 2)  # model switches share the budget with repairs
        data = self.metrics()
        self.assertEqual((data["outcome"], len(data["invocations"])), ("BUDGET_EXHAUSTED", 5))
        self.assertEqual(data["invocations"][0]["class"], "COMPILATION_ERROR")

    def test_a_rate_limited_attempt_stops_the_task_without_trying_the_next_model(self):
        run = self.run_loop("ratelimit")
        self.assertEqual(run.returncode, 75, run.stdout + run.stderr)
        self.assertEqual(len(self.calls("opencode run")), 1)
        out = self.outputs()
        self.assertIn("stop_class=RATE_LIMITED", out)
        self.assertIn("retry_after=30 seconds", out)
        self.assertIn("Task stopped: RATE_LIMITED", self.summary.read_text())
        self.assertIn("RATE_LIMITED | retry after 30 seconds", self.summary.read_text())
        data = self.metrics()
        self.assertEqual(data["outcome"], "RATE_LIMITED")
        (inv,) = data["invocations"]
        self.assertEqual((inv["class"], inv["status_codes"], inv["retry_after"], inv["llm_requests"],
                          inv["tokens_in"], inv["tokens_out"], inv["tokens_reasoning"], inv["cache_read"]),
                         ("RATE_LIMITED", [429], 30, 1, 100, 20, 5, 7))
        self.assertNotIn("Rate limit", json.dumps(data))  # no provider or model text

    def test_an_infrastructure_cargo_failure_never_reaches_the_model(self):
        run = self.run_loop("rust", "infra")
        self.assertEqual(run.returncode, 77, run.stdout + run.stderr)
        self.assertEqual(len(self.calls("opencode run")), 1)             # the draft only: no repair call
        self.assertEqual(len(self.calls("cargo check")), 2)              # retried once
        self.assertFalse((self.draft / "repair.txt").exists())
        self.assertFalse((self.draft / "errors.txt").exists())
        self.assertIn("stop_class=INFRA_FAILURE", self.outputs())

    def test_a_compile_error_does_reach_the_repair_loop(self):
        run = self.run_loop("rust", "compile", MAX_INVOCATIONS_PER_TASK="20")
        self.assertEqual(run.returncode, 1)  # all four models fail after their repairs
        self.assertTrue((self.draft / "repair.txt").exists())
        self.assertIn("mismatched types", (self.draft / "errors.txt").read_text())
        self.assertEqual(self.metrics()["outcome"], "NO_ACCEPTABLE_DRAFT")

    def test_no_changes_hands_over_to_the_next_model_and_is_classified(self):
        run = self.run_loop("nochange")
        self.assertEqual(run.returncode, 1)
        self.assertEqual(len(self.calls("opencode run")), 4)
        self.assertEqual(self.summary.read_text().count("NO_CHANGES"), 4)

    def test_opencode_runs_with_json_output_and_a_per_invocation_file(self):
        self.run_loop("ratelimit")
        self.assertTrue((self.draft / "metrics/1.jsonl").read_text().startswith('{"type":"step_finish"'))

    def test_the_timeout_is_shortened_to_the_time_left(self):
        self.run_loop("ratelimit", BUDGET_SECS="300")
        (timeout,) = self.calls("timeout")
        self.assertLessEqual(int(timeout.split()[1]), 300)
        self.assertGreater(int(timeout.split()[1]), 250)

    def test_no_call_starts_when_less_than_the_minimum_is_left(self):
        run = self.run_loop("ratelimit", BUDGET_SECS="100")
        self.assertEqual(run.returncode, 1)
        self.assertEqual(self.calls("opencode run"), [])
        self.assertEqual(self.metrics()["outcome"], "OUT_OF_TIME")

    def test_a_docs_only_draft_is_accepted_without_cargo(self):
        run = self.run_loop("docs")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertEqual(self.calls("cargo"), [])
        self.assertIn("cargo=skipped", self.outputs())
        self.assertTrue((self.draft / "edits.patch").exists())
        self.assertEqual(self.metrics()["outcome"], "ACCEPTED")

    def test_a_rust_draft_that_compiles_is_accepted_after_fmt_and_check(self):
        run = self.run_loop("rust", "ok")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertEqual(len(self.calls("cargo")), 2)
        self.assertIn("cargo=passed", self.outputs())

    def test_a_rustfmt_failure_is_a_compilation_error_for_the_repair_loop(self):
        fmt_fails = FAKE_CARGO.replace('[ "$1" = fmt ] && exit 0',
                                       '[ "$1" = fmt ] && { echo "error: expected one of `;`, found `}`"; exit 1; }')
        (self.bin / "cargo").write_text(fmt_fails)
        run = self.run_loop("rust", "ok", MAX_INVOCATIONS_PER_TASK="3")
        self.assertEqual(run.returncode, 78, run.stdout + run.stderr)
        self.assertEqual(self.calls("cargo check"), [])  # not checked: rustfmt already failed
        self.assertIn("expected one of", (self.draft / "errors.txt").read_text())
        self.assertTrue((self.draft / "repair.txt").exists())


if __name__ == "__main__":
    unittest.main()
