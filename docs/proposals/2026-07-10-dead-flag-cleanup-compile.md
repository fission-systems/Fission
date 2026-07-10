# Decompiler Change Proposal — Dead EFLAGS cleanup (compile blockers)

## 1. Baseline Row Anchor

- Binary: `crypto_gcc_O0.exe` / `control_flow_gcc_O0.exe` (fission-benchmark dev)
- Functions: `rc4_init`, `rc4_crypt` (also any post-flag-name-recovery row)
- After SLA flag naming (`a6949b77`), C output showed undeclared `cf`/`of`/`sf`/`zf`/`pf`
  assigns that were pure noise after high-level conditions recovered.
- `count_bits` already returns loop-carried accumulator on current main (Slice 2 done).
- `checksum` scalar/`%=` compile issue is largely fixed; remaining failure is **loop
  structuring** (orphan goto / no while lower) — separate owner.

## 2. Owner Proof

- [x] Normalize / flag_recovery: dead flag assign elimination only ran when a
  condition rewrite `changed == true`, and not late after residual stores.
- [x] Cleanup / rescue: `is_rescue_candidate_name` excluded flag names, so leftover
  live flags still compiled as undeclared identifiers.

## 3. Generality / Invariant Proof

```text
1. Assignments to x86 flag vars {cf,pf,af,zf,sf,of,df,if_} with zero rvalue uses
   and pure RHS must be eliminated, whether or not this pass rewrote a condition.
2. Late normalize waves may reintroduce residual flag stores; a final dead-flag
   cleanup must run before rescue/print.
3. If a flag remains live, rescue must declare it as Bool (not leave undeclared).
```

No function name, address, or corpus id in the production condition.

## 4. Validation

- Targeted: `dead_flag_assigns_removed_without_condition_rewrite`
- Crate: `cargo nextest run -p fission-pcode` (all pass)
- Remeasure: `rc4_init` prologue/body no longer emits flag assigns; compile path open.
