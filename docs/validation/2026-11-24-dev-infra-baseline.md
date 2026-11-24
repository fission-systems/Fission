# Dev-infra baseline, 2026-11-24

This snapshot is taken before the following land, so their effect can be measured later with the same method:

- `cargo xtask preflight`
- the review policy
- the area labeler
- the daily flow report
- the org's move from GitHub Free to Team on 2026-11-24 at about 06:30 to 06:45Z.

The upgrade moves the documented job concurrency limit from 20 to 60. That limit is not read here: the limits API needs `admin:org`, so both numbers are unverified. The concurrency table below is what actually ran at once.

**Window:** 2026-11-17 06:30Z to 2026-11-24 06:30Z, ending before the upgrade. The open-PR state is as of 2026-11-24T06:55Z, because GitHub keeps no history of it. Numbers: `2026-11-24-dev-infra-baseline.json`, from the script's `--json-out`. The collection made 781 GitHub API calls.

**Comparing after the upgrade:** once 24 to 48 hours of post-upgrade data exist, run the same method over the new window:

```bash
python3 scripts/report/dev_metrics.py --runner-metrics \
  --since 2026-11-24T06:45:00Z --until <now> \
  --compare docs/validation/2026-11-24-dev-infra-baseline.json
```

The workloads differ, so every time comes with its sample count (n, runs, jobs) and the workload in jobs per hour. Read a delta against those, not alone.

## Method

- **Lead time:** merged PRs that went through the merge queue. The split is:
  - last push to required checks done: the earliest `pull_request` check suite on the head commit, to the last `Detect changes` or `CI Summary` completion before the last queue add;
  - checks done to added to the queue;
  - queue to merged (enqueue to merge).

  Only the two required checks count. The report-only base-vs-head job does not.
- **Approval to enqueue cannot be measured.** No GitHub approvals are recorded (`required_approving_review_count` is 0). The proxy is "checks done to added to the queue". An auto-merge enabled before the checks finish counts as 0.
- **Merged without the queue:** counted separately and left out of the lead time.
  - The first queued merge was at 2026-11-22 13:16Z. 77 of the 78 unqueued merges predate it; #1010 (2026-11-23) went around the queue.
  - Only 45 merges feed the lead-time split. A later window will be all-queue, so compare the queue-only rows.
- **CI failure rate:** the required `CI Summary` job's conclusion per Fast Gate (`ci.yml`) run created in the window. The denominator is success plus failure. Cancelled runs, and runs with no `CI Summary`, are reported apart.
- **Merge-queue ejections:** counted separately, as `RemovedFromMergeQueueEvent` with a reason other than `merged`.
- **Runners: the jobs of every workflow's runs created in the window.** All workflows share the concurrency limit.
  - Job queue is `started_at - created_at`; job run is `completed_at - started_at`. Skipped jobs are excluded.
  - Run start is run `created_at` to its first job's `started_at`.
- **Peak concurrent jobs:** a sweep over job start and completion times, per runner label.
  - A completion sorts before a start at the same instant.
  - Jobs shorter than 5 s are excluded, since skips get a runner for a moment.
  - A job running across an hour counts in that hour.
  - "all but macOS" is the Linux and Windows pool. macOS has its own, smaller limit.
- **Fast Gate only:** runner minutes per merged PR, merge-group regenerations and duplicate cost.
  - A `pull_request` run belongs to the PRs it lists, or to the merged PR whose head branch it ran on while that PR was open. The API drops the list once the branch is deleted.
  - A `merge_group` run belongs to the PR in its `gh-readonly-queue/main/pr-N-*` branch.
  - **No group-size division.** The queue (`max_entries_to_build: 4`, `max_entries_to_merge: 4`, ALLGREEN) builds one run per entry, so each run is charged to its own PR.
  - **Duplicate cost:** the merge-group minutes of jobs whose name also ran in the PR's last `pull_request` run on its merged head.
  - All of these are wall-clock job minutes, not billed minutes.

## What the concurrency data shows, and what it does not

