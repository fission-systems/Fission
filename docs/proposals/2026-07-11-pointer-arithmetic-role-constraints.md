# Decompiler Change Proposal: Stabilize pointer arithmetic roles

## 1. Baseline Row Anchor

- Binary: local dev-corpus 64-bit optimized control-flow sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Current output summary: the buffer parameter and length parameter are both
  rendered as byte pointers; callers cannot pass an integer length.
- Semantic cases passed / total: 0 / 5
- Failure category: compile error

## 2. Owner Proof

- [x] Type/data recovery: pointer arithmetic constraints

Evidence:

```text
Strong base evidence: a parameter contributes to an address that is loaded.
Offset evidence: the peer parameter is added to that base and has no address use.
Current result: both parameters can converge to pointer types.
```

The first wrong fact is a type constraint. Builder, structuring, and printer
preserve the HIR they receive.

## 3. Generality / Invariant Proof

```text
For an Add that contributes to a pointer result, an operand with independent
load/store address evidence is a pointer-base candidate. A peer operand that
resolves to a formal parameter and has no address evidence is an integer-offset
candidate. Pointer constraints must not be propagated to that offset candidate.
```

- [x] Depends on def-use, address use, and pointer arithmetic semantics only.
- [x] No function, address, binary, compiler, ABI-register, or ISA condition.
- Synthetic coverage includes direct parameters and local aliases.

## 4. Risk And Ownership Check

- Existing owner: type inference/use-constraint fixed point.
- Shared analysis candidate: pointer base/offset role evidence.
- New pass/helper: no new pipeline pass; consolidate evidence in the existing
  type owner and remove late correction when the positive constraint is stable.
- Risk: ambiguous integerized pointers. Fail closed unless one operand has
  independent address evidence.
- New owner dependency: none.

## 5. Validation Matrix

- [x] Direct pointer-base plus scalar-offset synthetic test.
- [x] Alias-based pointer-base plus scalar-offset synthetic test.
- [x] Type fixed-point convergence test: stable after the second round instead
  of reaching the six-round cap.
- [x] Existing type inference tests.
- [x] `cargo check -p fission-pcode`.
- [x] Full crate test baseline comparison: 1,156 passed, 5 known failures,
  9 skipped; no new failure.
- [x] Focused local Docker semantic verification: both optimized variants pass
  5 / 5; their length parameters are rendered as integers.

## 6. AI Review / Prompt Firewall

- Row identity is retained only in the required baseline anchor.
- Production conditions use structural HIR and type evidence only.
- No reference-output style is copied.

## 7. Review Notes

- [x] No hardcoded row identity in production code.
- [x] Extend the existing type owner; do not add a corpus-shaped pass.
- [x] Semantic claim requires compile and execution verification.
