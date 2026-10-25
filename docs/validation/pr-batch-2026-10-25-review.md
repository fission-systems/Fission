# Integration PR #674: conservative review rollback

This is a mechanical rollback and regression-test record, not a measured
decompiler-quality improvement. No source-semantic or external corpus benchmark
was run for this rollback.

## Recorded inputs

- Submitted integration: `bdf9895a54cb6e7f59490515507cba4671ed6bd4`.
- Pre-integration main used for the rollback: `360b14bf0533659592c922129f2b0f1ccb8e74d6`.
- Main after the independently reviewed #615 merge:
  `1773ab367b9a8f6696b8511a5a97536160e79548`.
- The integration preserves the original heads of 92 per-issue PRs. Merging
  those histories does not mean every proposed behavior was retained.

## Reproduced blockers and disposition

### #599: reciprocal division

The submitted recognizer rewrites `((int64_t)x * 0x55555556) >> 32` as division
by three. At `x = -1`, the arithmetic shift returns `-1`, while C division
returns `0`. A destination cast alone also does not prove a half-width source.

Restore `arith/div_mod.rs` to the recorded main version, retaining the required
`Call.origin` fixture fields, and add the negative regression
`review_signed_shift_is_not_truncating_division`. This defers the entire #599
contribution, including its other CDQ and runtime-helper changes. **#522 remains
unresolved.** The rollback makes no claim that the older recognizer is generally
correct.

### #666: may-alias memory definitions

`Builder::add_def` retained only definitely non-aliasing reaching definitions.
A may-alias store does not prove that it completely overwrites a previous
store. For `*local_18 = 7; *p = 9; return *local_18`, the first store was deleted
even when `p` could point elsewhere.

Remove that new invalidation line and retain the other changes, including
overlapping-use accounting. The regression is
`review_may_alias_store_does_not_kill_reaching_definition`.

### #668: VSA propagation enablement

Bottom-seeding locals exposes an existing transfer-width defect: a signed
comparison uses the Boolean result width to interpret its operands. The
submitted resolver chooses the true arm of `x = 1; switch (signed x < 0)`.

Restore `vsa/solver.rs` to the recorded main implementation. The switch-break test uses
a literal discriminator to test splicing independently of disabled propagation.
The negative regression is
`review_bottom_seeding_must_not_enable_bool_width_comparison_misfold`.
**The always-Top solver portion of #457 remains unresolved.** Operand-width
recovery and more precise propagation require a separate measured proposal.

Two further regressions on `a5472ce96` show that restoring only the solver is
insufficient. A new `Select` evaluator chooses `22` for `1 <s 0 ? 22 : 11`,
and singleton addition makes `(1 + 0) <s 0` provably true in the old comparison
transfer. Both arise because the transfer interprets operands at the Boolean
result width. They failed as
`review_select_precision_must_not_expose_comparison_width_bug` and
`review_add_precision_must_not_expose_comparison_width_bug`.

Also restore `vsa/transfer.rs` and `vsa/circle_range.rs` to the recorded main
versions, retaining the two new negative regressions. This defers the precision
changes in #581 and #641 and the CircleRange changes in #668; retain the separate
traversal and switch-splicing changes. **#542 and #483 remain unresolved too.**
The rollback does not establish general soundness of the old VSA domain or
transfers; it removes this batch's new precision that exposes the defect.

## Validation

The first three negative regressions failed on the submitted integration. The
temporary rollback passed all three and the related normalize, prehir, and
pcode suites: 1,802 passed, one skipped. The applied rollback is also checked
with the focused regressions and full workspace suite before submission.

```bash
cargo fmt --all --check
cargo nextest run -p fission-midend-normalize -p fission-midend-prehir \
  -E 'test(review_)' --no-fail-fast
cargo nextest run --workspace --locked --no-fail-fast
cargo check --workspace --locked
cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity
```

The initial applied rollback passed three focused regressions, workspace nextest
(3,761 passed, 10 skipped), workspace check, workspace Clippy with the flags
above, formatting, and CI. The additional two regressions require the expanded
rollback described above. The expanded rollback passed all five regressions and
workspace nextest (3,754 passed, 10 skipped). Final check, Clippy, and CI results
are recorded in the PR description after completion. Do not infer runtime parity or
benchmark quality from these tests. Issues #522, #457, #542, and #483 must remain
open if ancestor commit closing keywords close them automatically on merge.