- **Plateaus at 20.** Peak concurrent jobs sit at exactly 20 in many hours, for example six of the seven hours 2026-11-24 00 to 06Z and every hour 2026-11-21 13 to 17Z. That fits the Free plan's documented 20-job limit.
- **Peaks above 20 are unexplained.** 2026-11-23 02 to 06Z peaks at 27 to 30, also on the Linux and Windows pool alone. That contradicts a hard 20-job limit, and nothing here explains it. Possible causes, none of them checked:
  - jobs whose `started_at` is set before a runner is held;
  - re-run attempts overlapping;
  - a limit that is not what the documentation says.
- **Queue time follows the plateaus.** Job queue p95 is about 5 to 6.5 minutes over the week, and the worst waits (18 to 21 minutes) are on 2026-11-24 05 to 06Z, just before the upgrade.

## Maintainer-intervention proxy

The API has no field for "a maintainer stepped in". The proxy counts events that need a person, or an agent acting for one, in PRs merged in the window or open at its end:

| Signal | What it captures | What it misses |
|---|---|---|
| merge-main commits | a two-parent commit titled `Merge branch 'main'` / `Merge remote-tracking branch 'origin/main'`: catching a PR up by hand | rebases (see force pushes), catch-ups squashed away, differently titled "Update branch" commits |
| force pushes | `HeadRefForcePushedEvent`: rebases, amended fixes | why the push happened |
| re-queues after an ejection | a queue add after a non-merge removal | a fix re-queued as a fresh PR |
| workflow re-runs | Fast Gate runs with `run_attempt > 1` | re-runs of other workflows; pushes made only to retrigger CI |
| queue adds by actor | who pressed merge, by login | human vs agent: both use the same account, so they cannot be told apart from the API |

It does not capture review time, local investigation, or any work that leaves no GitHub event.

## Snapshot

fission-systems/Fission. Open-PR state as of 2026-11-24T06:55Z.

### Action required

**Ready to enqueue** (merge conditions met, not queued): `gh pr merge N --merge --match-head-commit <reviewed_sha>`

- #1073 test(emulator): native single-instruction differential against the SLEIGH lift (x86-64 + AArch64)
- #1096 quotient: WBCQ prototype -- behavioural quotients of P-code slices (RFC #1093, P0/P1)
- #1106 causal: CSIC P0/P1/P2 -- counterfactual runner, input-first candidates, solver-backed joint search (RFC #1101 Track B)
- #1285 fix(normalize): preserve offsets, strides, and structured reads in MemSSA and RLE
- #1292 fix(normalize): eliminate None==None, control-flow leak, and 1-bit/effect bugs in double_precision and three_way
- #1294 fix(midend-normalize): fix constant folding, cast stripping, iblock phi, and subvar flow soundness (#875)
- #1295 docs: adopt correctness-first implementation and validation workflow for Fission 1.0
- #1300 fix(midend-normalize): fix soundness bugs across cleanup and global_opt passes (#890)
- #1306 fix(normalize): resolve soundness bugs in copy_prop, sccp, post_assign, subflow, split_flow, loops_conds, and rule_normalizer

**Conflicting or more than 50 commits behind main** (author):

- #176 Project Fission: A Unified Semantic Engine (conflicting)
- #1045 fix(debug): Windows session reads the thread it stopped; no poll error while frozen (141 behind)
- #1073 test(emulator): native single-instruction differential against the SLEIGH lift (x86-64 + AArch64) (62 behind)
- #1099 structuring: opt-in SAILR driver (fourth candidate) (141 behind)

**Open more than 7 days** (land, split or close):

- #176 Project Fission: A Unified Semantic Engine (55 d)

### Who acts next

| State | Next actor | PRs | Oldest in state |
|---|---|---|---|
| Queued | merge queue | 11 | 0.2 h |
| Waiting for CI | Actions | 7 | 0.3 h |
| Changes requested | author | 0 | n/a |
| Waiting for dependency | the other PR | 4 | 22.8 h |
| Ready to enqueue | merger | 9 | 22.7 h |
| Waiting for review | reviewer | 0 | n/a |

