# Decompiler Change Proposal: Interference-Checked Phi Congruence in Scalar SSA

Date: 2026-08-06

## 1. Context

Following up on this session's `kv_lookup`/`find_pair_value` fixes (rotated-
loop latch orphaning, guard-clause fallthrough deletion), the question came
up: what else could Fission's decompiler borrow from angr's decompiler?
angr's `dephication/graph_vvar_mapping.py` implements Sreedhar et al.'s
"Translating Out of Static Single Assignment Form" -- real interference-
checked phi coalescing, as opposed to naively forcing every phi output and
its operands into one variable regardless of whether they can genuinely
coexist. This looked directly relevant to the class of bug this session's
earlier fixes were chasing by hand at the PreHIR layer (`materialize`/
`cross_block.rs`'s ad hoc merge-binding synthesis).

Research turned up something better than expected: Fission already has a
real, from-scratch Cytron-style SSA construction pass with genuine phi
nodes, stable value identities, and a Ghidra-`Cover`-style live-range/
interference model --
`crates/fission-pcode/src/midend/builder/scalar_ssa.rs` (`NirScalarSsa`,
`NirPhiNode`, `SsaCoverBlock`, `groups_interfere`). It just wasn't doing the
interference check for phi congruence specifically: `build_out_of_ssa_facts`
force-unioned a phi's output with every one of its operands unconditionally,
the same naive behavior Sreedhar's algorithm exists to improve on. This was
also a real, live latent bug, not just a missed optimization: `high_variables`
already feeds `cover_diagnostics.rs`'s `cover_proves_*` functions, which are
already wired into the exact naming-decision chain in `materialize/mod.rs`
(`loop_carried_lhs_name`, same-block register-redefinition reuse, live-
register block-entry reuse) that this session's `list_sum`/`kv_lookup` fixes
touched by hand. Those guards all short-circuit on `high_a != high_b` before
checking interference -- if two genuinely-interfering values were wrongly
forced into *one* `SsaHighVariableId` by the old unconditional union, the
guard could never even see them as different, and would silently let an
unsafe name reuse through.

## 2. The fix

`build_out_of_ssa_facts` (`scalar_ssa.rs`) now runs the value-cover
computation *before* processing phis (previously it ran after, since the old
code didn't need covers for phi congruence at all), then coalesces each
phi's output with its operands one at a time via a new
`coalesce_phi_congruence_class`, using the existing `find_root`/
`groups_interfere`/`union_values` primitives -- the same machinery already
used for the adjacent-Copy/Cast speculative-merge step just below it.

This is a deliberately simplified, not globally-optimal, port of Sreedhar's
algorithm. The original mints a fresh vvar for whichever side of an
interfering pair needs a copy, then updates that fresh value's (narrower)
liveness incrementally so later candidates in the same phi can still
coalesce with it -- angr's AIL vvars are mutated in place as part of a
larger IR-rewriting pass. Fission's SSA facts are computed as one static
batch, and `SsaOutOfSsaCopy` already carries a distinct, pre-existing
`SsaValueId` for every phi operand -- there's no copy destination left to
mint. So instead of choosing *which side* of an interfering pair to isolate,
`coalesce_phi_congruence_class` always keeps the phi output as the anchor
and declines to coalesce anything that would interfere with the group as
currently formed. Because each candidate is checked against the *merged*
cover of everything already folded into the anchor, an operand that only
conflicts with an *earlier* operand (not with the output directly) is still
correctly excluded -- interference is transitive through the group, not
just pairwise against the original anchor. This can occasionally leave one
more member uncoalesced than an optimal vertex-cover choice would (a missed
cosmetic optimization), but it is never unsound: it only ever declines a
merge that could have been proven safe, never forces one that isn't.

`SsaOutOfSsaCopy`'s own shape/contract is untouched -- it still lists one
parallel-copy requirement per phi operand unconditionally, exactly as
before. Only `SsaHighVariable`/`value_high_variables` (the congruence
classes) change: they're now interference-aware instead of forced. This
keeps the change minimal and avoids touching `SsaOutOfSsaCopy`'s existing
consumers/tests.

## 3. What this does *not* do (scope)

This fixes the *fact layer*. It does not change `materialize`/
`cross_block.rs`'s own merge-binding synthesis to directly consult
`scalar_ssa.phis` as ground truth instead of its own local predecessor-op
scan -- that would be a much larger, higher-risk architectural change
(replacing a heuristic that's exclusively-2-predecessor-scoped and has no
stable cross-merge-point identity, with one driven by real dominance-
frontier-based phi placement that generalizes to any predecessor count).
Given how delicate `cross_block.rs` proved to be earlier this session (the
rotated-loop-latch and guard-clause bugs), that rewrite is flagged as real,
valuable future work, not attempted here.

What *is* already wired, with no additional code needed, is
`cover_diagnostics.rs`'s three "live counterpart" gates
(`cover_proves_distinct_and_interfering`, `cover_proves_existing_name_claim_interferes`,
`cover_proves_block_entry_reuse_unsafe`), called directly from
`materialize/mod.rs`'s real naming-decision chain. Those already consult
`self.scalar_ssa.value_high_variables`/`.high_variables` before this fix --
they were just fed an over-forced-congruent model that could hide real
interference behind a false "same high variable" identity. This fix closes
that gap for free, for every call site that already exists.

## 4. Verification

- New tests in `scalar_ssa.rs`:
  - `phi_congruence_coalesces_noninterfering_operands`: reuses the existing
    `loop_header_phi_receives_entry_and_latch_values` CFG shape and asserts
    the phi output and both its operands land in the same
    `SsaHighVariableId` -- confirms the common (non-interfering) case still
    coalesces for free, unchanged from before.
  - `coalesce_phi_congruence_class_declines_interfering_operand`: a direct
    unit test of the new helper against hand-built `SsaCoverBlock` data (a
    genuinely interfering phi output/operand pair can't be constructed
    through hand-written p-code within one block -- SSA renaming makes a
    later-defined operand value on the same storage always start after any
    earlier use of the output ends, so they can never overlap that way;
    real interference needs a multi-variable "lost copy"/swap-problem
    shape that's impractical to hand-assemble byte-exact). Confirms the
    non-interfering operand coalesces and the interfering one is correctly
    kept separate.
  - All 27 pre-existing `scalar_ssa` tests still pass unmodified, including
    `scalar_ssa_is_deterministic` and the `validator_rejects_*` shape checks
    -- confirms this doesn't need any change to `validate_scalar_ssa`'s
    consistency-checking contract (no new `SsaValueId`s are minted, so
    nothing there needed updating).
- `cargo nextest run -p fission-pcode -p fission-midend-structuring -p fission-midend-normalize -p fission-midend-core -p fission-midend-prehir`:
  1389/1389 passed.
- `cargo nextest run --workspace`: same result as this session's established
  baseline -- every failure/timeout is confined to `fission-emulator`
  (JIT/SLEIGH-decode/self-JIT differential tests, unrelated to the
  decompiler's SSA layer), and is the same fixed set of tests seen
  throughout this session, not any new failure.
- No panics across all 72 dev-corpus binaries (`--layer hir`, every compiler/
  optimization-level combination).
- Corpus-wide `git`-stash A/B, isolating *only* this change (all of this
  session's other fixes already committed on both sides): every one of the
  72 files differs, but every diff is a pure cosmetic pointer-name-pool
  reshuffle (`ptr`/`p`/`addr`/`ptrN` swapping which specific variable gets
  which name) -- line counts, `goto`/`if`/`while`/`return`/`break` counts,
  and declaration counts are byte-identical before/after in every sampled
  and fully-checked file. This is expected and benign: reordering which
  union happens when a union-find root shifts `SsaHighVariableId` numbering,
  which cascades into the semantic-naming pass's pool-assignment order --
  not a semantic change. **No file in this corpus shows a declaration-count
  increase**, meaning this specific benchmark corpus doesn't happen to
  contain a function whose phi actually exercises genuine interference (the
  classic trigger is a multi-variable "lost copy"/register-swap pattern,
  which is rare in straightforward small C programs without heavy register
  pressure). The fix is verified correct via the direct unit test above,
  not via an observed corpus split -- this is an honest, proactive
  correctness closure for a latent gap, not a fix demonstrably changing this
  particular corpus's output today.
