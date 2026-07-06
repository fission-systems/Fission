# ADR 0007: AI overfit and validation firewall

**Status:** Accepted
**Last verified:** 2026-07-04

## Context

Fission quality work may use structured prompts to ask other AI models for
decompiler improvement ideas. That loop is useful, but it can overfit faster
than manual development if prompts expose benchmark identity such as function
names, addresses, binary paths, corpus rows, or compiler tuples.

The existing decompiler quality gate prevents row-specific production branches
inside the semantic layer. This decision adds the same guard one step earlier:
prompt inputs and validation surfaces must be translated into owner-native
invariants before any implementation is accepted.

## Decision

AI review prompts for semantic decompiler work must use the dedicated prompt
template and provide structural evidence only:

- failure shape,
- owner evidence,
- invariant candidates,
- forbidden shortcuts,
- validation matrix.

Prompts must not expose benchmark function names, addresses, binary paths,
corpus row ids, or row-identifying compiler tuples when asking for
implementation advice. Concrete row anchors remain allowed in the local
decompiler change proposal, but they are evidence for human review, not prompt
payload for external AI suggestions.

Fission will maintain a patch validation pool separate from dev/holdout. The
pool is used as go/stop regression evidence only. It is not shown on public
dashboards, not used for ranking, and not used for repeated tuning. The pool
should rotate periodically so it does not become an implicit benchmark target.

Prompt leak scans, benchmark-smell scans, architecture-isolation audits,
regression replay, and metric-gaming analysis are audit tools. They may be run
locally, scheduled, or wired into CI later, but they are not the source of
truth. The source of truth is the architecture contract in
[`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md): an
AI-suggested or benchmark-motivated change must be restated as an invariant at
the canonical semantic owner before production code is added.

Ghidra remains a cleanroom reference for correctness, algorithms, and
invariants. Fission must not optimize to mimic Ghidra-specific presentation
quirks when correctness can be preserved with clearer pseudocode.

## Consequences

- AI-assisted changes have a reviewable boundary before benchmark identity can
  bias implementation.
- dev/holdout are less likely to become tuning data through repeated AI loops.
- Static scans can surface high-confidence leaks, while report-only audits can
  mature without blocking unrelated work.
- Reviewers can reject claims based on metric movement alone when correctness
  is flat or worse.
