# Decompiler Change Proposal: Restore scalar locals from exact use constraints

## 1. Baseline Row Anchor

- Binary: local dev-corpus unoptimized scalar-loop sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Corpus row or benchmark command: focused local Fission run with `--no-publish`
- Current output summary: a scalar accumulator is declared as a byte pointer;
  renderer-inserted casts obscure its scalar reads, writes, and return.
- Semantic cases passed / total: 5 / 5 for all four compiler variants
- Failure category: none; type-role defect

## 2. Owner Proof

- [x] Type/data recovery: use-driven type inference

Evidence:

```text
After storage-role splitting:
  transitive_address(acc) = false
  address_use(acc) = false
  use constraint from Return(acc) = Exact(Int64)
  binding type after fixed point = Ptr(Byte)
```

The use-driven pass computes the exact scalar constraint but its monotone merge
does not weaken an existing pointer. Scalar-only restoration currently consults
expression roles but not the constraints already computed by the same owner.

## 3. Generality / Invariant Proof

```text
A non-parameter pointer-typed local with an exact scalar use constraint may be
restored to scalar storage when it has no address use, pointer-compare peer, or
transitive path to a pointer root. Renderer-inserted casts are not HIR evidence.
```

- [x] No function name, address, compiler, register name, or ISA condition.
- [x] Synthetic test uses a neutral stack accumulator and exact scalar return.
- [x] Existing address, compare-peer, and transitive-root guards remain intact.

## 4. Risk And Ownership Check

- Existing owner: `normalize/types/use_type_infer.rs`.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
- Reuse the pass's existing constraint map; do not add a pass or helper owner.
- Risk: demoting a true pointer under an incorrect scalar signature. Retain all
  address and pointer-flow guards, and restrict this evidence to exact scalar
  constraints on non-parameter locals without explicit surface types.
- New owner dependency:
  - [x] None.
- Known cases that must not change: pointer loads/stores, pointer arithmetic,
  pointer comparison peers, transitive pointer contributors, and parameters.

## 5. Validation Matrix

- [x] Targeted exact-constraint and existing negative-control tests.
- [x] `cargo check -p fission-pcode`.
- [x] `cargo nextest run -p fission-pcode --no-fail-fast` with no new failures
  (1163 passed, 5 known baseline failures, 9 skipped).
- [x] Focused local benchmark remains 5 / 5 for all variants and renders the
  accumulator as a scalar without pointer round trips.
- [x] Boundary and smell scans remain at baseline (52 boundary findings: 5
  violations and 47 migration debts; 7 smell warnings).

## 6. AI Review / Prompt Firewall

- [x] Production rule uses structural HIR/type-constraint evidence only.
- [x] Row identity is retained only in this required baseline anchor.
- [x] No reference decompiler output style is copied.

## 7. Review Notes

- [x] No hardcoded row identity in production code.
- [x] Existing use-driven inference owner is extended.
- [x] Semantic and readability validation remain separate.
