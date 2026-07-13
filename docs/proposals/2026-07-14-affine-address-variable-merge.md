# Affine Address Inline Safety

## 1. Baseline Row Anchor

- Binary: `data_structures_gcc_O0.exe`
- Function: `sum_array`
- Address: `0x140001530`
- Corpus row or benchmark command: `fission-benchmark` dev corpus, Fission local main
- Current output summary: the array address `base + index * 4` is rendered as `base * 5`
- Semantic cases passed / total: 1 / 5 on the last published release baseline
- Failure category: assertion failure
- Relevant observations: Fission and Ghidra raw p-code agree for this row.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
Builder materialization trace:
  xVar8 = rax * 4
  xVar9 = xVar8
  rax = param_10
  rax = rax + xVar9

Normalized NIR:
  rax = param_10
  rax = rax * 5

Targeted rename diagnostics prove `variable_merge` only coalesces an unrelated
home slot. The first unsafe owner is cleanup's single-use temporary inliner,
which currently scans past redefinitions of variables read by the saved RHS.

After that temporal substitution is blocked, the focused harness exposes the
upstream role collapse: one primary-return hardware register binding represents
an index live interval, then a pointer-base live interval, producing `ptr * 4`.
Both intervals have a consumer before the next register definition.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Temporary inlining must preserve reaching-definition identity. A saved RHS may
only be substituted at a later use when every variable read by that RHS still
has the same reaching definition. An intervening assignment to any dependency
blocks forward inlining even when the saved temporary itself has one use.

Materialization must also split consumed register live intervals: when a register
definition has a bounded consumer and is then redefined in the same block, the
next definition must not inherit the earlier binding merely because storage
overlaps. A write with no bounded consumer may still share the binding for
path-composition shapes such as conditional moves.
```

ISA-agnostic check:

- [x] Production condition is not gated on an ISA or calling convention.
- [x] No ISA-specific data is added.
- [x] Synthetic coverage describes only assignments, uses, and live ranges.

Comparable coverage:

- Similar shape 1: scaled array load address retained across a base-register reload.
- Similar shape 2: scaled array store address retained across a value-register reload.
- Synthetic invariant test: a scaled index temporary is used after its source register is redefined.

## 4. Risk And Ownership Check

- Existing owners: `normalize/cleanup/temp_var.rs` and `builder/materialize/mod.rs`
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
- Why extending that owner is sufficient: it already scans the exact definition-to-use window and rejects temp redefinitions and control boundaries.
- Possible interaction: copy propagation, algebraic simplification, pointer arithmetic recovery.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none planned.
- Known cases that must not change: safe disjoint temporary reuse and existing GPR co-occurrence protection.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(inline_single_use_temps_preserves_rhs_across_dependency_redefinition)'`
  - Expected signal: the scaled temporary remains materialized across a source-register redefinition.
- [x] Targeted register live-interval test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(same_block_register_binding_splits_consumed_live_intervals)'`
  - Expected signal: a consumed-then-redefined register does not reuse its prior binding.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: all tests pass.
- [x] Focused benchmark row:
  - Command: local Fission `sum_array` dev row.
  - Expected row-level improvement: array access retains `base + index * 4` semantics and passes more oracle cases.
- [x] Smoke or automation sample:
  - Command: local Fission dev semantic smoke.
  - Expected no-regression signal: existing passing rows remain passing.
- [x] Optional related checks:
  - Command: full dev p-code parity.
  - Expected signal: raw p-code parity remains unchanged.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not applicable.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external model; repository-grounded implementation only.
- Information exposed in the AI prompt:
  - [x] Structural failure pattern only
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction confirmed for external prompts: no external prompt was sent.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request.
- Unseen or synthetic validation evidence:
  - Synthetic variable-merge invariant test listed above.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
