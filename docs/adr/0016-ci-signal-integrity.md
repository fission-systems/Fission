# ADR 0016: CI signal integrity

**Status:** Accepted (phase 1 and hardening implemented; phases 2–4 proposed)
**Last verified:** 2026-11-21
**Builds on:** PR #980 (exact-SHA change plans, merge-group support, pre-merge
lanes, nightly DecBench sweep, release promotion)

## Context

Fission is developed by one person and a number of concurrent agent sessions.
In the 30 days to 2026-11-21, at least 200 PRs merged, half of them within 20
minutes of opening, and Fast Gate took a median 8.5 minutes (p90 12.8). At that
rate CI is the only reviewer most changes get, so what matters is whether its
green and red mean what they say. Measured over the same 30 days
(`gh api .../actions/runs`, 799 runs of `ci.yml`, `ci-heavy.yml` and
`fuzz.yml`):

| Signal | What happened |
|---|---|
| PR red for reasons outside the PR | 113 of 137 failed PR runs were `cargo deny` advisories; 105 of them on one day, when one advisory was published while 105 PRs were open. 19 job failures were the sccache setup step (the cache service), not the build. |
| Main green while broken | 2026-11-21: #976 and #979, each green, added the same function; merged back to back, main stopped compiling (E0428). Their push runs were cancelled by the next push, and the next push (#981) changed only docs, so its run did not compile. 36 of 228 main push runs (16%) were cancelled. |
| Nightly red with no one told | Heavy failed 18 of 31 nights, 17 of them consecutively (Windows native debugger, 2026-10-26 to 11-12), while the release gate required Heavy green. The ELF fuzz target crashed on the same `align_up` overflow for four weeks. |
| Checks that could not fail | Doctests ran as `cargo test --doc … \|\| true`. Stacked PRs (`base` not `main`) ran no CI at all. The pre-merge debugger, solver and loader lanes added in #980 had never run; the first run of the macOS one failed every test. |
| A gate that blocked the right change | Pass-Gate's targeted-test rule required a new test for #983, which only deleted a duplicate definition. |
| Policy drift | `auto-merge-approved-prs.yml` enabled auto-merge, which the project does not use. |
| Fuzzing without memory | The fuzz corpus was uploaded as an artifact and never read back; every run began from the seeds, for 60 seconds. |

## Decision

Four rules, each a response to a row above.

1. **A required check depends only on the change.** Inputs that move with time
   (the advisory database) gate a PR only when the PR changes them (`Cargo.lock`,
   `deny.toml`, `.cargo/audit.toml`) and are otherwise checked nightly. Caches
   fail open: an unavailable cache service makes a job slower, not red.
2. **Main only advances to a validated tree.** Each push to main keeps its own
   run (no cancellation) and always compiles and lints the workspace, whatever
   the diff. Phase 2 makes the merge queue required so the validated tree is the
   merged tree.
3. **A red scheduled run opens an issue.** Heavy, fuzz and the DecBench sweep
   report through `reusable-report-red.yml`: one `ci-red` issue per workflow
   while it is red, a comment per further red run (the range to bisect), closed
   on the first green run. Fuzzing is not a gate; it runs on a schedule, keeps
   its corpus between runs, and reports.
4. **Every check can fail and has run.** No `|| true` on tests; nextest uses
   `--no-tests=fail`; a lane's definition changing runs the lane (#980). A gate
   that fires on a change it cannot be about (a removal) is narrowed, not
   worked around with a placeholder test. A test whose input is missing does
   not return early, which nextest counts as a pass: a committed input
   (`utils/`, `testdata/`) is `expect`ed; one only a developer machine has (a
   benchmark corpus, the gitignored `utils/source/`) makes the test
   `#[ignore = "..."]`, and it panics when an opt-in run cannot find it; an
   external tool (python3, the Swift demangler) goes through
   `fission_core::test_inputs::absent`, which fails under
   `FISSION_REQUIRE_TEST_INPUTS=1`, set in every CI test job. The z3
   differential is opt-in, so a missing z3 always fails it. Counted
   on 2026-11-22: 37 tests had passed in CI without running because their
   input was absent there (besides the three engine differential tests
   #997 fixed), and 46 more skipped on a committed input that could only be
   missing from a broken checkout.

## Hardening, and what was measured before adding it

Each check below was measured on main (9dd3bb658) before it was added, so its
cost and what it catches are known rather than assumed.

| Check | Measured | Where it runs |
|---|---|---|
| Third-party actions pinned to commit SHAs; template expansions moved into `env`; no workflow-wide `actions: write` | zizmor 1.32.0: 92 unpinned actions (one a branch, `dtolnay/rust-toolchain@stable`), 18 template injections, 2 over-broad permissions, all high severity; 0 after. Caches authenticate with the runner token, so dropping `actions: write` costs nothing | zizmor (high severity) in Script Checks whenever a workflow changes |
| Decompiler determinism | 2,868 functions over 18 DecBench binaries, decompiled at 1, 8 and 8 threads: one function (`dpkg-query` O0 `dpkg_options_parse`) printed `printforhelp = help_str;` or `*(unsigned long long *)(&printforhelp) = help_str;`, 8 runs to 4, even single-threaded. The nightly sample, run on main: 26 of 12,505 functions in 5 of 37 binaries differ across four runs (10 minutes locally) | PR fixtures: 3 runs each (all deterministic today). Nightly `determinism.yml`: reports into a `ci-red` issue, not yet a release gate; it joins Heavy once main is deterministic |
| Overflow checks and debug assertions in release builds | 0 panics over the same 2,868 functions; the 83 errors are lift failures on PE data symbols. Kept because the class has occurred (four panic sites from a debug sweep, the `align_up` fuzz crash) and costs little on the fixture lane | PR fixture lane |
| Retry masking | 0 tests passed only on retry in 59 recent green test jobs | Not changed |
| Hangs | nextest already terminates a test after 120 s | Not changed |

## Phases

| Phase | Content | State |
|---|---|---|
| 1 | Advisories off the PR path and into Heavy; sccache fails open; every main push lints the workspace; `ci-red` issues for Heavy, fuzz and DecBench; fuzz on a schedule with a persistent corpus; doctests enforced; removal-only midend changes exempt from the targeted-test rule; auto-merge workflow removed | This change |
| 2 | Require the merge queue on `main` (`Detect changes` and `CI Summary` already report on `merge_group`) | Repository setting; the owner's decision |
| 3 | A PR that can change decompiler output gets an output diff against its base on a fixed DecBench slice: functions whose text changed, then GED and type deltas. Output hashes first, scores second | Proposed |
| 4 | One release CLI per main SHA, built by the push run and reused by the nightly sweep and release E2E, so what is measured is what ships | Proposed |

## Consequences

- A new advisory no longer turns every open PR red; it turns the next Heavy run
  red and opens one issue. Heavy gates releases, so a release still cannot ship
  with an unignored advisory.
- Main pushes cost a workspace clippy run even for docs (about 5 minutes with a
  warm cache).
- Scheduled failures become visible issues. They name a SHA range, not a cause.
- Fuzz crashes no longer fail main pushes. Crash inputs stay attached to the run
  for 30 days; turning one into a regression test is part of fixing it.