<details><summary>Queued (11)</summary>

- #1094 structuring: a loop that exits from the middle of its body is a loop (0.0 h in state)
- #1114 fix(emulator): support half-precision (2-byte) float varnodes (0.1 h in state)
- #1115 fix(emulator): handle wide (>16-byte) integer shifts and ops in evaluator and JIT (0.1 h in state)
- #1148 fix(sleigh): sign-extend AArch64 W-register byte/halfword loads to 32 bits (0.1 h in state)
- #1156 fix(dynamic): overlay breakpoints, guard handles, rollback step-off, reset timeline (0.2 h in state)
- #1169 fix(emulator): fix ET_EXEC relocation bias and sync Windows heap base (0.1 h in state)
- #1202 fix(emulator): keep symbolic expressions through the interpreter's in-place ops and stores (0.1 h in state)
- #1214 fix(sleigh): Thumb stmia Rn! writeback and Thumb-2 logical-op carry (0.1 h in state)
- #1240 fix(structuring): validate and tombstone rotated loop bodies by sorted position (0.2 h in state)
- #1299 chore: establish managed issue labels and correctness-first PR lifecycle (0.2 h in state)
- #1301 ci: remove unnecessary Linux test dependency from CLI smoke critical path (0.2 h in state)

</details>

<details><summary>Waiting for CI (7)</summary>

- #1116 fix(emulator): enforce ISA NaN propagation rules in float evaluation (0.3 h in state)
- #1128 fix(emulator): implement SSE/MMX and AArch64 NEON vector userops (0.2 h in state)
- #1302 fix(midend-normalize): fix arithmetic soundness, width, and overflow bugs across arith passes (#906) (0.1 h in state)
- #1303 docs: defect intake needs a probe and an acceptance check (0.0 h in state)
- #1304 docs: review policy by area and risk, with SHA-bound review records (0.0 h in state)
- #1305 ci: label pull requests with area:* from changed paths (0.0 h in state)
- #1307 release: draft notes from merged PRs and open the Release PR (0.0 h in state)

</details>

<details><summary>Waiting for dependency (4)</summary>

- #1097 cli: `quotient` -- WBCQ behavioural quotients from the CLI (RFC #1093, P2) (22.8 h in state)
- #1102 quotient: P3 evaluation -- obfuscated corpus, native ground truth, DecBench reach (RFC #1093) (18.9 h in state)
- #1107 quotient: calls are effects, not refusals (WBCQ P4 step 1, RFC #1093) (15.3 h in state)
- #1229 quotient: pointer fields are regions of their own (WBCQ P4 step 2, RFC #1093) (2.5 h in state)

</details>

<details><summary>Ready to enqueue (9)</summary>

- #1073 test(emulator): native single-instruction differential against the SLEIGH lift (x86-64 + AArch64) (0.0 h in state)
- #1096 quotient: WBCQ prototype -- behavioural quotients of P-code slices (RFC #1093, P0/P1) (22.7 h in state)
- #1106 causal: CSIC P0/P1/P2 -- counterfactual runner, input-first candidates, solver-backed joint search (RFC #1101 Track B) (0.3 h in state)
- #1285 fix(normalize): preserve offsets, strides, and structured reads in MemSSA and RLE (0.5 h in state)
- #1292 fix(normalize): eliminate None==None, control-flow leak, and 1-bit/effect bugs in double_precision and three_way (0.5 h in state)
- #1294 fix(midend-normalize): fix constant folding, cast stripping, iblock phi, and subvar flow soundness (#875) (0.0 h in state)
- #1295 docs: adopt correctness-first implementation and validation workflow for Fission 1.0 (0.4 h in state)
- #1300 fix(midend-normalize): fix soundness bugs across cleanup and global_opt passes (#890) (0.0 h in state)
- #1306 fix(normalize): resolve soundness bugs in copy_prop, sccp, post_assign, subflow, split_flow, loops_conds, and rule_normalizer (0.0 h in state)

</details>

### Flow

