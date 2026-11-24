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
| `area:*` | One or more | Canonical code owner(s), not just changed paths |
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
2. Open draft for incomplete work; apply type/area/phase/risk labels manually
   after reviewing actual changes. Carry priority from the issue if appropriate.
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
No automatic mass labeling: classify small batches after reviewing issue
content, and preserve existing labels. Start with #1296–#1298 and then the
#1293 backlog. Do not infer confirmation or release-blocker status from titles.
