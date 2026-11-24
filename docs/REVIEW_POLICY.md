# Review policy

Which independent reviews a pull request gets, chosen by the owners it touches
(`area:*`) and its `risk:*` labels. Labels and the PR lifecycle are defined in
[ISSUE_PR_WORKFLOW.md](ISSUE_PR_WORKFLOW.md); this page only adds who looks at
what. An independent reviewer is one that did not write the change: a person,
or an agent given the diff and the claim, not the author's conclusion.

## What review is for

Review approval, human or AI, is not proof of correctness
([ADR 0018](adr/0018-decbench-is-not-a-correctness-oracle.md)). Correctness
comes from native execution, `verify`, call-site counts and tests that can
fail. A reviewer's job is to find the case those did not cover and to check
that the evidence offered could have failed. The reviews that found defects in
this repository did exactly that:

- **A search that ended in the wrong state.** In #1106 a reviewer found that
  the solver-backed search ended `Exhausted` when replays were unconfirmed or
  undecided; it had to end Unknown or Budget (`8d09a69f6`).
- **A test that passed without the fix.** In #1240 the new structuring test
  passed with the old tombstone too; the reviewer wrote one that fails on it
  (`1537ea107`).
- **Combined trees.** Reviewers merged all open PRs onto main in one tree
  before merging any of them, which surfaces interactions no single-PR run sees.
- **Mutation check.** Reviewers reverted each fix and confirmed that every new
  test fails without it.

## Required for every PR

1. Every new or changed test fails with the fix reverted (mutation check);
   the PR says how this was shown.
2. Required checks green on the final head; `cargo xtask preflight` before
   pushing.
3. The review is bound to a commit. The reviewer, human or agent, puts these
   fields in the review comment:

   ```text
   reviewed_sha:        <full head SHA reviewed>
   review_scope:        <files/owners read; what was not read>
   findings:            <each finding and its resolution, or "none">
   validation_evidence: <commands run, mutation check, native/verify runs>
   unresolved_risks:    <what could still be wrong, or "none">
   ```

4. Reviewed PRs enter the merge queue with
   `gh pr merge N --merge --match-head-commit <reviewed_sha>`, so a commit
   pushed after the review blocks the merge until it is reviewed again.
   The review record above is what binds a review to a commit, not a GitHub
   approval: branch protection requires 0 approvals, because every PR is
   opened from one account and GitHub does not let an author approve their
   own PR, so requiring one would block every merge. Where an approval is
   given it is dismissed on a new push (`dismiss_stale_reviews: true`).
   Never `--admin`, never bypass required checks, review or branch
   protection.

## Reviewer focus by area and risk

| Touches | Reviewer focus |
|---|---|
| `risk:semantic` | Can the output or claim be wrong while tests pass? Each new ending, verdict or fold: which inputs reach it, and is there an Unknown/Budget path? Native execution or `verify` evidence, not scores. |
| `risk:data-loss` | Persistence and evidence integrity: partial writes, crash between steps, snapshot compatibility. Round-trip test on old data. |
| `risk:concurrency` | Liveness: blocking inside rayon or a `OnceLock` initialiser, lock order, shared caches. A stress run, not one green run. |
| `risk:breaking` | CLI/JSON contract, schema and flags: what an agent consuming the old shape now sees. Schema check and changelog line. |
| `area:solver`, `area:verify` | Soundness of SAT/UNSAT/Equivalent and search endings; z3 differential where one exists. |
| `area:midend`, `area:sleigh` | Owner per AGENTS.md (no printer/CLI repair), ISA-agnostic rule, targeted test, real-binary before/after when quality is claimed. |
| `area:emulator`, `area:dynamic` | JIT/interpreter differential at the same boundary; OS process control on the platforms CI cannot run. |
| `area:loader`, `area:static`, `area:analysis-db`, `area:decompiler` | Facts at the canonical owner, no parallel fact maps; determinism of anything feeding snapshots. |
| `area:cli`, `area:plugin` | Contract tests and manual `info`/`list`/`decomp` smoke. |
| `area:ci`, `area:qa` | The gate can fail: show it red on a planted defect. Running cost does not exceed what it saves. |
| `area:docs` only | One reviewer; commands in the text were run. |

Two or more `risk:*` labels, or `risk:semantic` on `area:solver`/`area:verify`,
get two independent reviewers (each with its own review record; agents count,
as defined above), and the combined-tree check before merge.