| Lead time (h) | p50 | p95 | max | n |
|---|---|---|---|---|
| last push -> required checks done | 0.2 | 3.9 | 12.7 | 45 |
| checks done -> added to merge queue | 0.9 | 20.6 | 22.8 | 45 |
| queue -> merged | 0.4 | 0.7 | 0.8 | 45 |
| last push -> merged | 2.2 | 21.3 | 23.6 | 45 |

Approval -> enqueue cannot be measured: no approvals are recorded (no review is required), so "checks done -> added to merge queue" is the proxy, and an auto-merge enabled before the checks finish counts as 0.
- Merged: 123 (17.6/day, 0.73/hour, peak 13 in one hour). Not through the queue: 78; queued without required checks on head: 0.
- Merge queue ejections: 7 {'failed_checks': 4, 'invalid_merge_commit': 3}; re-queued after an ejection: 6.
- Required CI failure rate: pull_request 25.9 (79/305), merge_group 12.5 (8/64); cancelled runs not counted (0 + 0).
- Issues: 231 opened, 54 closed; 190 open without a label.
- Open PRs: 34 (3 draft), age p50 0.2 d, oldest 54.8 d.
- Intervention proxy: 27 merge-main commits, 12 force pushes, 6 re-queues, 7 workflow re-runs; queue adds by {'sjkim1127': 58}.

### Runners (all workflows)

Runner minutes, regenerations and duplicate cost below are Fast Gate (ci.yml) only.

| Event | Runs | Run start p50 / p95 / max (min) | Jobs | Jobs/hour | Job queue p50 / p95 / max (min) | Job run p50 / p95 (min) |
|---|---|---|---|---|---|---|
| pull_request | 312 | 0.1 / 4.4 / 13.7 | 3342 | 19.9 | 0.0 / 5.1 / 21.2 | 1.2 / 12.0 |
| merge_group | 64 | 0.1 / 1.1 / 2.6 | 680 | 4.0 | 0.1 / 6.5 / 18.7 | 1.3 / 10.4 |
| push | 155 | 0.1 / 6.1 / 9.7 | 918 | 5.5 | 0.0 / 2.9 / 18.5 | 1.1 / 8.5 |

