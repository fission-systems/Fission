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
- [x] `cargo nextest run -p fission-pcode`: 961 passed, 1 skipped (baseline
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
verified against the source): generic temps `xVar*`/`uVar*` (698
instances -- the classic "same generic name, two logically distinct SSA
values with overlapping live ranges" case), hardware GPRs `edi`/`ebx`/`edx`/`esi`/`ecx`/`rax`
(165), XMM lane pieces `xmm*_qa`/`xmm*_qb` (~150, overlaps with the P1
LaneDivide gap already tracked in the 2026-07-25 audit).

**Conclusion**: the P0 gap is real and measurable (235 functions, ~7% of
the dev corpus), but roughly an order of magnitude smaller than the raw
11,006 count suggests once the flag-register naming convention is excluded.
`xVar*`/`uVar*` generic-temp collisions are the largest single category and
the most direct match for the audit's "return/join noise" symptom.

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

## 8. Recommended next step

Wire a corrective pass targeting the largest, cleanest category first:
generic-temp (`xVar*`/`uVar*`) name collisions at
`merge_binding_name_for_materialized_output`'s reachability-proof
acceptance point in `materialize/mod.rs` -- reject a name-reuse candidate
whose SSA `HighVariable` differs from and interferes with the name's
existing owner, falling back to a fresh temp name (mirrors Ghidra's
`mergeTest` failure path: the merge is simply not performed, not an error).
Remeasure with this same diagnostic (should approach zero for the targeted
category) and with `fission-benchmark`'s `scripts/compare_runs.py` for
`type_match`/`semantic_score` movement on the same corpus rows.
