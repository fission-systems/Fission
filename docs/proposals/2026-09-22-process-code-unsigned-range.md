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
- Direct output defect: the gcc `-O2` NIR output renders the unsigned machine
  comparisons as `(int)(code - 1)` and `code - 400 < 100`. For `code < 0`,
  the source returns `0`, but the rendered body returns `2` because the first
  range comparison is evaluated with signed C arithmetic.

## 2. Owner Proof

- [x] P-code lowering / NIR expression construction
- [x] Normalize/type surface
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

The canonical owner is the shared p-code lowering of unsigned integer
comparisons, including the x86 branch-predicate path that reconstructs a
comparison from CF/ZF. The fix must make the comparison operands unsigned at
the expression boundary; it must not alter `IntSub` globally or add a function
or address guard.

## 3. Generalized Rule

```text
An unsigned p-code comparison must compare scalar integer operands at the
comparison width as unsigned values, even when an operand is a signed-looking
arithmetic expression whose bit pattern is being reused by the comparison.
Preserve pointer/aggregate operands and already-unsigned operands; do not
change unrelated arithmetic result types.
```

This covers direct `IntLess`/`IntLessEqual` expressions and x86 CF/ZF-derived
branch predicates. It is an ISA-agnostic p-code comparison invariant; x86
only supplies the flag-shaped consumer.

## 4. Validation Matrix

- [ ] Focused synthetic p-code regression: a signed-looking `IntSub` used by
  `IntLess` must render an unsigned operand cast and preserve negative-value
  behavior.
- [ ] Real-binary direct output: `process_code@0x140001730` must contain
  unsigned range comparisons for the subtracted operands.
- [ ] Cache-disabled DecBench rerun on the same nine `process_code` rows.
- [ ] `cargo nextest run -p fission-pcode` and emulator regression.
- [ ] Workspace check, format/diff check, and release CLI build.

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
