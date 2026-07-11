# Decompiler Change Proposal: Preserve transitive address local types

## 1. Baseline Row Anchor

- Binary: local dev-corpus 32-bit unoptimized control-flow sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Corpus row or benchmark command: focused local Fission run with `--no-publish`
- Current output summary: the buffer parameter is correctly pointer-typed, but
  an intermediate local casts it to a 32-bit integer before contributing to a
  dereferenced cursor, truncating the host pointer.
- Semantic cases passed / total: 0 / 5
- Failure category: runtime error (`SIGSEGV`)

## 2. Owner Proof

- [x] Type/data recovery: transitive address contributor local

Evidence:

```text
pointer parameter -> intermediate local -> cursor update -> Load address
```

The shared dependency graph recovers the parameter root, but local type
recovery currently promotes only the final address binding.

## 3. Generality / Invariant Proof

```text
A local on a def-use path from a proven pointer root to a Load/Store address
must preserve pointer-width semantics unless an explicit truncation operation
exists. Pointer-sized integer casts introduced only by storage width are not
evidence that host recompilation may truncate the address.
```

- [x] Uses def-use paths and memory address roles only.
- [x] No function, address, compiler, register name, or ISA branch.
- [x] Synthetic test uses neutral base/intermediate/cursor bindings.

## 4. Risk And Ownership Check

- Existing owner: shared definition dependencies plus type/use inference.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
- Extend `DefinitionDependencyMap` with address contributor queries; do not add
  a new normalize pass.
- Risk: classifying scalar indexes as pointers. Only nodes that reach a proven
  pointer root are returned; unrelated index branches are excluded.
- New owner dependency:
  - [x] None; existing type owner consumes existing normalize analysis.
- Known cases that must not change: pointer-plus-length optimized rows and
  scalar loop indexes sharing the address expression.

## 5. Validation Matrix

- [x] Targeted invariant tests for contributor path selection and type stability.
- [x] `cargo check -p fission-pcode`.
- [x] `cargo nextest run -p fission-pcode --no-fail-fast`: 1,158 passed,
  5 known failures, 9 skipped; no new failure.
- [x] Focused local benchmark: all four compiler/optimization variants pass
  5 / 5; the 32-bit O0 intermediate preserves pointer width.
- [x] Boundary and benchmark-smell scans remain at the existing baseline:
  5 boundary violations, 47 migration findings, and 7 smell warnings.

## 6. AI Review / Prompt Firewall

- [x] Production condition uses structural owner evidence only.
- [x] Row identity remains only in this required baseline anchor.
- [x] No reference decompiler style is copied.

## 7. Review Notes

- [x] No hardcoded row identity in production code.
- [x] Shared analysis is extended instead of adding a corpus-shaped pass.
- [x] Compile-and-run validation is required.
