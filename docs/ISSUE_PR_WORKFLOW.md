# Issue and PR governance

This is the operational companion to [#1293](https://github.com/fission-systems/Fission/issues/1293).
It does not replace #1121 release gates, #1168 ownership or AGENTS.md.

## Managed labels

The source of truth is [`labels.json`](https://github.com/fission-systems/Fission/blob/main/.github/labels.json). Prefer existing
GitHub `bug`, `enhancement`, and `documentation` labels over duplicate
type aliases. New managed labels are additive. Unmanaged and legacy labels
are never deleted or changed automatically.

| Dimension | Cardinality | Rule |
| --- | --- | --- |
| `priority:P0`–`P3` | One after triage | Risk and urgency; P0 candidates are not automatically release blockers |
| `area:*` | One or more | Canonical code owner(s); on PRs added automatically from changed paths, owners the paths miss added by hand |
| `phase:*` | One primary | #1293 execution phase |
| `bug` / `enhancement` / `documentation` / `type:*` | One primary | Nature of the work |
| `status:needs-repro` / `confirmed` | At most one | Evidence classification for defect reports |
| `status:blocked` / `ready` | At most one | Triage readiness; can coexist with evidence classification |
| `risk:*` | As applicable | Semantic, integrity, concurrency or compatibility review |
| `release:blocker` | Maintainer decision | Reproduced or established risk within promised release scope |

P0: potentially critical false proof, corruption, loss, hang or release risk.
P1: important correctness/reliability or measured bottleneck.
P2: normal improvement/experimental work. P3: deferred work.
Record the reason, dependencies and evidence status in the issue body.
A candidate may be P0 + needs-repro; do not call it confirmed until evidence
is recorded. GitHub native draft, review, check and merge status are the
source of truth for PR state, not manually maintained duplicate labels.

## Issue lifecycle

Report → reproduce/establish evidence → triage ownership and dependencies →
ready → implementation PR → independent review/validation → merge →
acceptance verification → close. Blocked items must name their blocker.
Use Refs rather than Closes for partial work or umbrella issues; do not close
an issue before its acceptance criteria have been verified.

## Defect intake: probe and acceptance check

A defect report is a claim until something executable shows it. Each defect
needs two checks, both run against a named `main` SHA:

| Check | What it shows | Before the fix | After the fix |
| --- | --- | --- | --- |
| **Probe** | The defect: wrong output, panic, divergence | reproduces | no longer reproduces |
| **Acceptance check** | The expected behaviour, stated as a test | fails | passes |

Each check is a command or test someone else can run: a `cargo nextest`
filter, a `fission_cli` invocation on an in-repo fixture, or a native-replay
witness.

**Labels**
- An issue with both checks and their output on a named SHA gets
  `status:confirmed`.
- Anything less gets `status:needs-repro`, including audit findings that only
  cite a file and line. A `status:needs-repro` issue is not handed to an
  implementation agent; the first step is writing the probe.

**Results that are not a defect.** Triage closes or reclassifies these, and
records the probe output either way:
- Does not reproduce on current `main`: first check that the probe ran under
  the report's original conditions: the same ISA, OS, host architecture,
  compiler and flags, input, and feature set. Only then close it as not
  planned, with the probe output and the conditions used. If those conditions
  are not available, the result is an environment gap: record it and keep
  `status:needs-repro`.
- Already fixed: close and reference the fixing PR.
- Flaky: the probe passes sometimes. Keep `status:needs-repro` and record the
  pass rate. Never retry until green.
- Environment error: the probe or check could not run, for example a missing
  tool, a runner fault or a network failure. Record it and hold the verdict.
  This is neither a reproduction nor a non-reproduction.

**Defects no single run can show.** Some defects cannot be shown by running
one test: a false proof, a vacuous verification, an ownership or architecture
violation. These may reach `status:confirmed` through recorded evidence
instead, such as a counterexample, a mutation that the check fails to catch,
or a cited invariant with the code path that breaks it. That evidence must be
reviewed by someone other than the reporter, and the review must be noted on
the issue. The fixing PR still adds a test that fails on the base wherever one
can be written.

**Promotion.** The implementation PR commits the acceptance check as a
regression test in the owning crate, so it stays in the suite permanently.
The PR states that the test fails on the base and passes on the head. That is
the positive control the PR template already requires.

**Do not open an issue for work you are about to do.** An agent fixing a
defect in its own PR puts the problem statement, probe and acceptance check in
the PR body and opens no issue. Open an issue only for work that is not being
done now, and only with a probe; otherwise label it `status:needs-repro`.
Never file every finding as an issue: an unbounded backlog costs more triage
than it saves. On 2026-11-24, 168 issues were opened in one day and 190 open
issues had no label.

**Keep PRs small.** One defect or one invariant per PR. Unrelated fixes found
along the way go in their own PRs, or in an issue with a probe.

## PR lifecycle

1. Start from the linked issue and current main. State scope and invariant.
2. Open draft for incomplete work; apply type/phase/risk labels manually
   after reviewing actual changes. `area:*` labels are added automatically
   from the changed paths (below); add any owner the paths miss. Carry
   priority from the issue if appropriate.
3. Include reproducer or preventive-work rationale, expected behavior, tests,
   measured before/after evidence when required, and limitations. Use the PR template.
4. Mark ready for review when the applicable evidence is present. Independent
   reviewer checks ownership, correctness, failure paths, and compatibility.
   Who reviews what, and the SHA-bound review record: [REVIEW_POLICY.md](REVIEW_POLICY.md).
5. Resolve substantive review findings, then re-run applicable checks on the
   final head. GitHub branch protection and existing CI/release policies govern
   merging. Do not bypass them or assume a label proves CI passed.
6. Merge after required checks and review; record evidence and verify issue
   acceptance. If using Closes, verify all acceptance criteria before merge.

Use small coherent PRs; don't mix unrelated fixes. Research remains isolated
until qualification. No automatic merge or issue closing based on labels.

### Free-model drafts (`agent:free`)

Label a simple issue `agent:free`: a typo, a docs fix, or a small
single-purpose code change. Do not use it on an issue whose body carries a log
or decompiled output. `free-model-draft.yml` then asks OpenCode Zen free models
for a fix and opens it as a draft PR labelled `agent:free-draft`. Free models
have usage limits, so the run number picks which one goes first. A model
that fails or leaves no acceptable edits hands over to the next, up to four,
within the budgets and failure classes below.

- The model sees only the public checkout and the issue text. It can read
  and edit files, nothing else.
- Its edits are kept only under `docs/`, in Markdown and in Rust under
  `crates/`, small and text-only. Nothing in a proc-macro crate is kept,
  because `cargo check` would run it.
- Build-error repair loop: when the edits touch Rust under `crates/`, the
  workflow (the model has no shell) runs `cargo fmt` and
  `cargo check --locked -p <package> --all-targets` on each touched package.
  On a compiler error the model gets the diagnostics (the first ~6,000
  characters) in a new run and fixes only those, at most 2 rounds per model,
  with the edit check and cargo run again after each. A model that still
  fails hands over to the next. The Rust toolchain is installed only when a
  draft touches Rust, so a docs-only draft stays fast. The PR body says
  whether `cargo check` passed and after how many repair rounds. That is a
  compile check only: no test ran.
- Comment-only issues: an issue whose body carries the hidden marker
  `<!-- free-task-kind:comment-only -->` is behaviour-neutral by design, so
  the edit check also requires every added or removed line in a `*.rs` file to
  be a comment (`//`, `///`, `//!`, or inside `/* */`) or blank. It judges the
  lines of the staged zero-context diff: a line that has code before a `//`,
  or sits inside a string, counts as code, and so does any deleted code line.
  An added comment line that opens a code fence (three or more backticks or
  tildes) or an indented code block is refused too, because rustdoc would run
  it as a doctest. `#[doc = ...]` is code, so it is refused.
  Markdown is not restricted further. The check runs in the model loop and
  again in the job that opens the PR.
- Budgets, shared by model switches and repairs: at most 5 `opencode run`
  calls per task (`MAX_INVOCATIONS_PER_TASK`, counted in the loop's state dir;
  it fails with `BUDGET_EXHAUSTED` when spent, instead of the old worst case of
  4 models x 3 calls). No opencode or cargo call starts if its timeout would
  cross `BUDGET_SECS`: the timeout is shortened to the time left, or the call
  is skipped (under 120 s for opencode, 300 s for cargo).
- Failure classes (`free_model_draft.py classify`, from the opencode exit
  status, its `--format json` error events and text, and the cargo log). Each
  attempt gets a row in the step summary.

  | Class | Action |
  |---|---|
  | `RATE_LIMITED` (429, "rate limit", "quota", "too many requests") | stop the whole task, exit 75, Retry-After reported when seen |
  | `PROVIDER_UNAVAILABLE` (connection refused, DNS, 5xx) | stop the task, exit 76 |
  | `INFRA_FAILURE` (cargo cannot download or reach the registry) | never sent to the model; cargo retried once, then stop, exit 77 |
  | `BUDGET_EXHAUSTED` | stop, exit 78 |
  | `MODEL_UNAVAILABLE`, `NO_CHANGES`, `POLICY_VIOLATION` | next model |
  | `COMPILATION_ERROR` | repair loop; `cargo fmt` failures count (a rustfmt failure is almost always a syntax error and its message goes to the model) |

  After `RATE_LIMITED` or `PROVIDER_UNAVAILABLE` the issue gets `agent:deferred`
  with a comment (reason and time); the scheduler removes it after 6 hours. Any
  other failed draft gets `agent:blocked`, so it is not retried until a human
  removes the label. Run `python3 scripts/check/sync_github_labels.py --apply`
  once so the labels exist.
- Duplicate guard: before drafting, and again before opening the PR, the
  workflow does nothing if an open PR labelled `agent:free-draft` says
  `Fixes #<issue>` or an open PR comes from a branch `agent-free/issue-<issue>-*`.
  The issue gets one comment saying so (checked first, so never repeated).
- `scripts/ci/free_model_draft.py` holds the rules and their tests;
  `scripts/ci/free_model_draft_loop.sh` is the model loop;
  `scripts/ci/test_free_model_draft_loop.py` simulates it with a fake `opencode`.

#### Scheduler (`free-agent-scheduler.yml`)

State lives in GitHub (issues, labels, runs, PRs, artifacts): no database, no
state branch. A workflow `concurrency:` group keeps one pending run and cancels
the rest, so it cannot queue tasks. Instead a cron (every 30 minutes,
`scripts/ci/free_agent_scheduler.py tick`) starts one `free-model-draft.yml`
run per tick, within `.github/free-agent-policy.json`:

- `max_concurrent` (1): draft runs queued or in progress;
- `max_daily_tasks` (10): draft runs started today (UTC), the only hard cap on usage;
- `max_pending_draft_prs` (10): open `agent:free-draft` PRs, because review is
  the bottleneck.

It picks the oldest open `agent:free` issue with no open draft PR and no
`agent:blocked` or `agent:deferred` label. At the start of each tick it removes
`agent:deferred` from issues that have had it for 6 hours (the `labeled` event
time in the issue timeline). Labelling an issue `agent:free` by hand still
starts a run at once. Try it without side effects: run the workflow with
`dry_run` (the default), or
`python3 scripts/ci/free_agent_scheduler.py tick --dry-run`.

#### Usage measurement (`free-agent-report.yml`)

The quota is measured from normal runs, never probed. `opencode run` uses
`--format json`; each invocation's events go to `metrics/<n>.jsonl`, and every
draft run uploads `free-agent-metrics-<run_id>` (90 days) with `metrics.json`:
per invocation the model, kind (draft or repair), seconds, LLM requests (the
`step_finish` count), tokens (in, out, reasoning, cache read), status codes,
Retry-After and class. It holds numbers, model ids, class names and status codes
only. A field the events lack is unknown (`?`), never 0, and opencode's internal
retries are not events, so a rate-limited model may show only as slow
invocations. `scripts/ci/free_agent_report.py` (daily, or on dispatch) writes
per day and model: runs, invocations, requests, tokens, 429s, rate-limited runs,
accepted drafts and mean seconds per invocation to the job summary. The
scheduler prints today's totals on every tick.

#### Mined issues (`free-task-miner.yml`)

A daily workflow (`scripts/ci/free_task_miner.py`) finds documentation work with
a clear target, with no model involved in finding it. Findings that need writing
(`missing-docs`, `rustdoc`) become `agent:free` + `area:docs` issues, at most 3
per run in total (kinds take turns); the scheduler above starts their drafts
(issues created with `GITHUB_TOKEN` trigger nothing, and the miner starts no
draft itself). Findings with an exact before and after (`home-path`,
`broken-link`) need no model: `--fix` applies one group of them, each
replacement only if `before` occurs exactly once on the recorded file:line (a
mismatch is skipped and reported), and the workflow runs
`free_model_draft.py check` on the result and opens a draft PR labelled
`agent:deterministic` on a branch `agent-fix/<fingerprint>`. No such PR is opened
while 3 are open. Detectors:

- `home-path`: an absolute home path in maintained Markdown, with the portable
  replacement.
- `broken-link`: a relative Markdown link whose target exists exactly once
  elsewhere under the same name.
- `missing-docs`: public items without a doc comment, from `missing_docs`
  warnings of one crate (`cargo rustc -p <crate> --lib -- -W missing_docs`, so
  only that crate is compiled with the lint). The issue asks for `///`
  comments that describe only what the code visibly does.
- `rustdoc`: rustdoc lint warnings of that crate (`cargo doc --no-deps`:
  broken intra-doc links, invalid HTML tags, bare URLs, ...).

The two Rust detectors run on one crate per run, picked by day of year from
`DOC_CRATES` in the script, so a run stays bounded. Their issues carry the
comment-only marker above, so a model cannot change logic. Groups are at most
8 files and 15 items (Markdown kinds: 12 files and 40 replacements). Every
finding has a hidden `free-task-item` key without a line number; a finding that
any `agent:free` issue (open or closed) or any open `agent:deterministic` PR
already carries is not filed or fixed again.
Dry run locally: `python3 scripts/ci/free_task_miner.py --dry-run --doc-crate
<crate>` (`--no-rust-docs` skips the cargo detectors).

The draft gets no CI and cannot merge. An Opus review of the diff comes first.
Marking it ready then starts CI, and the usual lifecycle above applies. If the
model produced nothing acceptable, the workflow says so on the issue. Hand the
issue to a stronger model.

## Bootstrap and migration

Maintainer with GitHub CLI access:

```bash
python3 scripts/check/sync_github_labels.py
python3 scripts/check/sync_github_labels.py --apply
```

The first command is read-only; the second creates missing labels only.
Existing metadata drift is reported, not overwritten. Review drift separately.
GitHub CLI authentication and repository label-write permission are required.
Optionally use the manual Sync managed labels workflow once merged.
The one automatic labelling is `area:*` on pull requests: the Label PR areas
workflow (`.github/labeler.yml`) adds them from the paths a PR changes, on
open and push, and never removes a label. `risk:*`, type, priority, phase and
status labels are never applied automatically, except by the Sentry triage
below, on the issues it files. Otherwise no automatic mass
labeling: classify small batches after reviewing issue content, and preserve
existing labels. Start with #1296–#1298 and then the
#1293 backlog. Do not infer confirmation or release-blocker status from titles.

## Sentry triage

`.github/workflows/sentry-triage.yml` runs `scripts/ci/sentry_triage.py`
every six hours. It reads unresolved Sentry issues of project `fission` and
considers one only if its latest event follows the Sentry defect-event
contract: environment `ci` or `nightly`, category `confirmed_semantic_defect`
or `crash`, kind not `flaky_test`, and every contract tag present and valid,
including `run_url`. It then checks the event against GitHub rather than
trusting it: the run exists in this repository, was triggered by the event its
environment stands for (`schedule` for `nightly`; `push`, `merge_group` or
`workflow_dispatch` for `ci`), tested the reported commit, and that commit is on
`main`. Anything else is ignored. The `main` check runs at triage time: a
`merge_group` event names the queue's head commit, which becomes a `main`
commit only when the group merges, so it is held until then and never filed if
the group failed.

The issue is built only from validated tags and Sentry's id, counts, first and
last seen, and the suspect commit SHA if Sentry has one. No event text is
copied. It carries a `Probe` section naming `repro_id` and `commit`, and an
empty `Acceptance` section.

| Label | Rule |
| --- | --- |
| `bug` | Every filed issue is a defect candidate |
| `status:needs-repro` | Always. The event names a probe but records neither its output nor an acceptance check, which `status:confirmed` requires (above), even when `repro_id` is an in-repo fixture |
| `priority:P1` | Both eligible categories are correctness or reliability defects; P0 is a maintainer's escalation, never automatic |
| `area:*` | From `component`, by the crate paths in `.github/labeler.yml`; none if no path matches |

Labels that do not already exist in the repository are dropped, never created.

A hidden `<!-- sentry-issue: <id> -->` marker deduplicates: an issue with it,
open or closed, is left alone. A closed one is never reopened or commented on.
At most 5 issues are filed per run and 10 per UTC day.

Every run is a dry run that writes the would-be issues to the job summary and
the `sentry-triage-preview` artifact. Filing needs the repository variable
`SENTRY_TRIAGE_ENABLED` set to `true`. The Sentry issue is not linked back to
the GitHub issue: that needs a Sentry token with `event:write`, and the triage
only reads Sentry.
