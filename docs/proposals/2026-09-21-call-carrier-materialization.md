# Call-carrier materialization change proposal

## 1. Baseline Row Anchor

- Binary: `semantic_stress_gcc_O2.exe`
- Function: `main`
- Address: `0x1400029a0`
- Corpus row or benchmark command: local DecBench dev row, project decompilation
  with `fission_cli decomp --project`
- Current output summary: four independently written call-carrier values in
  `rcx` are all bound to `xVar6`; the project output therefore passes the stale
  value to `overlap_move`, `mixed_width_accumulate`, and `rotate_words`.
- Semantic cases passed / total: the focused row currently has three observed
  call-argument mismatches (the existing issue is the source of truth; the
  before/after function-level DecBench measurement is part of validation).
- Failure category: builder/materialize register identity and project call
  argument recovery.
- Relevant observations: raw p-code has independent `Copy rcx <- ...`
  definitions at sequences 36, 59, 67, 92, 101, and 109. The materialization
  map resolves all of those definition sites to `xVar6`, even though the
  values cross calls and point at different stack objects.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
The raw p-code preserves the distinct carrier writes. `lookup_def_site` selects
the correct reaching `rcx` definition for each call, but
`same_block_prior_register_binding_name` reuses the earlier materialized name
for later independent writes. The printer only exposes the already-collapsed
binding; it does not create the alias.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
A same-block register binding may be reused only when the current definition
is a proven value-carrying update (for example, it reads the prior value) or
belongs to a proven same-block conditional/cmov merge. An independent register
write after an ABI call-carrier consumption must receive a distinct binding.
```

ISA-agnostic check:

- [x] The production condition is a shared register def-use/CFG invariant, not
  a function, address, or architecture-specific guard.
- [x] Register identity continues to come from the existing register model and
  calling-convention slot data.
- [x] The synthetic test describes repeated register definitions and call
  consumption without a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: repeated `rcx` pointer carriers in the memory-layout and
  data-structure project rows.
- Similar shape 2: distinct aggregate pointers and buffers in the crypto and
  advanced-pattern rows.
- Synthetic invariant test: sequential independent ABI-carrier writes must not
  share a materialized binding; a self-update must continue to share it.

## 4. Risk And Ownership Check

- Existing owner: `PreviewBuilder` materialization, specifically
  `prove_same_block_register_join` and
  `same_block_prior_register_binding_name`.
- Shared analysis/substrate candidate: def-use and register reaching-definition
  facts, including call-carrier consumption.
- Extending the existing owner is sufficient because the incorrect decision is
  made while selecting the left-hand binding; no new pass or representation is
  required.
- Possible interaction: x86 same-block cmov and loop-carried update naming must
  remain unchanged; unrelated register scratch reuse must remain split.
- New owner-to-owner dependency: none.
- Telemetry impact: none expected.
- Known cases that must not change: exact self-update chains, existing cmov
  joins, and non-register materialization.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode same_block_register`
  - Expected signal: independent writes split; self-update remains joined.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no regression.
- [ ] Focused benchmark row:
  - Command: local DecBench function/project evaluation for the four affected
    rows, with stale output disabled.
  - Expected row-level improvement: the three `main` call-carrier mismatches
    disappear without new mismatches in the same project.
- [ ] Smoke or automation sample:
  - Command: representative DecBench dev rows for semantic-stress, memory
    layouts, data structures, and crypto.
  - Expected signal: affected pointer arguments remain distinct.
- [ ] Optional related checks:
  - Command: `cargo check --locked`, `cargo fmt --all --check`,
    `git diff --check`, and release CLI build.
  - Expected signal: all pass.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? [x] No
- Information exposed in an AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence: the synthetic materialization test
  and unrelated existing register-binding tests.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  [x] Confirmed.
- The change does not claim semantic improvement from synthetic tests alone:
  [x] Confirmed; the focused real-binary measurement is required before calling
  the issue fixed.
- Any new metric/pass/helper does not duplicate an existing owner: [x] Confirmed.
