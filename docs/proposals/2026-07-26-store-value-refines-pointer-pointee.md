# Decompiler Change Proposal: Store Value Refines Pointer Pointee

## 1. Measured anchor

- Corpus: external `/Users/sjkim1127/fission-benchmark` dev corpus
- Source: `corpus/dev/source/c/memory_layouts.c`
- Binary: `corpus/dev/binaries/c/memory_layouts_gcc_O0.exe`
- Function/address: `matrix_multiply` at `0x1400015b8`
- Fission source: `b8442d08f8f41079348188114df2cddb02c0b1c9` plus the preceding structured-block ownership repair
- Baseline artifact:
  `results/local_b8442d08_label_ownership_after_matrix_multiply_fission_ghidra.json`
- Measured status: `assertion_fail`, semantic `0.20`, 1/5 cases
- Fission signature: `int matrix_multiply(float *, float *, uint *, uint)`
- Ghidra signature: `void matrix_multiply(float *, float *, float *, int)`
- Fission store: `*xVar84 = xmm0_da`, where `xmm0_da` and the accumulator are
  `float` but `xVar84`, its aliases, and parameter 3 remain `uint *`.
- Failure pattern: the all-zero output case passes because numeric conversion of
  `0.0f` to `uint` preserves zero; all four non-zero float-output cases fail.
- Control rows that must remain green: Fission `gcc -O1`, `gcc -O2`, and
  `clang -O0` are each 5/5 in the same artifact.

## 2. Canonical owner proof

The raw store, restored CFG statements, float accumulator, and pointer alias chain
are present before rendering. The wrong C semantics are introduced by NIR type
recovery: `use_type_infer::collect_assignment_copy_constraints` upgrades an
`Index` lvalue base from the known float type of the stored value, but the
equivalent `Deref` lvalue path only constrains the value from the already-defaulted
pointee type.

Owner:
`crates/fission-midend-normalize/src/types/use_type_infer.rs`.

This is not a printer fix. Changing only the printed declaration would hide an
incorrect store type in the semantic IR.

## 3. Reference invariant

Ghidra 12.0.4 models the same bidirectional edge in
`Ghidra/Features/Decompiler/src/decompile/cpp/typeop.cc`:
`TypeOpStore::propagateType` propagates slot 2 (stored value) to slot 1 (pointer)
by constructing a pointer to the value type. `ActionInferTypes` applies that
edge only when the propagated type is more specific than the existing temporary
type.

Fission invariant:

> For a dereference store `*p = value`, a known primitive value type may refine
> an unknown or same-width default integer pointee to that value type. The
> existing monotone constraint lattice remains authoritative, and declared
> surface types remain locked.

The rule is ISA-independent and applies to the common typed store edge, not to a
function name, address, ABI register, or x86 mnemonic.

## 4. Proposed change

Extend the existing `DirLValue::Deref` branch in
`collect_assignment_copy_constraints` to emit the same float-pointer constraint
already emitted for `DirLValue::Index` when the RHS is a binding proven to be
float. Let the existing fixed-point rounds and float-pointer copy propagation
carry the refinement through `xVar84 -> param_20 -> param_3`.

No new pass, dependency, metric, or vendor linkage is added.

## 5. Validation matrix

- [x] Focused normalize unit test: float value stored through a default `uint *`
      dereference refines the base and a two-hop parameter alias chain to
      `float *`.
- [x] `cargo nextest run -p fission-midend-normalize`: 278/278 passed.
- [x] `cargo nextest run -p fission-pcode --no-fail-fast`, compared with clean
      `b8442d08` baseline rather than treated as an absolute-green requirement.
      Result: 912/930 passed with the same 18 failures as the preceding
      structuring-only build; clean `b8442d08` was 906/930 with 24 failures.
- [x] `cargo check -p fission-pcode`
- [x] Force-build the local Linux Docker bundle with source fingerprint
      `27c6e655f5c7ead143cd9910ef8035a7416f28bbab279649e1ee9b43f4bb596b`.
- [x] Rerun the exact external `matrix_multiply` Fission/Ghidra benchmark with
      caches disabled by the local runner path.
- [x] GCC `-O0` moved from 1/5 to 5/5. GCC O1/O2 and Clang O0 remained 5/5.
      Result artifact:
      `results/local_b8442d08_store_type_after_matrix_multiply_fission_ghidra.json`.

## 6. Claim boundary

The unit test proves only the mechanical type-edge invariant. The external
original-binary oracle measured a semantic improvement from 1/5 to 5/5, so the
type change satisfies the quality-claim gate for this anchored row.
