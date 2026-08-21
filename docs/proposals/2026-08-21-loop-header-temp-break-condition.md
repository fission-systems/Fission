# Loop-header temporary break-condition promotion

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_027.elf`
  (findutils O0)
- Function: `prec_name`
- Address: `0xea2e`
- Corpus row or benchmark command: DecBench sample-set NIR output followed by
  `decbench.metrics.ged.GEDMetric` at DecBench HEAD `d9f4f8a`
- Current output summary: an unconditional `while (1)` first loads a table
  field into `xVar90`, then immediately exits through `if (xVar90 == -1)`.
- Semantic cases passed / total: not available in the public evalkit; the
  production rule is covered by a synthetic evaluation-order invariant test.
- Failure category: loop-header condition left as a one-use temporary plus a
  nested `break` shell.
- Relevant benchmark/static/readability observations: current GED is `4`
  (`5` nodes / `8` edges versus source `5` / `6`). Moving the one-use load into
  the loop condition, without duplicating it, scores GED `0` (`5` / `6`).

Two repeated anchors were measured with the same DecBench HEAD scorer:

- `bin_124.so`, `selinux_check_securetty_context`, libselinux O2-noinline:
  GED `22 -> 20` on the same emitted body with only this rewrite reversed.
- `bin_126.elf`, `xcscmp`, `0x3350`, libexpat O2-noinline: GED `13 -> 9`.

The new output is better without reference to the metric: a computation whose
only consumer is the loop's leading exit test is expressed as the loop
condition, rather than as a disposable local followed by an unconditional loop
and nested break.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [x] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
while (1) {
    xVar90 = *(ushort *)((local_4 << 4) + 300192);
    if (xVar90 == -1) {
        break;
    }
    ...
}
```

The CFG is already structured and the remaining defect is a local loop-condition
temporary. `cleanup/loops_conds.rs` already owns the symmetric do-while trailing
condition-temp inlining and is therefore the canonical owner.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For an unconditional while whose first two statements are
  temporary = side-effect-free expression;
  if (condition_using_only_that_temporary) break;
promote the negated exit condition to the while condition and substitute the
RHS exactly once, only when all reads of the temporary are in that exit
condition and the RHS neither calls nor self-references the temporary.
```

This preserves the expression's evaluation count, order, and fault point on
normal entry, loop backedges, and `continue`; it does not duplicate a load.

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data remains outside the rule.
- [x] Synthetic test states only the AST/dataflow shape.

Comparable coverage:

- Similar shape 1: libselinux O2-noinline whitespace scan (`bin_124.so`).
- Similar shape 2: libexpat O2-noinline `xcscmp` (`bin_126.elf`).
- Synthetic invariant test: load-backed one-use header temp is promoted, while a
  temp read after the guard and a side-effecting call RHS are rejected.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `inline_loop_condition_trailing_temps` in `cleanup/loops_conds.rs`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: it already computes function-wide
  `DefUseMap` read counts and rewrites loop condition temporaries.
- If adding a new pass/helper/metric: an owner-local helper extends the existing
  registered pass, which is re-run after final rule normalization exposes
  compact header chains. No new semantic owner, dependency, or metric is added.
- Possible interaction: later IV recovery sees the simpler loop condition;
  existing cleanup may remove the now-dead local binding.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact: existing pass firing only; no new telemetry contract.
- Known cases that must not change: side-effecting calls, a header temporary read
  outside the owned prefix/condition, an unresolved self-reference without a
  prior prefix definition, non-empty else arms, and multi-statement break arms.
  The live post-loop index in `bin_133.elf` is a measured rejection and remains
  unchanged.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize loop_header`
  - Result: 4/4 passed; safe single-load and p-code-chain promotion passed,
    while live-after-guard and side-effecting-call cases remained unchanged.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize`
  - Result: 328/328 passed.
- [x] Focused benchmark row:
  - Command: rebuild release CLI, decompile bins 027/124/126 with cache-disabled
    sample-set runner inputs, and recompute GED with DecBench HEAD.
  - Result: `4 -> 0`, `22 -> 20`, `13 -> 9`.
- [x] Smoke or automation sample:
  - Command: full 250-function NIR and HIR sample-set sweeps.
  - Result: both layers decompiled 250/250 functions. NIR remained at 1119
    gotos and changed `33,306 -> 33,190` lines; HIR remained at 1116 gotos and
    changed `30,931 -> 30,821` lines. Ten binaries changed.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize && cargo check -p fission-pcode`
  - Result: both passed; `cargo nextest run -p fission-pcode` also passed
    1002/1002 tests with one skipped.
- [ ] Boundary audit:
  - Command: not required; no new pass/helper dependency crosses an owner boundary.

The external Docker benchmark remains required before an official ranking claim;
the local OrbStack daemon was unavailable at proposal time.

Old/new CLIs built from `70a2ead34` and this patch were compared at every
changed address. DecBench HEAD GED results were: `prec_name 4 -> 0`, three
independently built ARM reset handlers `27 -> 3` each, securetty `22 -> 20`,
`xcscmp 13 -> 9`, shadow login `112 -> 110`, and rtmon `83 -> 79`.
`console_getc` remained 17. The FreeRTOS source fragment parsed as a degenerate
one-node CFG in standalone Joern, so it was excluded from the exact local GED
comparison; its published baseline was already non-perfect. No measured
previously-perfect row was lost.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model review was used.
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable; all investigation stayed local.
- Ghidra guidance confirmed:
  - [x] Reference-only; no production dependency or style-copy request.
- Unseen or synthetic validation evidence:
  - Synthetic invariant tests in `cleanup/passes_tests.rs`.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
