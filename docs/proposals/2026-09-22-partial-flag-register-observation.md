# Proposal: Preserve low-lane flag values through partial-register arithmetic

## 1. Baseline Row Anchor

- Binary: `memory_layouts_gcc_O2.exe`
- Function: `manipulate_bitfields`
- Address: `0x140001530`
- Corpus row or benchmark command: release `fission_cli decomp --addr
  0x140001530 --layer hir`; issue #79 and the existing admin-path wrapper
  (`val = 101`)
- Current output summary: the function declares `long long r8` but the first
  definition of the value used in `1 + r8 + r8` is the machine instruction
  `setg %r8b`; no assignment to the emitted `r8` carrier is present.
- Semantic cases passed / total: the focused wrapper is not a numeric score; the
  admin path is expected to return `105` and set the admin bit.
- Failure category: partial-register value recovery drops a narrow flag
  definition when a wider expression reads the same low storage lane.
- Relevant benchmark/static/readability observations: raw p-code at
  `0x140001539` contains `Copy R8B <- BOOL_AND(...)`; later arithmetic reads
  `R8`/`R8D`, and the final byte store and `& 2` observe only the low lane. The
  current HIR instead contains `(1 + r8 + r8)` with no reaching definition.

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
raw p-code:
  0x140001539 Copy R8B <- BOOL_AND(...)
  0x140001549 IntAdd unique:8 <- const(1:8), R8:8
  0x140001549 IntMult unique:8 <- R8:8, const(1:8)
  ...
  0x14000155e Copy unique:1 <- R8B:1

HIR before the change:
  long long r8;
  xVar19 = (int)((int)((uint)(int)(1 + r8 + r8) | uVar15) | rax);
```

The lifter has the flag value and the failure occurs when expression lowering
tries to resolve the wider register view. The existing zero-extended partial
register proof correctly rejects this case because x86 does not clear the
upper bytes of an `r8b` write; the missing rule is the separate proof that all
observable consumers retain only the written low lane.

## 3. Generality / Invariant Proof

Generalized rule:

```text
If a narrow register definition is the latest reaching definition of the low
lane, and every same-block consumer path from that definition consists of
low-lane-preserving arithmetic/casts followed by a low-lane observation, lower
the observed value from the narrow definition. Do not widen it for a consumer
that can observe unknown upper bits, and reject live-out paths that leave the
block without such a proof.
```

ISA-agnostic check (ADR 0009):

- [x] The production rule is phrased in terms of register storage ranges,
  p-code arithmetic, and use/kill facts, not `setg`, a function name, or an
  address.
- [x] ISA-specific alias coordinates continue to come from the register-space
  model; the proof itself is shared builder logic.
- [x] The synthetic test uses a generic one-byte flag definition, arithmetic,
  low `SubPiece`, and byte store.

Comparable coverage:

- Similar shape 1: `setcc`/boolean result in a low register lane consumed by
  add/or/multiply before a byte store.
- Similar shape 2: a non-zeroing byte accumulator whose value is observed only
  through a low-byte `SubPiece` or cast.
- Synthetic invariant test: `partial_flag_register_is_recovered_when_only_low_lane_is_observed`.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `try_lower_zero_extended_partial_register` and `lower_varnode` in expression
  value recovery already handle partial register definitions when upper bytes
  are proven zero.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [ ] CFG / dominance / postdominance fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: a bounded same-block use/kill walk can
  distinguish low-lane-only observation from a genuinely wide read without
  changing p-code semantics or adding a new pass.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass is added; the helper reuses existing register-range
  matching and reaching-definition utilities.
- Possible interaction with existing normalize/structuring/materialize passes:
  only the expression source for the already-observed low lane changes; CFG,
  memory effects, and full-width reads remain conservative.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: a partial write whose upper bytes reach a
  comparison, return, pointer, call, wide store, or cross-block use must keep
  the existing unresolved/conservative path; zero-cleared partial-register
  cases keep using the existing proof.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-pcode partial_flag_register_is_`
  - Expected signal: the low-lane-only case recovers the flag expression and
    the wide-condition case remains conservative. Both passed after the fix;
    the positive test failed before the fix at the direct wide-read assertion.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the three existing baseline tests.
- [x] Focused real-binary observation:
  - Command: fresh release `fission_cli decomp --addr 0x140001530 --layer hir`
    followed by an equivalent five-case C wrapper compiled at `-O0` and `-O2`.
  - Before: the emitted `r8` carrier was undefined; the wrapper exited `2`
    at `-O0` and `1` at `-O2`.
  - After: the emitted comparison-derived value is defined; the wrapper exited
    `0` at both optimization levels.
  - The repository benchmark runner was also rerun for all nine variants. Its
    semantic rows remain `compile_error` because the existing aggregate typedef
    prelude conflict from issue #80 occurs before these wrapper cases; that is
    recorded as a harness blocker, not as evidence against this fix.
- [ ] Crate-level and smoke gates:
  - Commands: `cargo nextest run -p fission-pcode`,
    `cargo nextest run -p fission-decompiler`, `cargo check`, and a release CLI
    build.
  - Expected no-regression signal: no new failures beyond the three existing
    pcode baseline tests.
- [x] Optional related checks:
  - Command: `cargo fmt --all --check` and `git diff --check`.
  - Expected signal: clean formatting and patch.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency.
  - Expected signal: not applicable.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run yet.
  - Synthetic invariant test command/result: two focused tests passed after the
    implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; the quality claim is tied to the anchored function and its
    measured wrapper behavior.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; this extends existing expression value recovery.
