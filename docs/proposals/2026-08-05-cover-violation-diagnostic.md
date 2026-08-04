# Decompiler Change Proposal: Cover Violation Diagnostic

Date: 2026-08-05

## 1. Baseline Anchor

This is a measurement-only diagnostic. It changes no materialize output and
makes no decompiler-quality claim by itself. It exists to answer a factual
question raised against `docs/audits/2026-07-25-ghidra-vs-fission-unimplemented.md`'s
P0 item ("Cover 기반 HighVariable 병합... 미완성"): is that still true, and
if a real gap remains, how big is it on real corpus data?

Current code facts (2026-08-05):

- `crates/fission-midend-core/src/ir/ssa.rs` +
  `crates/fission-pcode/src/midend/builder/scalar_ssa.rs` already compute
  `SsaHighVariable` groups with an explicit `cover` (per
  `docs/proposals/2026-07-26-heritage-memory-promotion-and-cover-coalescing.md`,
  landed one day after the audit). This part of the audit's premise is
  stale.
- Zero production call sites read `NirScalarSsa.high_variables` /
  `value_high_variables` (confirmed by exhaustive grep across `crates/`).
  The data is computed and validated by its own tests but has no effect on
  rendered output.
- The actual name-merge decisions live in
  `crates/fission-pcode/src/midend/builder/materialize/mod.rs` and
  `cross_block.rs` (~3800 lines): a reachability-proof system (`prove_definition_reaches_block_entry`,
  `merge_binding_name_for_materialized_output`, `explicit_merge_bindings`),
  not the "simple name matching" the audit's phrasing suggests.

## 2. Owner Proof

- [x] Shared SSA substrate (Cover/HighVariable -- already landed, cited above)
- [x] Builder diagnostic (new, this proposal)
- [ ] Normalize
- [ ] Structuring
- [ ] Printer

Ghidra reference:

- `Merge::mergeTest` / `HighIntersectTest`: two `Varnode`s may only join one
  `HighVariable` when their `Cover`s do not intersect. This proposal ports
  the *intersection test itself* as a read-only cross-check, not the merge
  policy around it (forced vs. speculative merge categories, `mergeAdjacent`,
  `mergeMultiEntry`, etc. remain out of scope).

Fission owner:

- New: `crates/fission-pcode/src/midend/builder/materialize/cover_diagnostics.rs`
- Reads (does not write): `PreviewBuilder::materialized_vns`,
  `PreviewBuilder::explicit_merge_bindings`, `PreviewBuilder::scalar_ssa`

## 3. Generality / Invariant Proof

Cover-violation rule:

> For each rendered PreHIR binding name, resolve every SSA definition site
> (`materialized_vns`, keyed by `(varnode, def_addr, def_seq)`) and every
> block-entry merge point (`explicit_merge_bindings`, keyed by
> `(block_idx, varnode)`) that produced that name back to its
> `SsaHighVariableId` via `NirScalarSsa.operation_outputs` /
> `NirScalarSsa.phis`. If two different `SsaHighVariableId`s map to the same
> name AND their `cover` intervals intersect (same block, overlapping
> half-open `[start, end_exclusive)` range), record a violation. Same-id
> sharing and non-intersecting different-id sharing are never violations
> (both are legitimate: the same value, or two values that are never live at
> the same program point).

Conservative by construction: only whole-varnode (`byte_offset == 0`)
definitions are resolved; a partial/sub-register piece is skipped rather
than guessed at (a wrong guess here would misreport a "measured" count).

The rule uses only already-computed SSA data (Cover, HighVariable id,
operation-output/phi maps) and the builder's own existing
`materialized_vns`/`explicit_merge_bindings` maps. No ISA, function,
address, compiler, or corpus condition.

## 4. Risk And Ownership Check

- Purely additive: a new module + one `if preview_builder_diag_enabled()`
  call site at the end of `build_hir`, after
  `trace_materialize_owner_repartition_summary()`. No existing function's
  control flow, return value, or materialize output changes.
- Diagnostic is `FISSION_PREVIEW_DIAG`-gated (existing env var, existing
  convention in this file) -- zero cost and zero output change when unset
  (the default).
