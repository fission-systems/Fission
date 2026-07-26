# Decompiler Change Proposal: Ghidra-Style Type Flow And Block Ownership

Date: 2026-07-26

## 1. Measured anchors

Primary row:

- External corpus: `/Users/sjkim1127/fission-benchmark`, dev
- Function: `matrix_multiply`
- Binary/address: `memory_layouts_gcc_O0.exe`, `0x1400015b8`
- Initial Fission result: `compile_error`, 0/5 semantic cases
- After structured-label ownership repair: `assertion_fail`, 1/5
- After store-value pointer refinement: 5/5
- Artifacts:
  - `results/local_b8442d08_matrix_multiply_fission_ghidra.json`
  - `results/local_b8442d08_label_ownership_after_matrix_multiply_fission_ghidra.json`
  - `results/local_b8442d08_store_type_after_matrix_multiply_fission_ghidra.json`

Regression surface:

- Clean `b8442d08` pcode baseline: 906/930 passed, 24 failed.
- Current two-repair state: 912/930 passed, 18 failed.
- Remaining failures are concentrated in structured conditionals, loops, switches,
  plus independent return/def-use cases.
- Existing all-variant 5/5 controls: GCC O1, GCC O2, Clang O0.

## 2. Owner proof

Type recovery currently has three overlapping fixed-point implementations:

- `types/type_infer.rs`: first-definition / near-SSA forward inference.
- `types/use_type_infer.rs`: syntax-directed backward constraints and several
  primitive-specific upgrades.
- `types/constraint.rs`: a second assignment/memory fixed-point solver.

The measured store failure required a `float`-specific branch because there is
no canonical operation-edge transfer engine or type-evidence ordering.

Structuring currently carries the same ownership fact through several parallel
representations:

- `RegionProof.members`
- `[start, skip_to)` ranges
- `tombstoned` block indices
- labels found recursively in structured statement bodies

The measured missing-store failure occurred when label surface was incorrectly
treated as block-body ownership.

Canonical owners:

- Type flow: `crates/fission-midend-normalize/src/types/`
- Structure ownership: `crates/fission-midend-structuring/src/graph.rs` and
  `sese_driver.rs`

No printer, UI, adapter, or benchmark change can correctly own either fix.

## 3. Ghidra reference invariants

Clean-room references:

- `TypeOp::propagateType`: each p-code operation owns its input/output transfer.
- `ActionInferTypes::propagateTypeEdge`: update only when the candidate is more
  specific, honor locks/stops, and enqueue newly strengthened nodes.
- `TypeOpLoad` / `TypeOpStore`: propagate pointee/value types in both directions.
- `CollapseStructure`: collapse actual `BlockGraph` nodes; a block belongs to a
  structure because the graph owns it, never because a printed label exists.

Fission invariants:

1. Type propagation is a monotone worklist over typed operation edges.
2. `COPY`, `LOAD`, `STORE`, `CAST`, and `INDEX` use shared transfer rules.
3. A surface-locked type cannot be overwritten.
4. A storage-width integer is weaker than same-width semantic value evidence.
5. Each final CFG leaf block is claimed by at most one top-level
   `StructureNode`.
6. Label emission and CFG block ownership are independent typed contracts.

These rules contain no function, address, compiler, or ISA guard.

## 4. Scoped implementation

### Phase A: type flow foundation

- Add an owner-local `TypeFlowSolver` with:
  - binding nodes,
  - typed operation edges,
  - evidence strength,
  - deterministic work queue,
  - monotone refinement.
- Extract `COPY`, `LOAD`, `STORE`, `CAST`, and `INDEX` edges from DIR.
- Invoke it from the existing use-driven owner; do not add another pipeline
  stage.
- Remove the measured `float`-only dereference/index/copy propagation branches
  once the generic solver covers them.
- Retain legacy comparison/arithmetic/call rules until separately migrated and
  measured.

### Phase B: typed structure ownership

- Add a `BlockOwnership` value to `StructureNode`.
- Derive structured ownership from `RegionProof.members`.
- Make `StructureGraph` reject duplicate claims deterministically.
- Use ownership membership, not recursively collected labels, to decide whether
  a residual block body was consumed.
- Keep label cleanup only for valid C label-definition deduplication.

No vendor dependency, C++ binding, copied implementation, new metric, or
parallel program-metadata map is added.

## 5. Validation matrix

- [x] TypeFlow unit tests:
  - generic same-width semantic store refines a pointer through aliases;
  - load propagates pointer pointee to its value;
  - locked surface types do not change;
  - worklist converges across chains longer than the old four-round cap;
  - multi-definition DIR bindings stop unsafe backward COPY propagation.
- [x] Structure ownership unit tests:
  - region members become node ownership;
  - duplicate block claims are rejected;
  - duplicate labels do not consume residual statements.
- [x] `cargo nextest run -p fission-midend-normalize`
- [x] `cargo nextest run -p fission-midend-structuring`
- [x] `cargo nextest run -p fission-pcode --no-fail-fast`
      compared with both 906/930 and 912/930 baselines.
- [x] `cargo check -p fission-pcode`
- [x] `cargo check -p fission-decompiler`
- [x] Release CLI build.
- [x] Forced Linux bundle bake with a new source fingerprint.
- [x] External `matrix_multiply` rerun with Fission and Ghidra comparator:
  Fission GCC O0/O1/O2 and Clang O0 remain 5/5; comparator failures are
  recorded separately.
- [x] Isolated external parity smoke with no canonical-result publication.

## 6. Measured result

- TypeFlow: 283/283 tests passed.
- Structure ownership: 110/110 tests passed.
- Pcode integration: 912/930 passed, 18 known failures, exactly matching the
  pre-refactor two-repair failure set.
- Checks passed for `fission-pcode`, `fission-decompiler`, and
  `fission-automation`; release CLI build passed.
- Final Linux source fingerprint:
  `3d3212305f9c35aa084ced501c91caaa453cc713c7c2e965ef939c04e0364b87`.
- Final Fission `matrix_multiply` controls:
  - GCC O0: 5/5
  - GCC O1: 5/5
  - GCC O2: 5/5
  - Clang O0: 5/5
- The first architectural bake exposed a real O1/O2 regression: because DIR is
  not SSA, unconditional bidirectional COPY propagation allowed a later scalar
  definition of a reused binding to flow backward into pointer parameters. The
  final implementation permits bidirectional COPY equality only for
  single-definition bindings, matching Ghidra's per-varnode boundary. A focused
  regression test and the external O1/O2 rerun both verify the correction.
- Final candidate artifacts:
  - `/Users/sjkim1127/fission-benchmark/results/local_b8442d08_typeflow_ownership_fixed_matrix_multiply_fission.json`
  - `/Users/sjkim1127/fission-benchmark/results/local_b8442d08_typeflow_ownership_fixed_matrix_multiply_ghidra.json`
- Ghidra's GCC O0 comparison row reproducibly reports `runtime_error` in the
  current external harness; the same final Fission row is 5/5. The other Ghidra
  variants in this focused run are 5/5.
- Isolated parity smoke: 15/15 match across assembly, p-code, CFG, function
  discovery, and IR-invariant stages.

## 7. Claim boundary

The architectural refactor is not itself a quality claim. Quality is preserved
only if the anchored external rows remain green and is improved only if a
previously failing measured row moves without regression.
