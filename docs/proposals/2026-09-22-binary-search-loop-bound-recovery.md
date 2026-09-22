# Binary-search loop-bound recovery

## 1. Baseline Row Anchor

- Binary: `math_gcc_O2.exe`
- Function: `binary_search`
- Address: `0x140001660`
- Corpus row or benchmark command:
  `runner/runner.py --corpus holdout --function binary_search --decompilers fission --run-mode local --no-resume`
- Current output summary: the loop initializes `hi = n - 1`, but the loop-head midpoint and the lower-bound update are rebuilt from the immutable `n` parameter.
- Semantic cases passed / total: `0/6` for the measured GCC O2 row.
- Failure category: `compile_error` (the emitted loop-bound expression does not match the source behavior).
- Relevant benchmark/static/readability observations: the issue-focused baseline measured 6 locally available variants; 5 passed and the motivating GCC O2 row failed. Three additional manifest variants were unavailable in the local fixture checkout and are not included in the denominator.
- After-change observation: the GCC O2 PreHIR/HIR now mutates the carried bound
  (`n--`, then `n = mid - 1`) and uses that value for both midpoint and lower
  bound calculations; the old output retained a separate `hi` but read the
  immutable `n` in those expressions.
- Focused after-run result: the same six-row measurement remained `5/6`
  semantic-perfect (`0.8333` mean) because both before and after still hit the
  independent NIR compile blocker `extern bool __sborrow(...)`. The row-level
  loop-bound defect itself is removed in the emitted HIR/PreHIR, but no
  DecBench score improvement is claimed from this run.

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
The raw p-code already preserves the transformed upper bound:

entry 0x140001660:
  [0002] IntSub   reg0x10:u32 <- reg0x10:u32, 1
  [0003] IntZExt  reg0x10:u64 <- reg0x10:u32

loop head 0x140001679:
  [0037] Copy     reg0:u32 <- reg0x10:u32

upper-bound update 0x14000168e:
  [0088] SubPiece reg0x10:u32 <- unique(mid - 1)
```

The raw p-code is therefore not recomputing the midpoint from `n`. The first
wrong fact is introduced by loop-carried materialization: an ABI-capable RDX
read is treated as `param_2` even though a dominating entry definition has
already changed that register into the loop's `hi` seed. The loop-head read
then shares the wrong formal binding with the later arithmetic.

## 3. Generality / Invariant Proof

Generalized rule:

```text
An ABI register slot identifies an incoming parameter only when no dominating
non-identity definition reaches the loop-body read. If the register has already
been transformed on the external path, preserve the loop-carried definition's
binding instead. Dominating definitions must be compared across register views,
so a wider or narrower alias of the same register family also counts.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] The production condition is a def-use/dominance rule, not a function,
      address, binary, compiler, or ISA guard.
- [x] ABI slot data remains supplied by the existing ABI/cspec/register model;
      the loop rule does not fork per calling convention.
- [x] The synthetic test describes a transformed ABI register seed, a wider
      register alias, and a loop-carried update without a compiler tuple.

Comparable coverage:

- Similar shape 1: direct loop-body passthrough of an entry-owned ABI register
  with no dominating redefinition must continue to resolve to `param_2`.
- Similar shape 2: an entry `IntSub` seed followed by a wider register-view
  extension must not be mistaken for the original formal when the narrow view
  is carried through a loop.
- Synthetic invariant test:
  `loop_body_parameter_passthrough_keeps_dominating_seed_definition`

## 4. Risk And Ownership Check

- Existing pass/owner that already owns this behavior: `PreviewBuilder` loop-
  carried materialization in `materialize/loop_carried/mod.rs`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: `lookup_def_site` already provides
  the reaching definition and `varnode_aliases_value` already provides the
  register-view alias relation. No new pass or state is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot
  express the invariant: not applicable; no new pass/helper/metric is added.
- Possible interaction with existing normalize/structuring/materialize passes:
  the change only prevents an incorrect formal-parameter name from being
  selected during materialization. Normalize and structuring receive the same
  loop seed they already see in raw p-code.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: direct entry-owned loop passthroughs and
  entry register aliases whose value has not been redefined.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode loop_body_parameter_passthrough --no-fail-fast`
  - Expected signal: 2 tests passed, including the existing entry-owned case
    and the new transformed-seed/wider-alias case.
  - Negative proof: removing the direct formal guard or the wider-alias proof
    makes the new test return `Some("param_2")` and fail.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Actual signal: `1087 passed, 3 failed, 1 skipped`; the three failures are
    pre-existing `diamond_join`/`movzx byte-add` assertions and are unrelated to
    loop-carried register naming.
- [x] Focused benchmark row:
  - Command: `runner/runner.py --corpus holdout --function binary_search --decompilers fission --run-mode local --no-resume` with `FISSION_BENCHMARK_NO_CACHE=1`, before and after the local Docker rebuild.
  - Actual signal: 6 local variants measured in both runs; GCC O2 changed from
    `hi = n - 1` plus midpoint/lower-bound reads from `n` to a single carried
    bound (`n--` / subsequent `n` reads). The row remains `0/6` with
    `compile_error` in both runs because of the independent `__sborrow`
    declaration; no aggregate quality gain is claimed.
- [ ] Smoke or automation sample:
  - Command: existing pcode/decompiler smoke lane after the focused rerun.
  - Expected no-regression signal: existing loop-carried parameter rows retain
    their prior outputs.
- [x] Optional related checks:
  - Command: `cargo build -p fission-cli --release`; direct `fission_cli decomp`
    on the anchored binary/function.
  - Expected signal: release build succeeds and the emitted loop uses the
    transformed bound (`param_2--` in PreHIR) for subsequent midpoint and bound
    updates. Observed on `561f4397b`.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not applicable.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending.
  - Synthetic invariant test command/result: targeted test passes; negative
    proof fails as expected when either new guard is removed.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-
  only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
