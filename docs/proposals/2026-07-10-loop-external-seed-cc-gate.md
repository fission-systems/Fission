# Decompiler Change Proposal: Drop external-seed CC admission gate

## 1. Baseline Row Anchor

- Motivating work: ADR 0009 / debt inventory **D2** (not a single corpus row)
- Current behavior: `loop_header_external_seed_binding_name_for_update` returns
  `None` unless CC is WindowsX64 | SystemVAmd64 | X86_32, even when the rest of
  the helper already keys on param slot / register-space / CFG external preds
- Failure category: ISA admission gate on a shared loop-carried helper

## 2. Owner Proof

- [x] Builder/materialize: `loop_carried/binding.rs`

Evidence: early `matches!(calling_convention, …)` before CFG seed collection.

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a loop-carried register update has a unique external-header seed binding
name collected along external predecessor paths, reuse that binding name.
Admission is: param slot OR primary return register OR loop-carried register
update candidate (register-space, non-constant, size ≥ 4). No CC enum gate.
```

ISA-agnostic check:

- [x] Not gated only on one CC enum for control meaning
- [x] Return identity via `register_namer().is_primary_return_register`
- [x] Synthetic coverage remains existing loop_carried / external-seed tests

## 4. Risk And Ownership Check

- Existing owner: `loop_carried` binding (extend only)
- Shared fact: CFG external preds + materialized seed names
- No new pass; remove gate only
- Known cases that must not change: x64/x86-32 loop seed binding tests

## 5. Validation Matrix

- [x] Targeted: `cargo nextest run -p fission-pcode -- loop_carried`
- [x] Crate check: `cargo nextest run -p fission-pcode` (or focused filter if full is long)
- Focused benchmark: N/A (architecture cleanup; no row claim)
- Smoke: N/A for this change alone
