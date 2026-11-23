# CI and CD operator guide

[CI and release gates](../docs/CI_RELEASE_GATES.md) defines the gate policy.
The workflow files and the [change classifier](../scripts/ci/change_plan.py)
define which jobs run.

| When | Workflow | What to check |
|---|---|---|
| Pull request, merge group, or main push | [CI](workflows/ci.yml) | Check `Detect changes` for the event's base/head and selected lanes. Check `CI Summary` for the result of every required lane. |
| Scheduled or manual extended validation | [CI Heavy Validation](workflows/ci-heavy.yml) | Check `Heavy Validation Summary` for platform builds and tests, feature compilation, coverage, and advisories. |
| Scheduled or manual corpus validation | [DecBench sweep](workflows/decbench-sweep.yml) | Check shard coverage and compare with a compatible main run. Use the [DecBench guide](../docs/BENCHMARK_DECBENCH.md) to interpret artifacts. |
| Scheduled or manual diagnostic checks | [Decompiler determinism](workflows/determinism.yml) and [Fuzz](workflows/fuzz.yml) | Check the job summary and any associated `ci-red` issue on main. |
| Intentional release | [Release Tag (CI green)](workflows/release-tag.yml) | Follow the [release checklist](../docs/RELEASE.md). |

For a red required check, open its failed job, then inspect the first failing
step. The last failure in the run may be `CI Summary` reporting another job's
failure. A skipped lane is expected when its change-plan flag is disabled.

Run the classifier and workflow-tool tests locally:

```bash
python3 -m unittest discover -s scripts/ci -p 'test_*.py'
actionlint -shellcheck= -pyflakes=
```

To inspect the classifier's path rules with real Git revisions, replace the two
SHAs in this example with the event's base and head.

```bash
PYTHONPATH=scripts/ci python3 - BASE_SHA HEAD_SHA <<'PYTHON'
import subprocess
import sys

from change_plan import plan

raw = subprocess.check_output(["git", "diff", "--name-only", "-z", sys.argv[1], sys.argv[2]])
lane, jobs = plan(raw.decode().rstrip("\0").split("\0") if raw else [], "pull_request")
print(lane)
print(*(name for name, enabled in jobs.items() if enabled), sep="\n")
PYTHON
```

Use the parent branch's SHA as the base for a stacked pull request. For a merge
candidate, pass `"merge_group"` to `plan`.

GitHub repository settings control whether the merge queue, tag protection,
release environment approvals, and immutable releases are enforced. See the
[operator rollout steps](../docs/CI_RELEASE_GATES.md#pr-and-merge-queue).
