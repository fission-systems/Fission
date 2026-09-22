# Factorial loop-carried accumulator recovery

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/math_gcc_O2.exe`
- Function: `factorial`
- Address: `0x1400016a0`
- Corpus row or benchmark command:
  `FISSION_BENCHMARK_NO_CACHE=1 FISSION_ENDPOINT=http://localhost:8007 .venv/bin/python runner/runner.py --corpus dev --function factorial --decompilers fission --run-mode local --no-resume --output results/issue99_before_8e660d6d9.json`
- Current output summary: the accumulator is initialized to `1`, the loop body is empty, and the function returns the seed instead of the loop-carried product.
- Semantic cases passed / total before the fix: `31/45` across the nine `factorial` variants; the motivating gcc `-O2` row passes `2/5` cases.
- Failure category: semantic assertion failure after successful decompilation; the output also has an empty loop body and returns the initial accumulator.
- After `afe75f519`, the same cache-disabled run is `37/45` cases and `7/9`
  perfect variants; the motivating gcc `-O2` row is `5/5`. The x86-64 gcc `-O3`
  row also moves from `2/5` to `5/5`; the x86-32 failures are unchanged.
- Type match remains `9/9`; GED perfect rows remain `4/9`, while mean GED moves
  from `4.6667` to `6.0`. This is recorded as a semantic-correctness repair,
  not a structural/readability improvement. The scored DecBench surface is
  NIR (`pseudocode_layer: nir`); the separate HIR presentation still needs a
  follow-up audit.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/control:
- [ ] Normalize:
- [x] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Raw p-code in the loop contains:

  IntMult register[0x10]:8 <- register[0x10]:8, register[0x80]:8

The first register is RDX, which is seeded with 1 before the loop and copied
to RAX at the exit. The materializer already lowers the operation to
`xVar8 = xVar8 * xVar0`, so SLEIGH and arithmetic materialization are correct.

The shared exit contains `RAX <- RDX; Return`. The old
`return_join_source_register` scan started before the current primary-return
definition, so it skipped that direct copy and returned no edge source. The
structurer consequently rewrote the loop edge to `break`; its fallback
condition-prefix fold then treated the pure multiply as unobservable, and the
shared return block's dominance lookup selected the entry seed `xVar12`.
The earliest emitted semantic loss is therefore structuring's discard of the
loop-head multiply, with builder return-join recovery being the missing proof
that allowed the loss.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Dominance is insufficient for a value selected at a shared return join:
different incoming edges may carry different definitions. For a proven simple
primary-return copy/extension, inspect the current definition and follow its
primary-return alias chain. When a natural-loop condition head exits directly
to that join, retain the edge-specific return expression and do not fold away
the condition-head prefix that computes it. If the source cannot be proven,
keep the existing conservative break/fold path.
```

ISA-agnostic check (ADR 0009):

- [x] Production condition is based on register def-use and natural-loop
  facts, not a function name, address, or corpus row.
- [x] Calling-convention information remains evidence about formal parameters;
  it does not replace the shared loop-carried proof.
- [x] The synthetic test expresses a shared return join with two incoming
  register definitions and checks the recovered edge value without a compiler
  tuple, function name, or address guard.

Comparable coverage:

- Direct regression: `x64_return_join_copy_uses_edge_source_register` fails
  on the old scan (`None`) and passes after the fix (`Const(2, u64)`).
- Related coverage: `x64_loop_direct_epilogue_return_exit_keeps_predecessor_value`
  protects a loop edge from inheriting a sibling sentinel return.
- Real-binary anchor: the cache-disabled factorial matrix verifies that the
  repaired edge returns the carried product on the motivating row.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `PreviewBuilder::return_join_source_register` in
  `builder/control/terminator.rs`, and
  `fission-midend-structuring::loops::try_lower_while_impl` for preserving the
  proven edge return.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the raw p-code and arithmetic
  lowering are already correct. The existing return-join and loop-structuring
  owners already expose the required CFG/ABI/def-use facts; the change adds no
  new pass and no output workaround.
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

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(x64_return_join_copy_uses_edge_source_register)' --no-fail-fast`
  - Result: fixed implementation passes. Temporarily restoring the old scan
    fails with `left: None`, `right: Some(Const(2, Int { bits: 64, signed: false }))`.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: `1086 passed, 3 failed, 1 skipped` (the three existing failures are
    `diamond_join_lowers_copy_through_join_read_as_select`,
    `movzx_after_byte_add_zero_extends_unsigned`, and
    `x64_byte_add_movzx_does_not_double_add_load`). The new return-join test
    and the structuring-loop suite pass.
- [x] Focused benchmark row:
  - Command: rerun the nine `factorial` dev variants with
    `FISSION_BENCHMARK_NO_CACHE=1` against the local container.
  - Result: `results/issue99_before_8e660d6d9.json` →
    `results/issue99_after_afe75f519.json`; `31/45 → 37/45` semantic cases,
    `5/9 → 7/9` perfect variants, and gcc `-O2` `2/5 → 5/5`.
- [x] Smoke or automation sample:
  - Command: dev smoke with `--limit 20 --variant-limit 1`, caches disabled.
  - Result: `results/issue99_smoke_after_afe75f519.json`; all 20 requested
    rows returned without adapter/output errors. Existing compile/assertion
    failures remain explicit in the artifact.
- [x] Optional related checks:
  - Command: `cargo nextest run -p fission-emulator`, `cargo check`,
    `cargo fmt --all --check`, `git diff --check`, and release CLI build.
  - Result: emulator `200 passed, 3 skipped`; workspace `cargo check`, format,
    diff check, and `cargo build -p fission-cli --release` all pass.
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
  - Patch validation pool command/result: not run in this focused issue pass.
  - Synthetic invariant test: completed by the direct return-join regression
    above.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed for the planned rule.
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; the semantic claim is backed by the same cache-disabled
    DecBench factorial matrix before and after the code change.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new metric/pass/helper is planned.
