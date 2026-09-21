# Decompiler Change Proposal: Preserve Pointer Return Contracts Through Integer Casts

Date: 2026-09-21
Issue: #109

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/libc_types_gcc_O2.exe`
- Function: `make_range`
- Address: `0x140001730`
- Corpus row / benchmark command: `libc_types.c`; release `fission_cli decomp` with
  `--addr 0x140001730 --no-db --layer both --prehir --no-warnings`; focused
  DecBench run `results/issue109_before_bd57f6355.json`.
- Current output: the direct HIR declares `uint make_range(int param_1)` and
  returns `(uint)(unsigned long long)(addr)`, although `addr` is a `uint *`
  binding used by indexed stores. The project prototype is also `uint`, so the
  caller consumes the result as an integer before indexing it.
- DecBench baseline: 7/7 focused rows reached the runner, but all scored
  `0/5` semantic cases with `compile_error` because the generated declaration
  `extern unsigned long long malloc(...);` is not valid ISO C. This is a
  separate existing declaration issue; the typed-return defect is visible in
  the direct and project output before that compile gate.
- Relevant observation: the pointer provenance is already present in the
  normalize input. The first wrong public fact is `PreHirFunction.return_type`
  remaining an integer because the builder seeds it from the outer cast type.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize / type recovery
- [ ] Structuring
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
PreHIR contains:
    addr = (uint *)(rax);
    *addr = ...;
    return (uint)(unsigned long long)(addr);

The binding pass already records addr as Ptr(Int(32, unsigned)), but the
builder's return seed and the return-type pass observe only the outer Int cast.
The p-code return carrier is unchanged; only the public type contract is lost.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When every value-return path is a representation-only scalar cast chain whose
leaf is a binding proven to be Ptr(T), preserve Ptr(T) as the function return
type. A zero/null integer return is neutral when another path supplies the
pointer candidate. Do not infer a pointer return from arithmetic, bitwise,
comparison, load, or call expressions that merely contain a pointer value.
```

The rule is based on expression shape and existing pointer binding facts. It
does not depend on a function name, address, binary, compiler, or ISA.

ISA-agnostic check (ADR 0009):

- [x] The production condition uses typed def-use evidence, not an ISA enum or
      register name.
- [x] No architecture-specific copy of the return rule is introduced.
- [x] The synthetic tests express pointer provenance plus representation-only
      return casts.

Comparable coverage:

- Similar shape 1: a pointer local returned through one or more integer casts.
- Similar shape 2: pointer return with a neutral null constant on one branch.
- Synthetic invariant tests: nested representation-only casts preserve the
  pointer return, while pointer-to-integer arithmetic remains scalar.

## 4. Risk And Ownership Check

- Existing owner: `types/type_infer/return_type.rs`, called by
  `apply_type_inference_pass` in the normalize type fixed point.
- Shared analysis candidate: existing binding/definition type facts; no new
  fact map is needed.
- Extending the return-type owner is sufficient because the wrong fact is
  created when the return expression is classified, before rendering or
  project prototype assembly.
- Known cases that must not change: explicit surface return hints, arithmetic
  or bitwise pointer-to-integer conversions, and scalar return expressions
  containing unrelated pointer operands.
- Main risk: promoting an intentional `uintptr_t` conversion to a pointer.
  Restricting the rule to a complete cast-only chain and requiring a proven
  pointer leaf avoids conversions that compute a scalar value.
- No new owner-to-owner dependency or telemetry change.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: focused normalize nextest filters for the new return-type tests.
  - Expected signal: old code keeps the scalar return; fixed code returns
    `Ptr(Int(32, unsigned))` and leaves arithmetic conversions scalar.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode` and the normalize crate tests.
  - Expected signal: no new failures beyond the two existing lower-expr tests.
- [ ] Focused benchmark row:
  - Command: rerun `make_range` with the local external DecBench service and
    caches disabled, plus direct release CLI output.
  - Expected row-level improvement: typed `int *` function/prototype and a
    pointer-typed caller expression; the separate invalid `malloc(...)`
    declaration compile failure may remain until its own issue is addressed.
- [ ] Smoke or automation sample:
  - Command: `cargo check`, `cargo fmt --all --check`, `git diff --check`, and
    `cargo build -p fission-cli --release`.
  - Expected signal: all pass.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- No external implementation prompt was used.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from synthetic tests alone:
  - [x] Confirmed; direct output and focused DecBench results are required.
- No new pass/helper/metric duplicates an existing owner:
  - [x] Confirmed
