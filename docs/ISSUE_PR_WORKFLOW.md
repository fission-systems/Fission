# Issue and PR governance

This is the operational companion to [#1293](https://github.com/fission-systems/Fission/issues/1293).
It does not replace #1121 release gates, #1168 ownership or AGENTS.md.

## Managed labels

The source of truth is [labels.json](../.github/labels.json). Prefer existing
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

## PR lifecycle

1. Start from the linked issue and current main. State scope and invariant.
2. Open draft for incomplete work; apply type/area/phase/risk labels manually
   after reviewing actual changes. Carry priority from the issue if appropriate.
3. Include reproducer or preventive-work rationale, expected behavior, tests,
   measured before/after evidence when required, and limitations. Use the PR template.
4. Mark ready for review when the applicable evidence is present. Independent
   reviewer checks ownership, correctness, failure paths, and compatibility.
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
