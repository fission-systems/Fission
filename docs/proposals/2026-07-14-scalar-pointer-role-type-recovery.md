# Scalar And Pointer Role Type Recovery

## Baseline

- Corpus: `dev`
- Source family: `data_structures`
- Compiler tuple: Windows x86-64 GCC `-O0`
- Anchor A: `sum_array`, `0x140001530`
  - Current output recovers the indexed load but declares the element pointer as
    `uint *` and the return as `longlong`.
  - Semantic evidence: 4/5 cases; the negative-element case fails.
- Anchor B: `reverse_in_place`, `0x140001581`
  - The second ABI parameter is declared `uint *` although it participates in
    shifts, subtraction, loop bounds, and affine address offsets.
  - Semantic evidence: 0/5; generated C does not compile.

## Owner Proof

- Raw p-code parity for the x86-64 rows is exact against the reference. The lift
  contains the expected load widths, return-register write, unsigned loop-bound
  comparison, and affine address arithmetic.
- Builder/materialize now preserves the distinct pointer and scalar live
  intervals. The remaining wrong declarations are introduced or retained by
  normalize type/data recovery.
- `normalize/types/type_infer.rs` owns formal-parameter role classification and
  return type refinement. `normalize/memory/ptr_arith.rs` owns pointee refinement
  from access width and stride.
- The printer only renders the supplied `NirType`; it is not the owner.

## Invariant Proof

1. A formal parameter that reaches an address only through the scalar side of
   affine arithmetic, and also has strong scalar uses such as shift, subtraction,
   or an integer loop bound, must remain scalar even when register reuse produced
   an earlier pointer candidate.
2. A pointer pointee inferred only from a native-width memory access should use
   the decompiler's ordinary signed integer default unless an unsigned operation
   provides stronger evidence. Byte accesses remain unsigned by default.
3. A function returning through a narrower ABI return-register write must not be
   widened to an unrelated local's storage width after cast cleanup. The return
   declaration follows the value written to the ABI return register.

## Generality

- The parameter rule applies to any pointer-base plus affine scalar expression;
  it does not depend on a function name, address, compiler, or corpus row.
- The pointee rule covers word-sized array loads with no signedness evidence and
  preserves explicit unsigned evidence.
- Synthetic tests use neutral binding names and standalone HIR shapes.

## Risk

- Default signedness is necessarily a decompiler policy when machine code does
  not encode source signedness. Explicit zero-extension, unsigned comparisons,
  bit masks, or surface type hints must win over the default.
- Scalar demotion must fail closed when both address contributors have genuine
  address-use evidence.
- Return narrowing must not alter explicitly hinted or surface-named prototypes.

## Validation Matrix

- Targeted tests:
  - Native-width indexed load refines an unknown word pointer without overriding
    explicit unsigned evidence.
  - A shifted/subtracted affine contributor is demoted from pointer to scalar.
  - ABI-width return evidence survives local alias/cast cleanup.
- Crate gate:
  - `cargo nextest run -p fission-pcode --no-fail-fast`
  - `cargo check -p fission-pcode -p fission-decompiler`
- Focused benchmark:
  - Re-run both anchor rows with a freshly built local Fission bundle.
- Regression smoke:
  - Re-run the existing ten-row Fission semantic smoke set.

## AI Use

- No external model proposal is used for the implementation.
- Production rules contain no benchmark identifiers.
- The reference output is used only to contrast recovered facts, not as a style
  template.

## Validation Results

- `cargo nextest run -p fission-pcode --no-fail-fast`: 1,179 passed, 9 skipped.
- `cargo check -p fission-pcode -p fission-decompiler`: passed.
- Fresh local Linux bundle, dev corpus:
  - `sum_array`: 5/5 semantic cases on all GCC O0/O2 x86/x64 variants.
  - `checksum`: 5/5 semantic cases on all GCC O0/O2 x86/x64 variants.
  - `reverse_in_place`: the length parameter is unsigned and ABI-width on all
    four variants; remaining failures are return/body recovery, not parameter
    type recovery.
  - `count_bits`: 6/6 semantic cases on all four variants with no timeout.
- The focused rows were also run concurrently to exercise both loop-index and
  end-pointer lowering shapes.
