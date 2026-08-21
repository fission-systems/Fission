# The frame, read from p-code instead of from a label

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/.../binaries/bin_008.elf`, `sub_ee70`
- Its prologue is six pushes -- forty-eight bytes of frame -- and the builder
  reported `stack_frame_size == 0`.
- Downstream: 214 suffixed duplicate stack slots across 59 of the 250 scored
  functions, of which 123 were declared and never referenced.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [x] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

Two defects in `builder`, and they are one problem.

**The prologue was recognised from assembly text.**
`infer_entry_stack_layout` matched `"PUSH "`, `"SUB RSP,"` and `"MOV RBP,RSP"`
against `asm_mnemonic`. That field does not carry disassembly -- it falls back
to the raw p-code opcode name -- so it held `"COPY"` and `"INT_SUB"`, nothing
matched, and `frame_size` never left zero. Instrumented across 40 binaries: **0
of 49 functions carried operand-bearing text.** The fact was already written in
this file's own test-helper comment; the scan fifty lines above went on looking
for `PUSH `.

**The rsp display coordinate was lossy.** `rsp_local_display_offset` converted
only `offset >= 0 && stack_frame_size > offset` and fell back to
`offset.unsigned_abs()`, so `rsp-16` and `rsp+16` -- thirty-two bytes apart --
both displayed as `local_10`, and the second slot took a `_N` suffix.

They are one problem: every one of the 63 collisions instrumented occurred
while `stack_frame_size` was zero. **Repairing the scan alone made it worse** --
duplicates rose 214 to 243, because a real frame size started remapping the
positive branch into the range the fallback occupied. Measured, not predicted.

## 3. Generality / Invariant Proof

```text
The frame is what the entry prologue does to the stack pointer, read from
p-code. A slot's display name is its distance below entry-rsp, by one
subtraction, for displacements on either side of the stack pointer.
```

Reading p-code rather than repairing the text, for two reasons. Semantics must
not be inferred from a presentation artifact -- disassembly text is rendered
*for* a reader. And `"PUSH "` and `"SUB RSP,"` are x86 spellings that can never
match on ARM however the field is filled, while `IntSub` on the stack pointer
is what every architecture's push lowers to.

The prologue boundary is unchanged in meaning: the leading run of frame-setup
instructions. Instructions lowering to no p-code (`endbr64`) are invisible and
so do not end the run, which is what the text scan's skip-until-started did.

ISA-agnostic check:

- [x] Stack and frame pointers resolved through `RegisterNamer`, not literals.
- [x] No binary, function, or address identity.

## 4. Risk And Ownership Check

`asm_mnemonic`'s contents are left alone. Two other readers depend on them, one
on the literal `"INSN_RAW"`, and changing what the field carries would change
all of them at once -- including `resolve_stack_address_from_memory_op`, which
fails the same silent way today and would start succeeding. That is possibly an
improvement and definitely a separate measurement.

## 5. Validation Matrix

- [x] `cargo nextest run -p fission-pcode` (1,001 passed).
- [x] `cargo nextest run --workspace --no-fail-fast` against the established
      baseline.
- [x] DecBench sample-set rerun, 250 of 250.

```text
suffixed duplicate slots     214 -> 102   (59 functions -> 53)
never-referenced declarations 222 -> 131
```

Half the duplicates and 41% of the dead declarations, from one coordinate.

### Seven fixtures modelled `push` incompletely

They carried the `Copy` half with `asm_mnemonic: "PUSH RSI"` and no stack
pointer decrement -- a shape the real pipeline never produces, which passed
only because the scan read the label. They now carry the `IntSub` too.

That the tests passed against input the pipeline cannot produce is the same
failure as the scan itself: both trusted the label over the p-code.

### A wrong coordinate, caught by the tests

An intermediate version used "distance above steady-state rsp" and reported
duplicates falling 214 to 7. It was wrong: an `rsp`-relative displacement is
measured *from* steady-state rsp, so `local_X` counts bytes below entry-rsp,
and four aggregate tests said so. The number that survives is 102, from the
coordinate the existing tests encode.

## 6. AI Review / Prompt Firewall

- No external model was consulted. Both diagnostics were temporary and are not
  committed.
