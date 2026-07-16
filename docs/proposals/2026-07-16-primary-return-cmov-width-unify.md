# Decompiler Change Proposal: Unify primary-return cmov width on x64

Status: **implemented** (2026-07-16)

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc -O2 (x64)
- Function: `saturating_add` @ `0x1400015e0`
- Semantic baseline: **4/5** (negative overflow / INT_MIN case)
- Asm (correct): `mov edx, 0x80000000; cmovl eax, edx; ret`
- Bad shape: cmov body materializes `xVar10 = 0x80000000` while epilogue
  returns `rax` (sum) — INT_MIN never reaches the return

## 2. Owner Proof

- [x] Builder/materialize
- DIAG before: `cmov_body lhs=xVar10` vs m32 working `lhs=eax` / `return eax`
- SLEIGH raw correct: `IntZExt RAX←EAX` then `Copy EAX ← INT_MIN` after CBranch
- First wrong fact: full-width `IntZExt rax` bound as a fresh temp; cmov reused
  that temp via same-block family join while epilogue recovered `return rax`

## 3. Invariant Proof

```text
On x64, when SLEIGH freezes the return value with IntZExt/IntSExt rax ← eax
AND a later same-block-forward cmov body writes the primary-return family,
the freeze must bind the ABI surface name (rax) so the cmov body family-joins
onto the same name used by epilogue return rax.

Guards (required): is_64bit, pointer_size >= 8, primary-return family,
extend from same-offset narrower half, later cmov body write. Not applied to
m32, param-owned slots, or freeze-without-cmov (RC4 movzx truncation).
```

ISA-agnostic data: register namer / pointer_size / primary-return offset /
is_64bit / same-block-forward cmov CFG only — no mnemonic or corpus address gates.

## 4. Validation

- Unit: `x64_eax_int_min_arm_shares_rax_return_surface` (materialize: cmov → rax)
- Full crate: `cargo nextest run -p fission-pcode` (1217 passed)
- Docker local (observation only): saturating_add / clamp / signum / checksum /
  count_bits all compiler variants **ALL_PASS** (incl. sat gcc -O2 5/5; clamp
  and signum O2 improved vs prior residual)
- Guards preserve: rc4 movzx index truncation; m32 checksum O0; return-7 zext
