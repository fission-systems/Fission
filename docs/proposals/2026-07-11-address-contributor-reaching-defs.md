# Decompiler Change Proposal: Recover address contributors across redefinitions

## 1. Baseline Row Anchor

- Binary: local dev-corpus unoptimized control-flow samples
- Function: `checksum`
- Address: resolved from the corpus manifest at run time
- Corpus row or benchmark command: focused local Fission run for one function
- Current output summary: the buffer parameter remains an integer while a
  reused cursor local is pointer-typed; generated callers cannot pass a byte
  buffer. The accumulator binding is independently collapsed with a pointer.
- Semantic cases passed / total: 0 / 5 for both unoptimized variants
- Failure category: compile error
- Relevant observation: optimized variants pass 5 / 5, so this slice is about
  multi-definition HIR rather than calling convention selection.

## 2. Owner Proof

- [x] Type/data recovery: address-contributor provenance

Evidence:

```text
The current def map stores only the first assignment for each HIR binding.
An unoptimized cursor is first seeded from an integer index, then updated with
a parameter-derived base, and finally dereferenced. The later additive
definition is absent from the map used to recover parameter pointer types.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Address provenance must follow reaching definitions that contribute to a
Load/Store address. A first-definition map is insufficient when one hardware
storage binding is reassigned before the memory operation. Parameter promotion
requires a path from the parameter to an address use and must distinguish
pointer-base from scalar-offset evidence.
```

- [x] The condition is based on HIR def-use and address-use facts.
- [x] No function name, address, compiler tuple, ABI register, or ISA condition.
- [x] Synthetic coverage will use neutral cursor/base/index names.

Comparable coverage:

- Similar shape 1: cursor seeded by an index and then incremented by a base.
- Similar shape 2: cursor copied through aliases before a load.
- Synthetic invariant test: a formal base reaches a load address through a
  later redefinition after an unrelated first definition.

## 4. Risk And Ownership Check

- Existing owner: normalize type recovery.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
- Existing `DefUseMap` only counts uses; extend shared analysis rather than add
  another function-specific type pass.
- Risk: promoting an index parameter to pointer. Require address provenance and
  preserve independent scalar-offset evidence.
- New owner dependency:
  - [x] None; type recovery consumes normalize analysis facts.
- Known cases that must not change: optimized pointer-plus-length rows and
  pointer copies with direct load/store evidence.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: focused `cargo nextest` filter for redefined address provenance
  - Result: dependency and type-recovery tests pass; base becomes a byte
    pointer while an unrelated limit remains scalar.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode --no-fail-fast`
  - Result: 1,158 passed, 5 known failures, 9 skipped; no new failure.
- [x] Focused benchmark row:
  - Command: benchmark runner with `--function checksum --no-publish`
  - Result: 64-bit O0 improved from 0 / 5 to 5 / 5. 32-bit O0 moved from
    compile error to runtime error; pointer-width cursor recovery remains.
- [x] Smoke or automation sample:
  - Command: `cargo check -p fission-pcode`
  - Result: successful check.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: 5 existing violations and 47 migration findings; no new
    owner-boundary violation.

## 6. AI Review / Prompt Firewall

- [x] AI assistance uses structural owner evidence for production conditions.
- [x] Row identity remains only in this required baseline anchor.
- [x] No reference decompiler output style is copied.
- Synthetic validation is required before focused corpus validation.

## 7. Review Notes

- [x] Production code contains no hardcoded row identity.
- [x] Shared def-use analysis is preferred over another special-case pass.
- [x] Semantic improvement requires compile-and-run verification.
