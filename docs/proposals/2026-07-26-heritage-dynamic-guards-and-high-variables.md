# Decompiler Change Proposal: Heritage Dynamic Guards And High Variables

Date: 2026-07-26

## 1. Baseline Anchor

This is a mechanical Heritage/out-of-SSA foundation slice. It is not motivated
by a benchmark row and makes no decompiler-quality claim.

Current code facts:

- Scalar SSA has deterministic register/unique partitions, values, uses, defs,
  and phi nodes.
- `LOAD`, `STORE`, `CALL`, and `CALLIND` have no typed indirect-effect record.
- Phi congruence is not exported as an edge-copy plan or HighVariable identity.
- A later coalescer therefore cannot distinguish an unguarded live range from a
  value that crosses a dynamic memory/call effect.

## 2. Owner Proof

- [x] Shared SSA substrate
- [x] Builder Heritage construction
- [ ] Normalize
- [ ] Structuring
- [ ] Printer

Ghidra reference:

- `LoadGuard` records the operation, target space, conservative range, step,
  and analysis precision for indexed stack `LOAD`/`STORE`.
- `Heritage::analyzeNewLoadGuards()` refines those ranges with value-set
  analysis after register Heritage.
- `StackAffectingOps` treats calls and guarded stores as indirect-effect
  barriers during HighVariable cover intersection.
- `Merge::mergeMarker()` forces MULTIEQUAL/INDIRECT congruence, while other
  merges require non-intersecting covers.

Fission owner:

- Typed facts: `crates/fission-midend-core/src/ir/ssa.rs`
- Construction and validation:
  `crates/fission-pcode/src/midend/builder/scalar_ssa.rs`

## 3. Generality / Invariant Proof

Dynamic guard rule:

> Every reachable `LOAD` and `STORE` receives a deterministic guard. Resolve
> its pointer through scalar SSA for constant, COPY/CAST/ZEXT, affine
> add/subtract, and finite phi alternatives. Emit an exact or bounded
> half-open byte range when proven; otherwise emit an unknown range. Every
> reachable `CALL`/`CALLIND` receives an unknown all-space read/write barrier
> until a typed call-effect model can prove a smaller effect.

Out-of-SSA rule:

> Each phi operand creates one parallel-copy requirement on its incoming CFG
> edge. Phi-connected SSA values form a deterministic HighVariable congruence
> class. All other values remain singleton classes. A class records dynamic
> guard sites crossed by any member's live range so later speculative
> coalescing can fail closed.

The rules use opcode semantics, SSA definitions, CFG dominance/reachability,
and byte ranges only. They contain no ISA, function, address, compiler, or
corpus condition.

## 4. Risk And Ownership Check

- Existing owner extended: scalar SSA Heritage builder.
- Guard ranges are metadata and do not promote RAM/stack varnodes into scalar
  SSA.
- Unknown pointers and calls remain maximally conservative.
- `CALLOTHER` is not assigned a general memory barrier because Ghidra treats
  user operations as affecting only an explicit output unless another effect
  model says otherwise.
- HighVariable recovery initially performs forced phi congruence only.
  Speculative COPY/type/address-tied merging remains deferred until complete
  cover/interference and stack-object facts exist.
- The edge-copy plan remains parallel; sequential cycle breaking is deferred to
  the first consumer that actually destroys SSA.
- No new dependency, output pass, telemetry schema, or runtime vendor access.

## 5. Validation Matrix

- [x] Constant and affine pointers produce exact guards.
- [x] Finite phi pointer alternatives produce a bounded guard.
- [x] Unresolved pointers produce unknown guards.
- [x] Calls produce unknown all-space read/write barriers.
- [x] Phi operands produce deterministic edge-copy records.
- [x] Phi-connected values share a HighVariable; unrelated values do not.
- [x] A value live across a dynamic effect records that guard barrier.
- [x] Shape and builder validators reject malformed guard/high-variable facts.
- [x] Focused scalar SSA tests:
  `cargo nextest run -p fission-pcode scalar_ssa` — 18/18 passed.
- [x] `cargo nextest run -p fission-midend-core` — 5/5 passed.
- [ ] Full `cargo nextest run -p fission-pcode --no-fail-fast` — 930/948
  passed, 18 failed, 1 skipped. The failures are identical by name to the
  pre-change 925/943 result; all five new guard/HighVariable tests passed.
- [x] `cargo check -p fission-pcode`
- [x] `cargo check -p fission-decompiler`
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .` — 0 findings.

External benchmark and smoke lanes remain deferred by explicit project
direction. This proposal reports only mechanical behavior and tests.

## 6. AI / Ghidra Firewall

- Ghidra is used as a cleanroom algorithm and invariant reference only.
- No implementation is copied and no production dependency points into
  `vendor/`.
- No benchmark identity or output-style artifact enters production logic.

## 7. Review Notes

- [x] Canonical owner identified.
- [x] No printer/UI/benchmark repair.
- [x] No hardcoded function/address/binary/corpus rule.
- [x] No quality claim without measurement.
