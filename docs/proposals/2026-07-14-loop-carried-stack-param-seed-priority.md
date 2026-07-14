# Decompiler Change Proposal: Loop-carried binding priority (param / HW / partial)

## 1. Baseline Row Anchor

- Binary: dev-corpus `control_flow` gcc-m32 -O2
- Function: `count_bits` — timeout (0/6), missing `param_1 >>= 1`
- Related: `checksum` must remain 5/5 (cursor `edx++`, sum zero seed)

## 2. Owner Proof

- [x] `loop_carried/mod.rs` binding selection order
- Pre-normalize HIR had `uVar5 >>= 1` while entry used `param_1`; normalize
  reconnected reads and dead-eliminated the writeback.

## 3. Invariant Proof

```text
1. Anonymous merge temps lose to stable seeds (stack param / HW register).
2. When stack-param seed and HW name both exist and differ:
   - primary-return full register → prefer stack param (scalar induction)
   - other GPRs → prefer HW name (pointer cursor)
3. Partial primary-return lanes (AL) reuse a wider same-family materialized
   name (xor-zero on EAX) so the accumulator keeps its zero seed.
```

## 4. Validation

- count_bits all variants 6/6 (docker local)
- checksum all variants 5/5 (docker local)
- `cargo nextest run -p fission-pcode`
