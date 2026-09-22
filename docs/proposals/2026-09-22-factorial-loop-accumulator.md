# Factorial loop-carried accumulator recovery

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/math_gcc_O2.exe`
- Function: `factorial`
- Address: `0x1400016a0`
- Corpus row or benchmark command:
  `FISSION_BENCHMARK_NO_CACHE=1 FISSION_ENDPOINT=http://localhost:8007 .venv/bin/python runner/runner.py --corpus dev --function factorial --decompilers fission --run-mode local --no-resume --output results/issue99_before_8e660d6d9.json`
- Current output summary: the accumulator is initialized to `1`, the loop body is empty, and the function returns the seed instead of the loop-carried product.
- Semantic cases passed / total: `31/45` across the nine `factorial` variants; the motivating gcc `-O2` row passes `2/5` cases.
- Failure category: semantic assertion failure after successful decompilation; the output also has an empty loop body and returns the initial accumulator.
- Relevant benchmark/static/readability observations: `5/9` variants are perfect on the current local run; the motivating gcc `-O2` variant is not. The one-shot PreHIR output already omits the multiply, so this is not a printer-only defect.

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
Raw p-code in the loop contains:

  IntMult register[0x10]:8 <- register[0x10]:8, register[0x80]:8

The first register is RDX, which is seeded with 1 before the loop and copied
to RAX at the exit. The materialization trace lowers the operation to
`xVar8 = xVar8 * xVar0`, but classifies the output as `MissingMergeBinding`.
The final PreHIR contains the seed assignment and an empty loop, followed by
`return xVar12`; therefore the first wrong fact is created while the
loop-carried materialized binding is selected/retained, before normalization
or printing.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a register definition reads its own prior value, reaches a natural-loop
backedge without an intervening kill, and has an entry/preheader definition
that reaches the loop, the definition is a loop-carried scalar update. The
materializer must retain the update under the carried binding even when the
register is also ABI-capable; an ABI register slot alone does not prove that
the value is an incoming formal when a prior local definition dominates the
loop update.
```

ISA-agnostic check (ADR 0009):

- [x] Production condition is based on register def-use and natural-loop
  facts, not a function name, address, or corpus row.
- [x] Calling-convention information remains evidence about formal parameters;
  it does not replace the shared loop-carried proof.
- [x] The synthetic test will express the preheader seed, self-read update,
  backedge, and post-loop use without a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: `loop_carried_register_update_does_not_promote_prior_defined_abi_scratch`
  protects the parameter/non-parameter distinction but does not cover the
  multiply surviving through the complete materialization/structuring path.
- Similar shape 2: `loop_carried_update_reuses_post_loop_join_binding` protects
  reuse of a carried name at the exit join.
- Synthetic invariant test: add a minimal preheader-seeded multiply loop and
  assert that the materialized loop body contains the update and the exit
  reads the carried value.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `materialize::loop_carried::loop_carried_output_binding_name` and its
  reaching-definition proof.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the raw p-code and arithmetic
  lowering are already correct. The existing loop-carried proof has the needed
  natural-loop and self-read facts; the fix should make its binding selection
  honor a prior local seed rather than adding a new pass or output workaround.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass/helper is planned.
- Possible interaction with existing normalize/structuring/materialize passes:
  preserving the carried name changes the materialized loop body and exit live-in;
  normalization and structuring must then consume the retained assignment without
  changing evaluation order.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none expected.
- Known cases that must not change: genuine incoming ABI parameters must not be
  renamed as local accumulators; register updates killed before the backedge must
  remain rejected; existing AArch64/x86 loop-carried tests must remain green.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode <focused filter>`
  - Expected signal: the old implementation fails because the update is absent;
    the fixed implementation retains the multiply and exit value.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the already-known unrelated failures.
- [ ] Focused benchmark row:
  - Command: rerun the nine `factorial` dev variants with
    `FISSION_BENCHMARK_NO_CACHE=1` against the local container.
  - Expected row-level improvement: the motivating gcc `-O2` row must improve
    from `2/5`; report exact before/after cases and do not generalize from the
    synthetic test alone.
- [ ] Smoke or automation sample:
  - Command: focused factorial smoke plus the existing local smoke manifest.
  - Expected no-regression signal: all selected functions return without adapter
    or output errors and previously passing factorial variants do not regress.
- [ ] Optional related checks:
  - Command: `cargo nextest run -p fission-emulator`, `cargo check`,
    `cargo fmt --all --check`, `git diff --check`, and release CLI build.
  - Expected signal: all pass.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass/helper/dependency is planned.
  - Expected signal: not applicable.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt:
  - [ ] Structural failure pattern only
  - [ ] Owner evidence only
  - [ ] Invariant candidates only
  - [ ] Validation matrix only
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed for the planned rule.
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; benchmark evidence will be remeasured after the semantic fix.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new metric/pass/helper is planned.
