# Decompiler Change Proposal: Split incompatible storage-role phases

## 1. Baseline Row Anchor

- Binary: local dev-corpus 32-bit unoptimized control-flow sample
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Corpus row or benchmark command: focused local Fission run with `--no-publish`
- Current output summary: one local first carries a pointer base and later a
  loaded byte, so the byte is rendered through pointer casts and subsequent
  scalar arithmetic casts it back.
- Semantic cases passed / total: 5 / 5
- Failure category: none; readability/type-role defect

## 2. Owner Proof

- [x] Normalize SplitFlow: storage-role phase separation

Evidence:

```text
pointer phase: local = pointer parameter; cursor += local
scalar phase:  local = pointer_cast(narrow_scalar); accumulator += local
```

Type inference cannot assign one correct declaration to both phases. The first
wrong representation is the unsplit HIR binding, not the printer.

## 3. Generality / Invariant Proof

```text
When a pointer-typed local is redefined from a narrow scalar value and all
reads until the next definition are scalar-only, that definition begins a
separate scalar live-range phase. Split the phase and remove only the
assignment-context pointer cast. Fail closed across control-flow joins or
address uses.
```

- [x] Depends on binding type, definition boundaries, cast width, and use role.
- [x] No function name, address, compiler, register name, or ISA condition.
- [x] Synthetic tests use neutral pointer/value/accumulator bindings.

## 4. Risk And Ownership Check

- Existing owner: `normalize/idioms/split_flow.rs`.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
- Extend the existing SplitFlow pass; do not add a pipeline pass.
- Risk: renaming a value that crosses a branch or is later used as an address.
  Reject those cases and only split a linear statement suffix before the next
  definition.
- New owner dependency:
  - [x] None.
- Known cases that must not change: pointer-only copies, scalar-only locals,
  and high/low piece SplitFlow behavior.

## 5. Validation Matrix

- [x] Targeted split/no-split invariant tests (4 passed).
- [x] `cargo check -p fission-pcode`.
- [x] `cargo nextest run -p fission-pcode --no-fail-fast` with no new failures
  (1162 passed, 5 known baseline failures, 9 skipped).
- [x] Focused local benchmark remains 5 / 5 for all four variants and removes the
  pointer cast from the loaded-byte phase.
- [x] Boundary and smell scans remain at baseline (52 boundary findings: 5
  violations and 47 migration debts; 7 smell warnings).

## 6. AI Review / Prompt Firewall

- [x] Production rule uses structural HIR evidence only.
- [x] Row identity is retained only in this required baseline anchor.
- [x] No reference decompiler output style is copied.

## 7. Review Notes

- [x] No hardcoded row identity in production code.
- [x] Existing SplitFlow owner is extended.
- [x] Semantic and readability validation remain separate.
