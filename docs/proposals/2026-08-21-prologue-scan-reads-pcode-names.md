# The prologue scan is looking for assembly and being handed p-code

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/.../binaries/bin_008.elf`, `sub_ee70` at `0xee70`
- Its prologue is entirely ordinary:

```text
0xee70  endbr64
0xee74  push R15
0xee76  push R14
0xee78  push R13
0xee7a  push R12
0xee7c  push RBP
0xee7d  push RBX
```

- Current measured output: `stack_frame_size == 0`.

Six pushes, forty-eight bytes of frame, and the builder concludes the function
has no frame at all.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [x] Builder · [ ] Normalize · [ ] Structuring
- [ ] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

`entry_analysis::infer_entry_stack_layout` recognises a prologue from
assembly text:

```rust
if asm.starts_with("PUSH ") { frame_size += pointer_size; ... }
let sub_rsp = asm.strip_prefix("SUB RSP,");
if asm.starts_with("MOV RBP,RSP") { frame_pointer_established = true; ... }
```

Every one of those needs operands: `"PUSH "` carries a trailing space,
`"SUB RSP,"` a register and a comma. Instrumenting what the field actually
holds at those exact sites:

```text
[PROLOGUE] 0xee74  asm="COPY"
[PROLOGUE] 0xee76  asm="COPY"
[PROLOGUE] 0xee7e  asm="INT_LESS"
```

`asm_mnemonic` is carrying **p-code opcode names**, not disassembly. `"COPY"`
does not start with `"PUSH "`, so no prologue form can ever match and
`frame_size` stays at its initial `0`.

Measured across 40 binaries: of 49 functions the scan reached, **0 had an
`asm_mnemonic` containing operands** -- not one. The values are `COPY` (188),
`INT_ADD` (47), `INT_LESS`, `INT_OR`, `BOOL_NEGATE`, `SUBPIECE`.

The fact is already written down, in this same file, in the test helper's
comment:

> Never actually `None` from the real pipeline (confirmed: falls back to the
> raw p-code opcode name, e.g. "COPY", "INT_SUB", when there's no real
> disassembly text)

It was recorded to explain why the helper fills the field at all, and the scan
fifty lines above it went on looking for `PUSH `.

## 3. Why it stayed invisible

`resolve_stack_address_from_memory_op` reads the *same field* and searches for
`[`, so it fails the same way and falls back silently. Nothing errors. The
function still decompiles, stack slots are still created -- just without a
frame to place them in, which is what makes `rsp_local_display_offset` fall to
`offset.unsigned_abs()` and name `rsp+16` and `rsp-16` identically. See
`2026-08-21-stack-slot-name-collision.md`: 63 of 63 name collisions occur at
`stack_frame_size == 0`.

And the unit tests pass, because the helper constructs ops whose
`asm_mnemonic` is `format!("{opcode:?}")` -- the same p-code names -- for op
sequences hand-built to *represent* a prologue. The tests exercise the scan on
input shaped like what the real pipeline produces, against expectations
written for input shaped like disassembly.

## 4. Risk And Ownership Check

Not fixed here, and the fix is not obviously "populate the field with
disassembly text". Two other readers already depend on the current contents,
one of them (`tail_wrapper`) on the literal string `"INSN_RAW"`. Changing what
the field carries changes all of them at once.

The alternative is to stop reading assembly text for this at all: the prologue
is recognisable from p-code directly -- a `push` is a `Store` through a
decremented `rsp`, and this same function already tracks `rsp_delta` in p-code
for `frame_pointer_bias`, independently of the text scan. That tracking is the
shape the frame size should come from too.

## 5. Validation Matrix

- [x] Anchor function's real prologue read from `fission_cli disasm`.
- [x] `asm_mnemonic` contents instrumented at the scan's own comparison sites.
- [x] Corpus-wide: 0 of 49 functions carry operand-bearing text.
- [x] Correlation with the downstream naming collision already established.
- [ ] Whether p-code-level prologue recognition reproduces the frame sizes the
      text scan was meant to find. **Next cycle.**

## 6. AI Review / Prompt Firewall

- No external model was consulted. Both diagnostics were temporary and are not
  committed.
