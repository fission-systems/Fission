# Decompiler Change Proposal: ABI return width after setcc/neg

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc-m32 / gcc -O2
- Function: `signum`
- Failure: semantic **3/5 assertion_fail** (cases -5, -1000)
- Bad shape: `uchar signum(...)` with correct `return -uVar` body — recompiles
  `-1` as `255` under unsigned 8-bit return type

## 2. Owner Proof

- [x] Normalize types: `type_infer.rs` return-width recovery
- Raw p-code is correct: `setnz al; movzx eax,al; neg eax; ret`
- Body already computes `-1`/`0`/`1`; only the **declared return type** was wrong

## 3. Invariant Proof

```text
1. Zero-extension return narrowing may reduce 64→32, never below 32 bits
   (machine-word ABI integer return).
2. Sub-32 integer return types are promoted to 32-bit; signedness follows
   Neg / high-bit evidence (setnz+neg yields signed i32).
3. Prefer-narrow aggregation of return candidates ranks 32-bit above setcc
   8/16-bit lanes and 64-bit zext wrappers.
```

## 4. Validation

- Unit: `promotes_uchar_return_after_setnz_neg_to_signed_i32`
- Existing zero-ext return narrow tests
- Host decomp: `int _signum` with `return -uVar2`
- Docker local: signum variants; no regression on count_bits/checksum
