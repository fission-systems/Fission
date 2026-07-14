# Decompiler Change Proposal: Do not complete-replace same-block cmov bodies

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc -O2 (x64)
- Function: `clamp` @ `0x140001550`
- Failure: semantic **5/6** (loses upper bound)
- Bad shape: `return param_2 <= param_1 ? param_1 : param_2` (max only)
- Asm (correct):
  ```text
  cmp ecx, r8d
  mov eax, edx
  cmovle r8d, ecx      ; hi = min(value, hi)
  cmp ecx, edx
  cmovge eax, r8d      ; result = max(lo, hi')
  ret
  ```

## 2. Owner Proof

- [x] Builder/materialize
- Raw p-code correct; both CBranch skips resolve (`op_idx=17→19`, `33→35`)
- DIAG: first cmov **body_stmts=0**; second body `uVar5 = param_1`
- Cause: `output_replacement_is_complete` sees one later use of R8 and treats the
  guarded `Copy R8 ← value` as always-taken complete inline, then
  `representative_downgrade` emits no statement. Later uses always read value.

## 3. Invariant Proof

```text
A def that sits strictly between a same-block-forward CBranch and its skip
target is conditionally executed. It must not be classified as a complete
value replacement for later uses. Materialize the guarded assignment (or
equivalent select); do not unconditional-inline the taken-path RHS.
```

ISA-agnostic: same-block forward CBranch + op index range only.

## 4. Validation

- Unit: `preview_dual_absolute_cmov_clamp_chain_keeps_hi_bound` (strengthened)
- Host/docker: clamp gcc -O2 6/6; no regression signum/checksum/count_bits
