# For-loop region ownership and loop-control finalization

## Status

Proposed for issue #37.

## Observed defect

`try_lower_for` recognizes an init block as the unique predecessor outside the
natural loop and embeds that assignment in `PreHirStmt::For::init`. The block
is outside the candidate's `[start, skip_to)` interval, but the reducer does
not report it through `record_extra_absorbed_member`. A surrounding region can
therefore emit the same CFG block as a residual statement and emit the init a
second time.

The loop-body reducer normally rewrites head/exit gotos to `continue`/`break`.
However, `lower_loop_body_subgraph` has an early return when an accepted child
region reaches a loop exit. That path returns before the common final rewrite,
so a `for` body can retain a goto to the loop head or exit.

## Invariant and owner

- A structured candidate owns every CFG block whose statements it embeds,
  including non-contiguous blocks such as a `for` init predecessor.
- Every successful loop-body lowering path must pass through the same
  loop-context rewrite and trailing-continue cleanup.

The canonical owner is `crates/fission-midend-structuring/src/loops.rs`:
`try_lower_for` owns the init membership fact, while
`lower_loop_body_subgraph` owns the shared body finalization contract.

The existing simple for fixture is not changed to claim a readability gain;
it is a mechanical regression surface for the ownership/control-flow
invariants. The shared rewrite is already present on the ordinary completion
path, so the fix closes only the unfinalized early-return path.

## Scoped change

1. Record `init_idx` as an extra absorbed member only after all for-loop
   lowering succeeds.
2. Factor loop-control rewriting/trailing-continue cleanup into one helper and
   invoke it before every successful `lower_loop_body_subgraph` return.
3. Add focused regressions for non-contiguous init ownership and for gotos
   escaping an accepted child region.

No acceptance rule, ISA-specific condition, address guard, or printer change
is required.

## Validation

- Focused structuring-loop regressions.
- `cargo nextest run -p fission-midend-structuring`.
- Existing `fission-pcode` structuring loop tests.
- `cargo check -p fission-pcode` and `git diff --check`.

This is a semantic/ownership correction. Any claim about improved decompiler
quality requires a measured real-binary before/after row separately.
