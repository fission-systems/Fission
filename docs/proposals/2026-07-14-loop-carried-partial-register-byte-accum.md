# Decompiler Change Proposal: Loop-carried partial-register byte accumulators

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc-m32 -O2
- Function: `checksum` (`_checksum`)
- Failure: semantic 2/5 — multi-byte cases assert (last-byte-only)
- Bad shape: `do { al = *edx; edx++; } while (...); return al;`
- Assembly/p-code retains `add al, [edx]; movzx eax, al`

## 2. Owner Proof

- [x] Builder materialize: `loop_carried/shape.rs`
- `is_loop_carried_register_update_candidate` required `output.size >= 4`
- Size-1 AL self-updates were never admitted, so pre-loop `xor eax,eax`
  was inlined into the self-read and folded: `al + *mem` → `*mem`

## 3. Invariant Proof

```text
A register-space self-update that reads its prior value and reaches a loop
backedge without an intervening non-preserving kill is loop-carried, for any
positive register width (including partial GPR lanes).
Value-preserving ZExt/SExt/Copy/Cast of the same storage does not kill the
carried definition (existing kill helper).
```

- No function/address/binary/compiler/ISA enum gate.
- Exact carried proof still required (fail-closed).

## 4. Risk And Ownership

- Extend existing candidacy predicate only; no new pass.
- Flag-sized writes that do not self-read remain non-carried.

## 5. Validation Matrix

- Synthetic: size-1 self-add before ZExt proves carried
- Synthetic: multi-block byte accum + movzx does not collapse to bare load
- Existing `loop_carried` suite
- Host decomp of motivating row shows `sum += *p; sum = (uchar)sum`
- Docker local: checksum all compiler variants