- No new dependency, runtime vendor access, or ISA-local rule.

## 5. Validation Matrix

- [x] Unit: intersecting covers under the same name are flagged
      (`intersecting_covers_under_same_name_are_flagged`).
- [x] Unit: non-intersecting covers under the same name are not flagged
      (`nonintersecting_covers_under_same_name_are_not_flagged`).
- [x] Unit: the same `HighVariable` under the same name is never flagged,
      regardless of cover shape (`same_high_variable_under_same_name_is_never_flagged`).
- [x] Unit: distinct names are never flagged even with intersecting covers
      (`distinct_names_are_never_flagged_even_if_covers_intersect`).
- [x] Unit: two p-code ops at the same source instruction address sharing a
      name are never flagged even with intersecting covers
      (`same_instruction_widening_chain_is_never_flagged`) -- added after
      tracing a real corpus false positive (below).
- [x] `cargo nextest run -p fission-pcode`: 962 passed, 1 skipped (baseline
      unchanged).
- [x] `cargo nextest run --workspace`: no new failures vs. established
      baseline (7 pre-existing unrelated `fission-emulator` failures).

### Real-corpus measurement (`fission-benchmark/corpus/dev`, all binaries, `--all`)

`FISSION_PREVIEW_DIAG=1 fission_cli decomp <binary> --all` across every
`.exe` in the dev corpus, ~3338 functions attempted:

| Bucket | Count |
|---|---:|
| Functions with >=1 raw violation | 452 |
| Total raw violation instances | 11,006 |
| ...of which x86 status flags (`zf`/`cf`/`of`/`sf`/`pf`) | 9,928 (90.2%) |
| **Non-flag violation instances** | **1,078** |
| **Distinct functions with >=1 non-flag violation** | **235 (~7% of scanned functions)** |

Flag registers (`zf`/`cf`/`of`/`sf`/`pf`) dominate the raw count and are a
known, separately-handled naming convention (each CMP/TEST-style op
redefines them; they are consumed immediately by the following branch, not
modeled as ordinary mergeable variables) -- excluded from the headline
number as a likely non-bug category, not asserted as definitely benign.

Top **non-flag** violation name families (all real corpus rows, examples
verified against the source): generic temps `xVar*`/`uVar*` (initially 698
instances, see revision below), hardware GPRs `edi`/`ebx`/`edx`/`esi`/`ecx`/`rax`
(165), XMM lane pieces `xmm*_qa`/`xmm*_qb`.

