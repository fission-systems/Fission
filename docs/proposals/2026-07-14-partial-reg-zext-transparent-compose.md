# Decompiler Change Proposal: Transparent same-register IntZExt in partial compose

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc -O2 (x64)
- Function: `signum` @ `0x140001560`
- Failure: semantic **3/5** (negative inputs return 0 instead of -1)
- Bad shape:
  ```c
  xVar9 = !zf;
  xVar9 = 0;   // Int2Comp of stale full EAX (xor zero), ignoring setnz AL
  ...
  if (!uVar) { xVar9 = 1; }  // cmovg
  return xVar9;              // only 0 or 1
  ```
- Asm (correct): `xor eax,eax; mov edx,1; test ecx,ecx; setnz al; neg eax; test ecx,ecx; cmovg eax,edx; ret`

## 2. Owner Proof

- [x] Builder/materialize: `try_lower_zero_extended_partial_register` in `lower_expr.rs`
- Raw p-code is correct: setnz AL then `Int2Comp EAX`
- After `xor EAX`, SLEIGH emits `IntZExt RAX ← EAX`. Reverse scan for composing
  setnz AL into full EAX hits that IntZExt as an overlapping non-clear def and
  aborts, so Int2Comp still reads the pre-setnz zero.

## 3. Invariant Proof

```text
INT_ZEXT of a same-space, same-offset low subregister into a wider register
preserves the low lane and zeros the newly introduced upper bytes. It must not
block composing a later low-lane write with an older register clear (xor/sub
idiom). Upper bytes introduced by the zext count as zeroed ranges for the
requested read width.
```

ISA-agnostic: space/offset/size + IntZExt opcode only; no CC/function gates.

## 4. Validation

- Unit: `x64_xor_zext_setnz_neg_composes_partial_into_full_return`
- Existing: `x86_32_xor_setnz_add_composes_partial_into_full_return` (no zext)
- Type follow-up: `promotes_uchar_return_temp_when_return_already_i32` — after
  compose restores `x = -x`, returned uchar temps must widen to ABI i32 so
  recompilation does not truncate `-1` → `255`
- Host decomp signum @ 0x140001560: neg path yields -1/0 before cmovg; `int xVar`
- Docker local: all signum variants; no regression checksum/count_bits m32/x64
