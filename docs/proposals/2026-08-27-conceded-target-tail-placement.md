# Preserve live targets of conceded structuring edges

## 1. Baseline Row Anchor

- Binaries/functions/addresses:
  - `bin_033.elf:main @ 0xf390`
  - `bin_075.elf:sub_7e00 @ 0x7e00`
  - `bin_129.elf:sub_14ea0 @ 0x14ea0`
- Corpus command: direct static `fission_cli decomp --prehir` plus the stripped
  eval-kit sample-set gate.
- Current output: `dream_L9`, `dream_L84`, and `dream_L5` are referenced by
  15, 1, and multiple gotos respectively, but none is declared in the emitted
  body. The target block statements are absent as well.
- Semantic cases passed / total: not the failure surface; the emitted C has an
  unresolved control-flow target.
- Failure category: live basic block lost during structuring placement.
- Baseline gate: HIR GED 60/232 perfect, NIR GED 59/232; HIR and NIR types
  14/222 perfect.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
FISSION_PREVIEW_DIAG=1 shows concessions into nodes 9, 84, and 5.
The corresponding goto survives in NIR, HIR, and the PreHIR diagnostic body,
but no matching Label survives and the target statements are not emitted.
```

Label cleanup already protects referenced labels recursively. The transfer and
label are created by `reaching_driver::concede_one_edge`; final placement is
therefore a structuring ownership obligation, not a printer concern.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Every materialized goto created by edge concession must have exactly one
matching label in the emitted body, and the live target node's statements must
be placed exactly once. If structural assembly cannot claim that live node,
append the labeled node at the tail rather than deleting the transfer or body.
```

ISA-agnostic check:

- [x] The rule depends only on live CFG node ownership and explicit transfers.
- [x] No ISA/compiler information enters the rule.
- [x] Synthetic coverage will use a small CFG with all incoming edges
  virtualized and a live target body.

Comparable coverage:

- Similar shape 1: many conceded incoming edges share one target.
- Similar shape 2: one conceded forward edge targets a block not claimed by the
  entry-rooted structured body.
- Synthetic invariant test: every emitted goto is paired with one label and
  the target side effect appears once.

## 4. Risk And Ownership Check

- Existing owner: `reaching_driver.rs` and its `CollapseGraph` node bodies.
- Shared substrate: CFG reachability and block ownership.
- Extend final assembly/placement in the existing driver if instrumentation
  confirms an unclaimed live target; do not add a cleanup that deletes gotos.
- Interaction risk: appending a tail changes emitted CFG and can move GED in
  either direction; duplicated target statements would be semantically wrong.
- New owner dependency: none.
- Telemetry impact: diagnostic-only unless an existing counter applies.
- Must not change: fully claimed nodes, already-declared labels, and declined
  DREAM candidates.

## 5. Validation Matrix

- [ ] Instrument collapse completion to confirm live/unclaimed target state.
- [ ] Targeted structuring invariant test.
- [ ] `cargo nextest run -p fission-midend-structuring` and
  `cargo nextest run -p fission-pcode`.
- [ ] Focused rerun of all three rows; compare `bin_033:main` target statements
  with static `objdump` around the target block.
- [ ] Sweep all 250 outputs: every goto has a matching label.
- [ ] Full `decbench_sample_set_gate.py check`: no GED/type-perfect drop.
- [ ] `cargo fmt --all --check`.

## 6. AI Review / Prompt Firewall

- [x] No separate model was asked for implementation advice.
- [x] Production code will contain no binary/function/address/corpus guards.
- [x] Validation includes a synthetic invariant and the measured rows.

## 7. Review Notes

- A result that only removes dangling gotos is invalid because it still loses
  the target block.
- Quality claims require the measured full gate; test-only success is reported
  only as mechanical coverage.