**Revision after tracing a concrete case.** Manually tracing
`apply_binop`'s `uVar14` collision found a second false-positive category:
a single source instruction's p-code expansion legitimately touching the
same storage at different widths (e.g. a CALL's 32-bit `EAX` result
immediately zero-extended into 64-bit `RAX`, both p-code ops carrying the
call's own address). The SSA model correctly treats these as distinct
values, but sharing a display name between them is the intended "same
value, different width view" case, not the cross-value collision this
diagnostic targets. Added a same-source-instruction-address exemption
(`scan_cover_violations`, plus regression test
`same_instruction_widening_chain_is_never_flagged`). Re-measuring after
this fix:

| Bucket | Before | After same-instruction exemption |
|---|---:|---:|
| Non-flag violation instances | 1,078 | 822 |
| Distinct functions with >=1 non-flag violation | 235 | 215 |

**Conclusion**: the P0 gap is real and measurable (215 functions, ~6% of
the dev corpus scanned), well below the raw 11,006 count once both the
flag-register convention and the same-instruction widening-chain pattern
are excluded. `xVar*`/`uVar*` generic-temp collisions remain the largest
single category (608 of 822, 74%) and the most direct match for the
audit's "return/join noise" symptom.

## 6. AI / Ghidra Firewall

- Ghidra is used as a cleanroom algorithm and invariant reference only
  (`Merge::mergeTest` / `HighIntersectTest` cited for the intersection-test
  invariant; no C++ copied).
- No production dependency points into `vendor/`.

## 7. Review Notes

- [x] Canonical owner identified (materialize; read-only diagnostic, not a
      new output pass).
- [x] No printer/UI/benchmark repair.
- [x] No hardcoded function/address/binary/corpus rule.
- [x] Quality claim scoped to what was measured (a violation *count*, not a
      type_match/semantic_score movement -- no corrective change has landed
      yet, so no such movement exists to claim).

## 8. Attempted corrective fix (reverted -- documenting for the next attempt)

Tried wiring a live guard at `ensure_temp_binding_for_output`'s and
`ensure_explicit_merge_binding_for_block`'s generic-temp
(`next_unused_temp_binding_name`) fallback: reject a candidate name already
claimed by a different, interfering `SsaHighVariable`, allocating a fresh
name instead (Ghidra's `mergeTest` failure path -- the merge is simply not
performed).

**Result: measurably no effect.** Re-running the same real-corpus scan
after the guard landed produced byte-identical counts (822 / 215,
including the same `apply_binop` `uVar14` case used to design it). Tracing
`apply_binop` directly (`raw-pcode` dump + entry/exit instrumentation on
`ensure_temp_binding_for_output`) found the second conflicting definition
(`def_addr=0x4014fd def_seq=42`, a `Copy` into `EAX` at p-code op index 42)
**never calls `ensure_temp_binding_for_output` at all** -- yet its
`(varnode, def_addr, def_seq)` key is present in `materialized_vns` mapped
to `uVar14`. There is exactly one `.insert()` call site into
`materialized_vns` in the entire crate (re-verified), and that site is
inside the very function that was never called for this key. The real
insertion mechanism for this case was not found before time ran out on
this session; it is not `next_unused_temp_binding_name` producing a
non-fresh name (that function's freshness check against the append-only
`self.temps` is airtight and was verified by direct trace) -- something
else in `materialize` is writing this key, or a different, not-yet-found
path shares state with it.

**Reverted** (`git checkout` on `state.rs`/`init.rs`/`mod.rs`, guard
methods removed from `cover_diagnostics.rs`) rather than ship inert code.
What remains landed is only the verified diagnostic and its
same-instruction-exemption refinement (Section 5).

## 9. Real insertion path found, corrective fix landed

Following the recommended next step: a proper multi-line-aware regex scan
(`materialized_vns\s*\.\s*insert\s*\(` over the whole crate, catching
`.insert(` calls split across lines that the earlier single-line grep in
Section 8 missed) found **three** insertion sites, not one --
`ensure_temp_binding_for_output` (mod.rs), `bind_materialized_output_to_fresh_temp`
(mod.rs, always-fresh, no cache check), and
`bind_materialized_output_to_existing_name` (loop_carried/mod.rs, binds an
op directly to a caller-supplied *existing* name string with **no**
freshness or collision check at all).

Tagging `bind_materialized_output_to_existing_name`'s ~9 call sites (the
big `if let / else if let` chain in `materialize/mod.rs` around line 1000
choosing which name-reuse heuristic applies) and re-tracing `apply_binop`
found the exact culprit: `same_block_prior_register_binding_name` ->
`prove_same_block_register_join`. This heuristic exists for cmov-style
same-block register redefinition chains (a default value, then a
conditional override -- legitimately one logical value). Its guard
(`output_has_consumed_interval_before_redefinition`) checks whether the
*current* (new) definition is itself used-then-redefined later in the
block, but does **not** check anything about the relationship between the
current definition and the *prior* one it's about to reuse the name of.
For `apply_binop`'s three sequential, independent lookup-table reads (each
passing through `EAX` for an unrelated purpose), the last one has no
further redefinition after it in the block, so the guard never fires, and
it silently inherits `uVar14` from an earlier, semantically unrelated
`EAX` definition.

**Fix**: added `PreviewBuilder::cover_proves_distinct_and_interfering`
(`cover_diagnostics.rs`) -- the live counterpart of `scan_cover_violations`'s
core test, resolving both definitions' `SsaHighVariableId` via the same
already-computed Cover data and returning `true` only when the SSA model
positively proves them distinct and interfering (permissive on missing
data, so it only ever blocks what it can prove, never blocks on
uncertainty). Wired into `prove_same_block_register_join`'s backward search:
skip a candidate proven to interfere and keep looking further back, exactly
mirroring `mergeTest`'s "this merge doesn't happen" (not an error).

