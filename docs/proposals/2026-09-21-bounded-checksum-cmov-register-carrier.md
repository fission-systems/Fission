# Bounded-checksum guarded register carrier

## 1. Baseline Row Anchor

- Binary: `advanced_patterns_gcc_O2.exe`
- Function: `bounded_checksum`
- Address: `0x140001660` (current dev manifest; the issue's older address is stale)
- Corpus row or benchmark command:
  `runner/runner.py --corpus dev --function bounded_checksum --decompilers fission --run-mode local --no-resume`
- Current output summary: the recovered `min(len, max_take)` value is split across
  unrelated names. The guarded `len` assignment is emitted as `xVar64`, while the
  later zero test is emitted as `if (xVar64)` even though that binding has no
  unconditional initialization; the loop end is also reconstructed as `len + p`.
- Semantic cases passed / total: 19 / 54 across the nine current compiler/optimization
  rows (the gcc O2 row is 0 / 6 and fails the compile gate).
- Failure category: materialize/builder register-carrier and conditional-copy merge.
- Relevant benchmark/static/readability observations: the focused no-cache run on
  commit `4f435f275` produced the existing failure pattern: gcc O2 `sim=0.318`,
  semantic `0/6`, `compile_error`; gcc O1 `1/6`, gcc/clang O0 `6/6` on the
  successful O0 rows. The output is wrong before normalize/structuring finalization.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
140001660: cmpq   %rdx,%r8
140001663: cmovaq %rdx,%r8
140001667: testq  %r8,%r8
14000166a: je     0x140001690
14000166c: addq   %rcx,%r8
```

The raw p-code expresses the same semantics: compare `len` and `max_take`,
conditionally copy `len` into `R8`, then use `R8` for the zero test and pointer
addition. The current materialized PreHIR instead emits a guarded `xVar64 = len`
and later consumes `xVar64` without a stable entry/merge carrier. The raw
instruction stream is therefore correct and the first incorrect fact is created
in materialize/register binding.

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a same-block-forward CBranch guards a register write (the p-code form of a
conditional move), the selected register-family value must have one stable
materialization binding that remains valid for later reads after the guarded body
and across the successor edge. The binding must represent the reaching selected
definition, not merely the guarded RHS or a new uninitialized temporary.
```

ISA-agnostic check (ADR 0009):

- [x] The rule is based on CFG, p-code register varnodes, and reaching
  definitions, not a function/address guard or a copied x86-only output pattern.
- [x] ISA-specific register-family data remains in the existing register namer
  and SLEIGH p-code; the materialize rule is shared.
- [x] The regression will describe the guarded register-write shape directly.

Comparable coverage:

- Similar shape 1: `x64_test_branch_preserves_register_copy_snapshot` in
  `control/terminator_tests.rs`.
- Similar shape 2: the existing materialize cmov return-register and saturating
  arithmetic tests in `materialize/mod_tests.rs` and
  `control/terminator_tests.rs`.
- Synthetic invariant test: guarded register copy followed by a successor-block
  predicate and pointer arithmetic must reuse one initialized carrier.

## 4. Risk And Ownership Check

- Existing pass/owner: `midend::builder::materialize`, specifically same-block
  cmov preservation and register-join binding selection.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] CFG / dominance / postdominance fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Extending materialize is sufficient because the existing cmov span and register
  family analyses already identify the guarded body; the missing behavior is only
  the stable carrier selection for a value consumed in a later block.
- Possible interactions: existing same-block cmov tests, register alias handling,
  and materialization replacement plans. No normalize or structuring contract
  should change.
- New owner-to-owner dependency: [x] None
- Telemetry impact: none expected.
- Known cases that must not change: existing x64/x86 cmov, partial-register, and
  return-register materialization tests; unrelated values must not be promoted to
  stable register bindings without a reaching-definition proof.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: focused `fission-pcode` materialize/control test filter
  - Expected signal: old behavior fails with an uninitialized/split carrier;
    fixed behavior uses one initialized selected-value binding.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the two known unrelated x86 tests.
- [ ] Focused benchmark row:
  - Command: no-cache DecBench `bounded_checksum` run on the same nine rows
  - Expected row-level improvement: gcc O2 output no longer loses `max_take` and
    at least the affected row's semantic/compile result improves.
- [ ] Smoke or automation sample:
  - Command: existing emulator/decompiler smoke suite
  - Expected signal: no regression in existing cmov and register-alias rows.
- [ ] Optional related checks:
  - Command: `cargo nextest run -p fission-emulator`, `cargo check`, release CLI build
  - Expected signal: all pass.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable unless the implementation adds a new helper.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; benchmark movement will be reported only after the same row is
    re-run with caches disabled.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the change extends existing materialize register-join logic.