Peak concurrent jobs (jobs of at least 5 s; the plan's limit is not read from the API, which needs admin:org):

| Runner label | Peak | Median hourly peak | Hours |
|---|---|---|---|
| all | 30 | 10 | 93 |
| all but macOS | 30 | 9 | 93 |
| macos-15-intel | 3 | 1 | 6 |
| macos-latest | 5 | 2 | 51 |
| ubuntu-22.04 | 3 | 1 | 22 |
| ubuntu-22.04-arm | 3 | 1 | 6 |
| ubuntu-24.04 | 12 | 3 | 42 |
| ubuntu-24.04-arm | 1 | 1 | 5 |
| ubuntu-cca-77785908-7ef3-498a-9ae0-bbebb95125d3 | 1 | 1 | 1 |
| ubuntu-latest | 21 | 9 | 89 |
| windows-latest | 5 | 2 | 33 |

<details><summary>Peak concurrent jobs per hour, all labels (UTC)</summary>

| Hour | Peak |
|---|---|
| 2026-11-17 07:00 | 1 |
| 2026-11-17 09:00 | 2 |
| 2026-11-17 10:00 | 7 |
| 2026-11-17 11:00 | 2 |
| 2026-11-18 01:00 | 1 |
| 2026-11-18 09:00 | 2 |
| 2026-11-18 10:00 | 7 |
| 2026-11-18 12:00 | 15 |
| 2026-11-18 13:00 | 3 |
| 2026-11-18 22:00 | 5 |
| 2026-11-19 02:00 | 8 |
| 2026-11-19 05:00 | 7 |
| 2026-11-19 06:00 | 6 |
| 2026-11-19 07:00 | 6 |
| 2026-11-19 08:00 | 1 |
| 2026-11-19 09:00 | 2 |
| 2026-11-19 10:00 | 11 |
| 2026-11-19 11:00 | 11 |
| 2026-11-19 12:00 | 11 |
| 2026-11-19 14:00 | 5 |
| 2026-11-19 15:00 | 5 |
| 2026-11-19 16:00 | 5 |
| 2026-11-20 01:00 | 10 |
| 2026-11-20 02:00 | 10 |
| 2026-11-20 03:00 | 11 |
| 2026-11-20 04:00 | 6 |
| 2026-11-20 05:00 | 6 |
| 2026-11-20 06:00 | 6 |
| 2026-11-20 07:00 | 7 |
| 2026-11-20 08:00 | 8 |
| 2026-11-20 09:00 | 2 |
| 2026-11-20 10:00 | 7 |
| 2026-11-20 11:00 | 12 |
| 2026-11-20 12:00 | 7 |
| 2026-11-20 13:00 | 1 |
| 2026-11-20 14:00 | 5 |
| 2026-11-20 21:00 | 8 |
| 2026-11-20 22:00 | 5 |
| 2026-11-20 23:00 | 8 |
| 2026-11-21 00:00 | 7 |
| 2026-11-21 05:00 | 10 |
| 2026-11-21 06:00 | 4 |
| 2026-11-21 08:00 | 2 |
| 2026-11-21 09:00 | 20 |
| 2026-11-21 10:00 | 20 |
| 2026-11-21 11:00 | 10 |
| 2026-11-21 12:00 | 18 |
| 2026-11-21 13:00 | 20 |
| 2026-11-21 14:00 | 20 |
| 2026-11-21 15:00 | 20 |
| 2026-11-21 16:00 | 20 |
| 2026-11-21 17:00 | 20 |
| 2026-11-21 18:00 | 1 |
| 2026-11-22 03:00 | 13 |
| 2026-11-22 04:00 | 19 |
| 2026-11-22 05:00 | 9 |
| 2026-11-22 06:00 | 6 |
| 2026-11-22 08:00 | 12 |
| 2026-11-22 09:00 | 20 |
| 2026-11-22 10:00 | 20 |
| 2026-11-22 13:00 | 20 |
| 2026-11-22 14:00 | 20 |
| 2026-11-22 15:00 | 13 |
| 2026-11-22 16:00 | 14 |
| 2026-11-22 17:00 | 8 |
| 2026-11-22 20:00 | 15 |
| 2026-11-22 21:00 | 11 |
| 2026-11-22 22:00 | 2 |
| 2026-11-23 00:00 | 13 |
| 2026-11-23 01:00 | 9 |
| 2026-11-23 02:00 | 28 |
| 2026-11-23 03:00 | 30 |
| 2026-11-23 04:00 | 20 |
| 2026-11-23 05:00 | 20 |
| 2026-11-23 06:00 | 27 |
| 2026-11-23 07:00 | 20 |
| 2026-11-23 08:00 | 3 |
| 2026-11-23 09:00 | 12 |
| 2026-11-23 10:00 | 15 |
| 2026-11-23 11:00 | 10 |
| 2026-11-23 12:00 | 3 |
| 2026-11-23 14:00 | 20 |
| 2026-11-23 15:00 | 17 |
| 2026-11-23 20:00 | 1 |
| 2026-11-23 21:00 | 6 |
| 2026-11-23 22:00 | 1 |
| 2026-11-24 00:00 | 20 |
| 2026-11-24 01:00 | 20 |
| 2026-11-24 02:00 | 20 |
| 2026-11-24 03:00 | 20 |
| 2026-11-24 04:00 | 4 |
| 2026-11-24 05:00 | 20 |
| 2026-11-24 06:00 | 20 |

</details>

- Runner minutes per merged PR: p50 33.5, p95 332.6, mean 80.6.
- Merge-group regenerations (runs beyond the first per merged PR): 13; cancelled merge_group runs: 0.
- PR-vs-merge-group duplicate cost: 1310.4 runner-min; top jobs: Tests (Linux) / test (core) 290, Native fixture replay and Python port 258, Tests (Linux) / test (extras) 167, decomp-corpus / Binary fixtures (ubuntu-latest) 133, CLI Smoke (Linux · PE sample) / Smoke 80.

