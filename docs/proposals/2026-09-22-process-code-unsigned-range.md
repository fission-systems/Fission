# Preserve unsigned range-comparison operands

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/math_gcc_O2.exe`
- Function: `process_code`
- Address: `0x140001730`
- Issue: [#101](https://github.com/fission-systems/Fission/issues/101)
- Baseline command: cache-disabled DecBench `dev/process_code` run using the
  local service at commit `23e2095ee`.
- Baseline result: `results/issue101_before_23e2095ee.json`, 44/45 cases,
  8/9 perfect rows. The default wrapper does not exercise a negative status
  code, so it does not expose the motivating semantic defect in its aggregate
  score.
- Direct output defect: the gcc `-O2` NIR output rendered unsigned machine
  comparisons as signed C expressions. The first two range checks were fixed
  by the preceding comparison-boundary change, but the third check remained
  as `code - 400 < 100` after copy cleanup. That residual expression returned
  `-1` for `code` in `200..399`, although the source returns `0`.

## 2. Owner Proof

- [x] Shared NIR normalization of unsigned comparison operands
- [x] P-code mapping and comparison consumer classification
- [ ] Structuring
- [ ] Printer-only
- [ ] Benchmark/automation

Evidence:

```text
Raw p-code at the motivating row contains IntLess for the carry/unsigned
range checks and IntSLess only for x86 flag bookkeeping. The mapping correctly
distinguishes IntLess -> PreHirBinaryOp::Lt from IntSLess -> SLt.

The generic IntSub result is initially represented as a signed 32-bit integer,
because IntSub has no signed/unsigned opcode. When that result becomes an
operand of an unsigned IntLess predicate, no operand-width/signedness coercion
is added. The printer therefore sees a signed subexpression and emits a signed
C comparison, changing the bit-vector semantics for negative inputs.
```

The canonical owner is the shared normalize pass that sees the final NIR
comparison after p-code lowering and temporary inlining. P-code lowering
already distinguishes `IntLess` from `IntSLess`, and the x86 branch-predicate
path already distinguishes unsigned CF/ZF predicates from signed predicates.
The fix therefore makes the final `Lt`/`Le`/`Gt`/`Ge` operand boundary
explicit, without changing the generic `IntSub` type or adding a function or
address guard.

The first fix narrowed the real row to this sequence:

```text
before final normalize:  (int)(code - 1) < 98
unsigned-boundary pass:  (uint)(code - 1) < 98
atomic variable inputs:  unchanged when their type is unknown/already unsigned
```

The cast is not a conversion of the arithmetic producer; it is a bit-pattern
reinterpretation at the unsigned comparison boundary. Signed comparisons and
already-unsigned or untyped atomic operands retain the old output shape.

The remaining row-specific-looking expression exposed a second, more general
ordering hole. The builder emitted an unsigned temporary for `uVar22 =
code - 400`, followed by `IntLess uVar22, 100`. The final copy/alias cleanup
replaced that temporary with the signed binding `iVar18` after the builder had
already established the unsigned comparison. The final cleanup now restores
the boundary from the binding type, so it covers the same alias shape wherever
it occurs.

## 3. Generalized Rule

```text
An unsigned p-code comparison must compare scalar integer operands at the
comparison width as unsigned values, even when an operand is a signed-looking
arithmetic expression whose bit pattern is being reused by the comparison.
Preserve pointer/aggregate operands and already-unsigned operands; do not
change unrelated arithmetic result types.
```

This covers direct `IntLess`/`IntLessEqual` expressions and x86 CF/ZF-derived
branch predicates once they are represented as shared NIR comparisons. It is
an ISA-agnostic comparison invariant; x86 only supplies one flag-shaped
consumer. The binding-aware final cleanup applies only to signed scalar
integer operands, preserving unknown, pointer, aggregate, and already-unsigned
values.

## 4. Validation Matrix

- [x] Focused synthetic p-code regression: a signed-looking `IntSub` used by
  `IntLess` must render an unsigned operand cast and preserve negative-value
  behavior.
- [x] Normalize regression: a signed-looking compound operand is reinterpreted
  at an unsigned comparison boundary, while an untyped atomic operand is left
  unchanged.
- [x] Real-binary direct output: `process_code@0x140001730` contains
  unsigned range comparisons for the subtracted operands.
- [x] Actual emitted body execution: `-1`, `200`, and `399` return `0`, while
  `400` and `499` return `-1`; the remaining source cases also match.
- [x] Cache-disabled DecBench rerun on the same nine `process_code` rows:
  `fission-benchmark/results/issue101_after_bb907a58f.json`. The aggregate
  remained 44/45 cases and 8/9 perfect rows, with the same `gcc-m32 -O2`
  assertion failure; the standard wrapper does not exercise the negative and
  200..399 cases.
- [x] `cargo nextest run -p fission-pcode` and emulator regression (three
  pre-existing #103 pcode failures remain).
- [x] Full normalize nextest, workspace check, format/diff check, and release
  CLI build.

## 5. Measurement Interpretation

The official dev wrapper uses `process_code(100)` and its standard case set,
so it may remain 44/45 even after the defect is fixed. The direct real-binary
output and a focused negative-input execution check are required to establish
the motivating behavior change; no aggregate score increase will be claimed
unless the measured case set itself moves.

## 6. Guard / Overfit Audit

- No binary, function, address, compiler, or architecture-specific guard.
- No printer-only semantic repair.
- No change to the generic signedness of `IntSub`.
- Existing signed comparison and pointer/aggregate comparison behavior must
  remain unchanged.
