# Decompiler Change Proposal: Scalar SSA Heritage Foundation

Date: 2026-07-26

## 1. Baseline Anchor

This is an architecture/mechanical foundation, not a benchmark-driven quality
fix. Per the current project direction, external benchmark work is intentionally
deferred and no quality gain will be claimed from this change.

Code anchor:

- `NirFunction`, `NirBlock`, and `NirPhiNode` are declared in
  `fission-midend-core`, but no production path constructs them.
- `PreviewBuilder::run_incremental_heritage()` only discovers and registers
  stack partitions.
- Builder def lookup stores multiple definitions under one `VarnodeKey`, then
  resolves them through site-sensitive searches instead of a canonical SSA
  value identity.

## 2. Owner Proof

- [x] Builder/materialize
- [x] Shared IR substrate

Ghidra reference:

- `Heritage::placeMultiequals()` owns phi placement.
- `Heritage::rename()` owns SSA renaming.
- `Heritage::heritage()` links eligible free Varnodes into the data-flow graph.

Fission owner:

- Construction: `crates/fission-pcode/src/midend/builder/`
- Typed shared facts: `crates/fission-midend-core/src/ir/`

Normalize, structuring, and printer are downstream and cannot establish
per-definition value identity soundly.

## 3. Generality / Invariant Proof

Generalized rule:

> For each exact, eligible scalar storage location, every reachable operation
> output creates one SSA value; each operation input resolves to the current
> dominating value; join blocks receive phi definitions on iterated dominance
> frontiers; phi operands are keyed by real predecessor block.

- [x] No ISA, calling-convention, function, address, compiler, or corpus guard.
- [x] Register/unique address-space classification is data, not duplicated SSA
  policy.
- [x] Deterministic allocation follows block, storage-key, operation, and
  predecessor order.

Initial scope:

- Exact register/unique locations only.
- Constants and memory locations are excluded.
- Overlapping/subregister refinement, LOAD/STORE/CALL guards, delayed address
  spaces, and out-of-SSA remain later Heritage phases.

## 4. Risk And Ownership Check

- Existing owner: builder `run_incremental_heritage` and CFG facts.
- Shared substrate: typed SSA IDs, sites, values, and phi records.
- New helper is required because current def/use maps overwrite or collect
  definition sites but do not establish SSA identities or phi nodes.
- No new owner-to-owner dependency: `fission-pcode` already depends on
  `fission-midend-core`.
- No telemetry changes in this foundation.
- Existing materialization remains behaviorally unchanged: the first slice
  constructs and validates the SSA overlay but does not yet replace expression
  lowering lookups.

## 5. Validation Matrix

- [x] Diamond CFG places one phi with predecessor-complete operands.
- [x] Loop CFG places a loop-carried phi.
- [x] Straight-line redefinitions resolve each use to the current value.
- [x] Unreachable definitions do not enter reachable SSA.
- [x] Repeat construction produces byte-for-byte equal typed facts.
- [x] Validator rejects missing phi operands and non-dominating uses.
- [x] Focused test:
  `cargo nextest run -p fission-pcode scalar_ssa` — 7/7 passed.
- [ ] `cargo nextest run -p fission-pcode` — 919/937 passed, 18 failed,
  1 skipped. The failures are the current worktree's existing normalize and
  structuring regression set; none exercises the new SSA overlay.
- [x] `cargo check -p fission-midend-core`
- [x] `cargo check -p fission-pcode`
- [x] `cargo check -p fission-decompiler`
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .` — 0 findings.

External benchmark and smoke lanes: deferred by explicit current direction.
This proposal makes no decompiler-quality claim.

## 6. AI / Ghidra Firewall

- Ghidra is used only for Heritage algorithm and ownership invariants.
- No output style, sample identity, binary path, address, or compiler tuple is
  embedded in production logic.

## 7. Review Notes

- [x] No hardcoded function/address/binary/corpus guards.
- [x] No benchmark-only or printer-only edit.
- [x] The helper fills a missing canonical builder analysis rather than
  duplicating a normalize pass.