### Validation

- [x] `cargo nextest run -p fission-pcode`: 962 passed, 1 skipped (no
      change -- including all existing `same_block_prior_register_binding_name`
      / cmov-chain tests).
- [x] `cargo nextest run --workspace`: no regression in `fission-pcode` or
      any crate depending on it; two `fission-emulator` JIT-differential
      tests timed out under concurrent load from this session's own
      Docker/benchmark runs (that crate has no dependency in the other
      direction -- `fission-pcode` does not depend on `fission-emulator` --
      and the same tests pass reliably at baseline; not attributed to this
      change).

### Real-corpus diagnostic re-measurement

| Bucket | Before this fix | After |
|---|---:|---:|
| Non-flag violation instances | 822 | **137** |
| Distinct functions with >=1 non-flag violation | 215 | **53** |
| `xVar*`/`uVar*` instances specifically | 608 | **25** |

### Real-corpus type_match / semantic_score measurement

Isolated before/after on the exact same corpus and profile (this fix only,
`fission-benchmark/scripts/compare_runs.py`):

| Metric | Before | After | Delta |
|---|---:|---:|---:|
| `type_match` | 0.4425 | 0.4456 | **+0.0031** |
| `semantic_score` | 0.5044 | 0.5131 | **+0.0087** |
| `ok` rows | 182 | 185 | **+3** |

Row-level: 5 fixed (`accumulate_pairs`/`find_pair_value`/`kv_lookup`
gcc-m32 -O0 `runtime_error`->`ok`; `reverse_string` gcc-m32 -O0 and
`sum_array` gcc -O2 `assertion_fail`->`ok`), 2 regressed (`clamp` gcc-m32
-O2, `power` clang -O2, both `ok`->`assertion_fail`). Net positive but not
side-effect-free -- reported in full rather than only the favorable
direction.

**Bottom line**: the P0 gap ("Cover 기반 HighVariable 병합... 미완성") is
now genuinely wired from analysis into a live decompiler decision, with a
real (if modest) measured improvement and an honest accounting of the two
regressions it introduced. The `edi`/`ebx`/`rax`-family hardware-register
violations (44/2/7 of the remaining 137) and the `st*`/x87 family (36) were
deliberately left untouched -- they route through different heuristics
(`full_width_primary_return_surface_name`, `primary_return_live_out_name`,
cross-block `merge_lhs_name`) not covered by this fix, and are the natural
next slice.

## 10. Second pass: generalizing the guard to the remaining name-reuse branches

`materialize/mod.rs`'s output-binding chain has 8 branches that call
`bind_materialized_output_to_existing_name` with a name computed by their
own reachability proof (`loop_carried_lhs_name`, `direct_successor_merge_lhs_name`,
`merge_lhs_name`, `live_register_lhs_name_for_partial_gpr_join_family`,
`live_register_lhs_name_for_passthrough_join_store_producer`,
`live_register_lhs_name_for_safe_missing_merge`,
`full_width_primary_return_surface_name`, `same_block_prior_register_binding_name`
(already guarded), `primary_return_live_out_name`). Section 9 only guarded
one. Attribution tracing (a temporary `[BIND_BRANCH]` eprintln tagging each
branch plus its resolved `SsaHighVariableId`, removed after use) against the
real corpus's remaining `edi` violation (`__pei386_runtime_relocator`,
`control_flow_gcc-m32_O0.exe`) traced the true mechanism precisely: one
`edi` definition (`HighVariable(546)`) got the raw name via
`ensure_temp_binding_for_output`'s already-existing "reuse the hardware
register name if `!self.temps.contains_key(&candidate)`" fallback (line
~703 in `builder/mod.rs`, pre-existing, not part of this session's earlier
fix) -- purely an accident of block-visitation order, since it ran *before*
any of the other branches touched `edi`. A second, unrelated `edi`
definition (`HighVariable(146)`, a genuine loop-carried induction value)
then claimed the same raw name later via `loop_carried_lhs_name` and
`live_register_lhs_name_for_safe_missing_merge`, with neither branch ever
checking whether `edi` was already spoken for by an interfering value.

