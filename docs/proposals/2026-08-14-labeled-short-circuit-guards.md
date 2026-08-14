# Fold forward guard chains into a labeled conditional body

## 1. Measured anchors

At the 996-NIR / 988-HIR corpus baseline, five forward gotos have a target
label three rendered lines later. They are the structured-AST shape:

```text
if (A) { goto L; }
if (B) {
L:
    BODY
}
```

Measured rows are `bin_030` (two consecutive guards to the same label),
`bin_040`, `bin_200`, and `bin_209`. These are real DecBench outputs, not
synthetic motivation.

## 2. Owner and invariant

Owner: structuring post-layout cleanup, specifically
`invert_forward_guard_gotos`. The CFG is already faithful; the late AST layout
has exposed a short-circuit condition across a C label.

For consecutive guards `A1..An` that all jump to `L`, immediately followed by
`if (B) { L: BODY }`, when those guards are every reference to the single
definition of `L`, the sequence is equivalent to:

```text
if (A1 || ... || An || B) { BODY }
```

Logical OR preserves evaluation order and skips later conditions after the
first true condition, exactly like the original forward transfers.

## 3. Safety contract

- Guards and the labeled `if` must be consecutive siblings.
- Every guard must have one goto and no else arm.
- The label must be the first statement of the final then-arm; its else arm
  must be empty.
- The label must have exactly one definition, must not be protected, and its
  total reference count must equal the number of consumed guards.
- No address, sample, compiler, function name, or ISA fact enters production
  logic.

## 4. Validation matrix

- [x] Positive one-guard and multi-guard tests.
- [x] Negative external-reference, protected-label, and non-leading-label tests.
- [x] Focused NIR/HIR rows.
- [x] Structuring and pcode crate tests/checks.
- [x] Full NIR/HIR corpus with per-file regression comparison.
- [x] Real-machine differential and external Docker smoke.

## 5. Claim gate

No quality claim is made until the measured rows and full corpus improve
without per-file or semantic regression.

## 6. Measured results

- Focused: `bin_030` NIR 10→8 and HIR 9→7; `bin_209` NIR/HIR 73→72.
  The other-reference cases in `bin_040` and `bin_200` remained unchanged.
- Full 224-binary / 250-function corpus: NIR 996→993 and HIR 988→985.
  Two files improved, 222 tied, and zero regressed in each layer.
- Common 204-file comparison: NIR 952, HIR 944, Ghidra 691, angr 420.
- Structuring tests 242/242, pcode tests 1000/1000, pcode/decompiler checks,
  real-machine differential 1/1, and owner-boundary scan all passed. Full
  workspace: 2502 passed, 7 failed, 5 skipped; the seven failures are the
  unchanged emulator `addr64 DecodeNoMatch` set at `0x1007c8e`.
- External local Docker smoke: 10/10 backend clean, 10 non-empty outputs,
  adapter output/matrix/artifact/overall validity all true. Not published.
