# Decompiler Change Proposal: `cross_block.rs` Merge-Binding Synthesis Falls Back to Scalar SSA

Date: 2026-08-06

## 1. Context

Direct follow-up to `2026-08-06-interference-checked-phi-congruence.md`, which
flagged as future work that `materialize/cross_block.rs`'s own merge-binding
synthesis (`synthesize_explicit_merge_bindings_for_block`) still resolves
values by scanning predecessor p-code for a directly-visible producer op,
rather than consulting `scalar_ssa`'s already-correct, dominance-frontier-
placed phi nodes. That scan has two structural limits: it only ever
considers ops living directly in an immediate predecessor block (a value
reaching the merge through a longer chain is invisible to it), and its final
gate requires *exactly* two predecessors with a fully-resolved candidate on
both sides -- if even one predecessor doesn't have a directly-scannable op,
the merge is silently dropped. This is architecturally the same class of gap
this session's `kv_lookup`/`list_sum` fixes closed at the structuring layer.

## 2. The fix

`synthesize_explicit_merge_bindings_for_block` now seeds its `pending` map
with entries derived from `scalar_ssa.phis[block_idx]` whenever the existing
ad hoc scan didn't already produce a complete, usable 2-predecessor entry for
that storage. Two new helpers do the work:

- `resolve_ssa_value_to_expr(value_id, merge_block_idx)`: walks an SSA value
  to its defining p-code op and lowers it via the same
  `try_lower_materialized_output_rhs` the existing scan already uses.
- `resolve_ssa_phi_incoming_by_pred(phi, merge_block_idx)`: resolves every
  operand of a phi into the `(predecessor address -> PreHirExpr)` shape the
  rest of the function already expects.

Everything downstream of that -- the 2-predecessor gate, the diamond-select
synthesis, the `allow_fallback_intrinsic` gate -- is untouched. This keeps
the change additive: it only ever fills in a `pending` entry the scan
couldn't complete, never overrides one the scan already resolved fully.

## 3. Two real bugs found while building this, and the safeguards that came out of it

Reaching a safe design took three iterations, not one, and both intermediate
failures were caught by this session's own corpus-verification discipline
before shipping -- worth recording precisely, since the final code's shape
is a direct answer to each one.

**Attempt 1: duplicate computation.** The first version fired for any phi
`scalar_ssa` placed at a block, with no check that the phi's own output was
actually *read* there. `scalar_ssa` places a phi wherever a storage is live
across a merge, including storages merely passing *through* a block on their
way to a later use -- eagerly materializing a binding for one of those
duplicated whatever the real control-flow branches already computed for it.
Corpus A/B showed 800-3000+ line diffs per file with genuinely duplicated
statement pairs. Fix: `phi_output_is_consumed_in_block` requires a real use
of the phi's output within the exact merge block, mirroring the existing
scan's own `consumer_block_idx == block_idx` requirement.

**Attempt 2: a real value bug.** With duplication fixed, one case remained:
`___chkstk_ms` (a real stack-probe helper in the dev corpus) got a
synthesized `eax < 4096 ? iVar4 : ptr - 1024` at the join *after* an
if-guarded loop. One operand's defining op (`ptr -= 1024`) lives inside the
loop's latch -- it's a *one-iteration* delta, not the value that actually
reaches the merge after however many pages the loop probed. A flat
expression evaluated once at the merge is only correct if the loop runs
exactly once; for any larger stack allocation it's wrong. Excluding loop
*header* blocks as merge sites (the first attempted fix) didn't catch this,
because the merge block here is the join *after* the whole loop, not the
loop's own header. The real invariant has to live at the operand, not the
merge site: `resolve_ssa_value_to_expr` now declines whenever the operand's
defining op sits inside a natural loop body that does *not* also contain the
merge block -- allowing the case where both sides are in the *same* loop
(an if/else joining back within a single iteration, where a flat expression
is still exactly right) and rejecting the case where the operand's loop is
strictly upstream of the merge.

**Confirmed the guard actually matters**: temporarily disabling the
loop-boundary check made the new regression test fail with a synthesized
`100 - 1` (a constant one-iteration delta, since the test's loop body has no
other inputs) instead of declining -- proving the test would have caught
attempt 2's exact failure mode.

## 4. Verification

- New unit test,
  `merge_bindings_decline_operand_defined_inside_a_loop_reaching_a_post_loop_merge`:
  reproduces `___chkstk_ms`'s shape directly (`v = 100;` then either skip to
  a merge or enter a self-loop doing `v = v - 1;` before falling through to
  the same merge, which reads `v`), asserts no `Select` gets synthesized.
  Confirmed failing (with the exact wrong-constant shape) when the guard is
  disabled, confirming it's a real regression test and not vacuous.
- `cargo nextest run -p fission-pcode -p fission-midend-structuring -p fission-midend-normalize -p fission-midend-core -p fission-midend-prehir`:
  1390/1390 passed.
- No panics across all 72 dev-corpus binaries (`--layer hir`).
- Traced how often the new fallback actually fires on the real corpus
  (temporary instrumentation, removed before commit): 3991 times across 72
  binaries. Corpus-wide `git`-stash A/B (isolating only this change) showed
  **zero** difference in line count, `goto`/`if`/`while`/`return`/`break`
  counts, or total ternary-expression count in any of the 72 files -- every
  one of those 3991 firings either reproduces exactly what the existing
  scan already found, or gets filtered by the same downstream 2-predecessor/
  select-synthesis gates the scan's own results are already subject to. This
  benchmark corpus doesn't happen to contain a case where the two resolution
  paths produce a *different* final statement -- the same honest caveat as
  the phi-congruence fix: this closes a real, demonstrated gap (the `___chkstk_ms`
  false-positive it had to be hardened against was a genuine bug this
  exact code path introduced, not a hypothetical), verified directly via
  the regression test above, but its benefit on top of the existing scan is
  currently latent on this specific corpus rather than corpus-visibly
  different.
- `kv_lookup`, `list_sum`, and `___chkstk_ms` all re-verified correct after
  the final version.