**First attempt (reverted)**: gated `ensure_temp_binding_for_output`'s raw-
name fallback with a whole-function-scoped
`storage_key_is_cover_ambiguous` check (any 2+ interfering `SsaHighVariable`s
ever sharing this storage key anywhere in the function -> refuse the raw
name for anyone). This fixed the `edi` case but broke
`loop_carried_stack_param_seed_preferred_over_anonymous_merge_temp`
(caught by `cargo nextest run -p fission-pcode`, confirmed as a real
regression by re-running on the pre-session baseline commit). Root cause:
this check is too coarse. A legitimate `Copy ecx <- eax` inside a loop body
speculatively unions `ecx`'s SSA value into `eax`'s (much larger,
loop-spanning) `HighVariable` per `build_out_of_ssa_facts`'s own Copy-chain
merge rule -- correct, since after that copy `ecx` and `eax` really do hold
the same value. But `ecx`'s storage key is *also* independently touched by
an unrelated same-block redefinition (`ecx = ecx & 1`) that never actually
asks for the raw `ecx` name (it gets a normal synthetic name via a totally
different path). The whole-function "any interfering pair anywhere"
check has no way to know the second value was never going to collide in
practice, and blocked a safe case. Reverted in favor of an order-dependent,
already-happened-collisions-only check, matching Section 9's guard
philosophy exactly rather than trying to prove safety in the abstract.

**Real fix**: `PreviewBuilder::cover_proves_existing_name_claim_interferes`
(`cover_diagnostics.rs`) -- given a candidate name, scans
`materialized_vns`/`explicit_merge_bindings` (the same sources
`scan_cover_violations` reads) for any *already-bound* entry under that
exact name whose `SsaHighVariableId` is different from and interferes with
the value currently being bound. Only ever blocks a collision that has
concretely already happened, never speculates about bindings not yet made
-- the same permissive-on-uncertainty contract as
`cover_proves_distinct_and_interfering`. Wired as a `.filter(...)` gate on
each of the 7 remaining name-reuse branches (all except
`same_block_prior_register_binding_name`, already covered by Section 9's
narrower same-block check): when a branch's candidate name is already
claimed by an interfering value, that branch is treated as if it returned
`None` and the chain falls through to the next candidate, ultimately to
`ensure_temp_binding_for_output`'s fresh-name fallback.

### Validation

- [x] `cargo nextest run -p fission-pcode`: 962 passed, 1 skipped -- including
      the previously-broken `loop_carried_stack_param_seed_preferred_over_anonymous_merge_temp`.
- [x] `cargo nextest run --workspace` (excluding the known-slow
      `selfjit_matches_cranelift` differential tests): 938 passed, 7 failed
      -- all 7 failures are the exact same pre-existing, unrelated
      `fission-emulator` tests already established as baseline earlier this
      session (`diag_alloc_meta`/`diag_livelock`/`diag_expand_stall`/
      `srd_semantic_replay`/`static_crt_profile` x2), not caused by this
      change.
- [x] Concrete repro (`__pei386_runtime_relocator`,
      `control_flow_gcc-m32_O0.exe`, addr `0x401bb0`): `edi` violation gone.
- [x] All temporary attribution tracing (`[BIND_BRANCH]`,
      `debug_high_variable_id`, `FISSION_AMBIG_TRACE`) removed after use;
      only the durable guard remains.

### Real-corpus diagnostic re-measurement (dev + adversarial corpus, 48+1
binaries, ~452 functions)

| Bucket | After Section 9 | After this pass |
|---|---:|---:|
| Non-flag violation instances | 137 | **7** |
| Distinct functions with >=1 non-flag violation | 53 | **6** |

Remaining 7: `xmm0_wh` (2), `rax` (2), `xVar6`/`uVar70`/`rsi` (1 each) --
scattered, no longer dominated by one branch/register family. The
`edi`/`esi`/`ecx`/`eax`/`ebx`/`r8`/`r9`/`edx`/`st0`-`st6` clusters that made
up the bulk of the 137 are gone.
