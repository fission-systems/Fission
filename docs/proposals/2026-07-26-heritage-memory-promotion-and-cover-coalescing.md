# Decompiler Change Proposal: Heritage Memory Promotion And Cover Coalescing

Date: 2026-07-26

## 1. Baseline Anchor

This is a mechanical Heritage/HighVariable architecture slice. It is not
motivated by a benchmark row and makes no decompiler-quality claim.

Current code facts:

- Every `CALL`/`CALLIND` is an unknown all-space read/write guard even when
  `NirTypeContext::call_effect_summaries` proves a narrower effect.
- Exact stack/RAM guards remain metadata; memory locations do not participate
  in phi placement or rename.
- HighVariables force phi congruence but have no explicit cover and perform no
  interference-checked adjacent COPY/CAST coalescing.
- Cspec resolves the stack-pointer register offset, but that fact is not
  retained in `NirRenderOptions` for Heritage.

## 2. Owner Proof

- [x] Cspec-derived ABI data
- [x] Shared SSA substrate
- [x] Builder Heritage construction
- [ ] Normalize
- [ ] Structuring
- [ ] Printer

Ghidra reference:

- `Heritage::guardCalls()` distinguishes unaffected, reload, unknown, and
  killed-by-call ranges before rename.
- `Heritage::guardStores()` creates INDIRECT definitions only for ranges a
  store may affect.
- `LoadGuard` uses range analysis to constrain indexed stack aliases.
- `Merge::mergeMarker()` forces phi/indirect congruence.
- `Merge::mergeAdjacent()` performs speculative same-operation merges only
  after `HighIntersectTest` proves covers do not intersect.

Fission owner:

- Cspec: `crates/fission-pcode/src/midend/cspec/apply.rs`
- Typed facts: `crates/fission-midend-core/src/ir/ssa.rs`
- Construction and validation:
  `crates/fission-pcode/src/midend/builder/scalar_ssa.rs`

## 3. Generality / Invariant Proof

Typed call-effect rule:

> Resolve a direct call target through `NirTypeContext`. If its summary proves
> `may_call_unknown == false`, derive the guard's read/write effect from
> `reads_memory` and `writes_memory`. Missing or uncertain facts remain
> read/write. A pure call remains a typed no-memory-effect barrier record but
> creates no memory SSA definition.

Memory-promotion rule:

> Track pointer ranges as either absolute RAM offsets or signed offsets from
> the cspec stack-pointer input. Promote only exact, finite LOAD/STORE byte
> ranges. Build a deterministic disjoint partition per memory region/space.
> Exact LOADs read reaching versions; exact STOREs define their pieces.
> Bounded/unknown STOREs and write-capable calls define only promoted pieces
> they may overlap. Unknown effects never invent new memory locations.

Cover/coalescing rule:

> Compute each SSA value's block/instruction cover from its definition and all
> operation/phi-edge uses. Phi congruence remains forced. COPY/CAST-connected
> equal-sized scalar classes may merge only when their aggregate covers do not
> intersect. Stack/RAM address-tied classes never merge speculatively.

The rules use typed call summaries, cspec data, SSA, CFG
dominance/reachability, and byte ranges only. They contain no ISA, function,
address, compiler, or corpus condition.

## 4. Risk And Ownership Check

- Existing owners are extended; no new output pass is introduced.
- Promoted memory uses a dedicated stack/RAM storage identity with a
  signed-capable coordinate, separate from register/unique scalar storage.
  This prevents memory-space ids from colliding with varnode-space ids and
  represents negative stack offsets without encoding tricks.
- Call summaries narrow effects only when unknown callees are explicitly ruled
  out; all absent facts fail closed.
- Memory promotion does not claim general memory SSA: heap aliases, symbolic
  globals, and unresolved stack bases remain guards only.
- Speculative coalescing is limited to COPY/CAST and rejects input,
  address-tied, size-mismatched, or cover-intersecting classes.
- No new dependency, runtime vendor access, metric, or ISA-local rule.

## 5. Validation Matrix

- [x] Cspec stack-pointer offset reaches Heritage.
- [x] Pure/read-only/write-only call summaries narrow call guards.
- [x] Unknown calls remain read/write.
- [x] Negative stack offsets promote to stack-domain SSA.
- [x] Absolute exact addresses promote to RAM-domain SSA.
- [x] Bounded/unknown stores kill only overlapping/already-promoted locations.
- [x] Write-capable calls kill promoted memory; pure/read-only calls do not.
- [x] Memory versions receive phi nodes across CFG joins.
- [x] Value and HighVariable covers are deterministic and shape-valid.
- [x] Non-interfering COPY/CAST classes coalesce.
- [x] Interfering or address-tied classes remain distinct.
- [x] Focused scalar SSA tests: 26 passed.
- [x] `cargo nextest run -p fission-midend-core`: 5 passed.
- [x] Full `cargo nextest run -p fission-pcode --no-fail-fast`: 938 passed,
  18 pre-existing failures, 1 skipped. The failure set is unchanged from the
  baseline run.
- [x] `cargo check -p fission-pcode`
- [x] `cargo check -p fission-decompiler`
- [x] `cargo check -p fission-automation`
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .`: 0 findings.

External benchmark and smoke lanes remain deferred by explicit project
direction. This proposal reports only mechanical behavior and tests.

## 6. AI / Ghidra Firewall

- Ghidra is used as a cleanroom algorithm and invariant reference only.
- No implementation is copied and no production dependency points into
  `vendor/`.
- No benchmark identity or output-style artifact enters production logic.

## 7. Review Notes

- [x] Canonical owners identified.
- [x] No printer/UI/benchmark repair.
- [x] No hardcoded function/address/binary/corpus rule.
- [x] No quality claim without measurement.
