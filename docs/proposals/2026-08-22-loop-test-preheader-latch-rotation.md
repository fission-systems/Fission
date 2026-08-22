# Rotate A Loop Test Into A Preheader And Latch

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_174.elf`
- Function: `statdb_write`
- Address: `0x4800`
- Corpus row or benchmark command: release sample-set NIR, scored with
  `vendor/decbench` at `d9f4f8a` and the official serialized source CFG.
- Current output summary: the match-fold `WhileDo` lowering emits
  `while (1) { rax = next(...); if (!rax) break; body; }`.
- Semantic cases passed / total: no concrete behavior oracle is available for
  this row; the local decompilation succeeds and the published Fission output
  recompiles.
- Failure category: a top-tested loop whose test block contains one statement
  is represented as an infinite loop plus a manual exit.
- Relevant benchmark/static/readability observations: source CFG 4 nodes / 4
  edges, current NIR/HIR 4 / 6, exact GED 4. Glaurung represents the same
  evaluation order as an initialization, `while (value)`, and latch update.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [x] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
fission-midend-structuring::collapse_driver::lower_shape,
ShapeKind::WhileDo:

if test_stmts.is_empty() { emit While(cond) }
else { emit while (true) { test_stmts; if (!cond) break; body } }

The same schedule can be represented without a synthetic guard as
`S; while (cond) { body; S; }` when the body has no `continue` targeting this
loop.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a graph-proven WhileDo region whose test node lowers to a non-empty linear
statement sequence S that contains an evaluation whose schedule must remain
explicit (a non-pure call, memory observation, or observable write), emit
`S; while (cond) { B; S; }` when B contains no `continue` targeting this loop.
S executes once before the first condition and once after every normal body
path. Break, return, and outward-goto paths do not execute S again in either
representation. Nested-loop continues are unrelated and do not block the
rewrite. Pure arithmetic/flag scaffolding stays on the existing path, where
cleanup can fold it into the loop condition without duplicating it.
```

ISA-agnostic check:

- [x] Production condition is a structured-CFG and typed-statement invariant.
- [x] No ISA-specific data is introduced.
- [x] Synthetic tests use abstract statements and conditions.

Comparable coverage:

- Similar shape 1: dpkg O2 `statdb_write`, call assignment in the test node.
- Similar shape 2: libselinux O2-noinline row, `getline` assignment test.
- Similar shape 3: gnutls O2 and O2-noinline rows, call assignment tests.
- Additional measured shapes: coreutils O0, libopencm3 O0 ARM, and betaflight
  O2 ARM.
- Synthetic invariant test: the preheader and latch contain the same ordered
  test sequence, with no manual break guard; a loop-scoped continue rejects.

The current sample-set census found seven functions across six projects,
O0/O2/O2-noinline, x86-64 and ARM.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: match-fold
  `ShapeKind::WhileDo` lowering. Normalize's for-loop recovery only moves an
  already-separated initialization/update around an existing `While`; it does
  not own graph-node lowering.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: `ShapeKind::WhileDo` has already
  proven the test node, body node, follow, and branch direction. Only emission
  changes.
- If adding a new pass/helper/metric: private owner-local rotation and
  loop-scoped-continue helpers keep admission directly testable; they add no
  pass, analysis, or dependency.
- Possible interaction: the existing structuring comparator will still decide
  whether the whole isolated candidate replaces the baseline.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact: none.
- Known cases that must not change: empty test blocks remain `while (cond)`;
  a current-loop `continue` retains the manual-guard representation; a
  non-linear test sequence is not duplicated; a pure arithmetic/flag-only test
  stays on the existing path so it can become a plain natural `while`.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-structuring -E 'test(while_test_preheader_latch)'`
  - Expected signal: an ordered linear test rotates; a current-loop continue
    and a pure/control-only test sequence reject.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-structuring -p fission-pcode`
  - Expected signal: all tests pass.
- [x] Focused benchmark row:
  - Command: release decompile `bin_174.elf@0x4800`, exact current GED scorer.
  - Expected row-level improvement: no `while (1)`/manual break pair; one
    structured loop test; exact GED 4 to 0.
- [x] Smoke or automation sample:
  - Command: regenerate all 250 NIR and HIR sample-set functions and compare
    exact topology, per-row perfect status, and goto counts.
  - Expected no-regression signal: no existing exact row lost, no goto growth.
- [x] Optional related checks:
  - Command: concrete PreHIR/HIR differential tests including `continue`.
  - Expected signal: preheader once, latch after normal completion, not after
    continue/break/return.
- [ ] Boundary audit: not applicable; no new pass or dependency.

Official leaderboard publication remains gated on a DecBench submission.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external model was asked.
  - [ ] Yes, using the review template.
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: reference code was not copied and no style mimicry
  was requested.
- Unseen or synthetic validation evidence: owner-level shape tests plus the
  full fixed-denominator NIR/HIR sweep.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the private constructor belongs to the existing lowering.

Measured outcome on the fixed 250-function sample set:

- NIR and HIR both decompiled 250/250 functions.
- NIR `goto` count stayed 1119 -> 1119; HIR stayed 1116 -> 1116. No
  individual output file gained a `goto`.
- The synthetic `while (1)` plus manual-break representation fell by six in
  each layer (NIR 136 -> 130, HIR 128 -> 122).
- Among the 240 rows whose published source CFG could be rebuilt locally,
  exact-isomorphism perfect rows moved NIR 49 -> 51 and HIR 50 -> 52, with no
  perfect-row loss in either layer.
- The two new exact rows are `env_free` and `statdb_write`, from different
  projects. Both moved from 4 nodes / 6 edges to the source's 4 / 4 topology.
- Two changed HIR files lack a published source CFG in the current manifest;
  direct old/new output comparison found their CFGs isomorphic, so their
  topology did not change.

These are local sample-set measurements, not a published leaderboard result.
