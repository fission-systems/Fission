# Decompiler Change Proposal: Heritage Overlap Refinement And Space Guards

Date: 2026-07-26

## 1. Baseline Anchor

This is the second mechanical Heritage foundation slice. It is not motivated by
a benchmark row and makes no decompiler-quality claim.

Current code facts:

- `NirScalarSsa` treats `(space_id, offset, size)` as an exact, independent
  storage identity.
- A 4-byte write and an overlapping 8-byte read therefore have unrelated SSA
  stacks even when they refer to the same register bytes.
- Phi placement seeds only operation-definition blocks. A storage updated on
  one diamond arm can reach the join without a phi against the entry value.
- Eligibility is an inline register/unique predicate rather than a typed,
  inspectable address-space guard.

## 2. Owner Proof

- [x] Shared SSA substrate
- [x] Builder Heritage construction
- [ ] Normalize
- [ ] Structuring
- [ ] Printer

Ghidra reference:

- `TaskList` constructs a disjoint cover from overlapping address ranges.
- `Heritage::buildRefinement()` records every read/write/input boundary.
- `Heritage::refinement()` splits reads, writes, and inputs to the common
  partition before phi placement and rename.
- `HeritageInfo` admits only address spaces marked for Heritage.
- `processJoins()` removes join-space values before Heritage; LOAD/STORE/CALL
  memory effects are guarded separately.

Fission owner:

- Typed facts: `crates/fission-midend-core/src/ir/ssa.rs`
- Construction and validation:
  `crates/fission-pcode/src/midend/builder/scalar_ssa.rs`

## 3. Generality / Invariant Proof

Generalized rule:

> For every eligible address space, collect all observed access start/end
> boundaries and form a deterministic disjoint byte partition. Every scalar
> read and write maps to the ordered partition pieces covering its physical
> range. SSA stacks and phi nodes are keyed by partition piece, so a partial
> write updates only overlapping pieces and preserves the reaching values of
> untouched pieces.

Address-space guard:

- Register and unique spaces are admitted.
- Constant, RAM, join, IOP/fspec-like, zero-sized, and overflowing ranges are
  excluded until their required alias/effect model exists.
- The rule uses address-space classification and byte ranges only; it has no
  ISA, function, address, compiler, or corpus guard.

Entry-definition rule:

> Each partition has a formal input definition at entry. Phi placement includes
> that entry definition, so a one-arm update merges with the incoming value.

## 4. Risk And Ownership Check

- Existing owner extended: scalar SSA Heritage builder.
- `operation_inputs` and `operation_outputs` become ordered access-piece lists
  because one P-code varnode can cover multiple SSA partitions.
- Physical byte order is recorded as an offset from the original access.
  Value-significance/endianness reconstruction remains a later expression
  lowering concern.
- Dynamic LOAD/STORE/CALL memory guards are not guessed in this slice. Memory
  spaces remain excluded, which is the conservative sound boundary.
- No new dependency, pass, metric, runtime vendor access, or ISA-specific rule.

## 5. Validation Matrix

- [x] Wide definition followed by low subregister definition preserves the
  original high piece and replaces only the low piece.
- [x] Wide read after the partial write resolves to both reaching pieces.
- [x] A one-arm partial definition places a join phi against the formal input.
- [x] Shifted overlapping ranges produce a complete, ordered disjoint cover.
- [x] Constant, memory, zero-sized, and overflowing ranges are excluded.
- [x] Repeat construction remains deterministic.
- [x] Validator rejects reordered/non-contiguous access pieces.
- [x] Focused scalar SSA tests:
  `cargo nextest run -p fission-pcode scalar_ssa` — 13/13 passed.
- [x] `cargo nextest run -p fission-midend-core` — 5/5 passed.
- [ ] Full `cargo nextest run -p fission-pcode --no-fail-fast` — 925/943
  passed, 18 failed, 1 skipped. The 18 failures are identical by name to the
  pre-change 919/937 result; all five new overlap/guard tests passed.
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
