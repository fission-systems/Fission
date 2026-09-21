# Known indirect-call ABI and result-carrier recovery

## 1. Baseline Row Anchor

- Binary: `win32_status_gcc_O2.exe`
- Function: `wait_for_one`
- Address: `0x1400015f0`
- Corpus row or benchmark command:
  `fission-benchmark` dev corpus, `--function wait_for_one --decompilers fission`
- Current output summary: the HIR emits `WaitForSingleObject()` without its
  `handle` and `ms` operands, then overwrites the return surface with `1`
  before comparing it with `258`.
- Semantic cases passed / total: `no_wrapper` for the seven variants in the
  current DecBench dev run; no wrapper cases are available for this row, so the
  artifact is used as a real-binary decompilation baseline rather than a scored
  semantic case.
- Failure category: `no_wrapper` / real-output correctness defect.
- Relevant benchmark/static/readability observations: the baseline artifact is
  `/Users/sjkim1127/fission-benchmark/results/issue106_before_b3c923bf0.json`.
  The O2 output contains `WaitForSingleObject()` with zero arguments and tests
  the constant `1` against `258`.

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
The raw p-code contains a CallInd whose target is an IAT load resolved as
KERNEL32.dll!WaitForSingleObject. The call has no p-code output, while the
Windows x64 incoming RCX/RDX values remain live because no output definition
overwrites them. call_recovery.rs only accepts declaration-locked exact arity
for PcodeOpcode::Call, so the known indirect call cannot use its two-parameter
prototype to recover those live slots.

After the call, p-code copies EAX to the saved status carrier, writes RAX=1
for the timeout return arm, and joins at a shared epilogue. The diagnostic
trace first sees the constant return value, but the later shared return join
selects the stale call-result/status carrier. The result-carrier liveness rule
therefore does not consistently honor a post-call primary-return-register
definition across predecessor paths.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
An indirect call may use declaration-locked register arity when its target is
resolved by the existing target-resolution facts (for example an IAT load) and
the prototype summary is exact. This proves which ABI register slots are call
operands without guessing arguments for unresolved indirect calls.

For a primary return-register read, a call-result carrier is live only until a
reaching definition that aliases the same register family. At a join, every
predecessor must provide the same live call result, and a post-call register
definition must outrank the call carrier on that path.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is based on resolved call/prototype and register
      def-use facts, not the function name, address, or binary.
- [x] Calling-convention data remains in existing ABI/cspec/register-namer
      models; the proof is shared by direct and resolved indirect calls.
- [x] Synthetic tests can state the indirect-call and post-call redefinition
      shapes without requiring a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: x64 IAT-backed `CallInd` with live incoming register args.
- Similar shape 2: a call result copied to a non-return register before a later
  write to the primary return register and a shared epilogue join.
- Synthetic invariant test: known exact-arity indirect calls recover only the
  declared ABI slots, and a later primary-register definition is selected over
  the call-result carrier.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `calls/call_recovery.rs`
  owns call-argument proof; `expr/lower_expr.rs` owns primary return-register
  reaching-value selection; `materialize/call_results.rs` owns call-result
  binding registration.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed:
  extend the existing exact-arity proof to resolved `CallInd` targets and make
  the existing call-carrier liveness check respect the same reaching-definition
  ordering. No new pass or parallel fact map is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: not applicable.
- Possible interaction with existing normalize/structuring/materialize passes:
  the change affects only recovered call operands and the expression selected
  for a primary return-register read; CFG and later normalization contracts are
  unchanged.
- New or changed owner-to-owner dependency:
  - [ ] None
  - [x] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact, if any: none.
- Known cases that must not change: unresolved indirect calls must not gain
  guessed arguments; direct-call exact-arity recovery, x86-32 stack recovery,
  and call-result bindings for genuinely live return values must remain intact.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: focused `fission-pcode` nextest filters for call recovery and
    return-carrier liveness.
  - Expected signal: the old implementation loses the indirect operands or
    selects the stale carrier; the fixed implementation preserves the declared
    operands and post-call return definition.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the two already-known unrelated
    lower-expression tests.
- [x] Focused benchmark row:
  - Command: external local fission-benchmark dev run for `wait_for_one`, with
    cache disabled.
  - Expected row-level improvement: the call has two ABI operands and the
    status comparisons/return mapping consume the call result instead of a
    constant or stale carrier.
- [ ] Smoke or automation sample:
  - Command: existing fission-benchmark dev function sample.
  - Expected no-regression signal: unchanged behavior on the neighboring
    Windows status variants and no new compile/decompilation failures.
- [x] Optional related checks:
  - Command: `cargo nextest run -p fission-emulator`, `cargo check`, release
    CLI build, `cargo fmt --all --check`, and `git diff --check`.
  - Expected signal: all pass, subject only to documented pre-existing tests.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: not applicable; no new pass or dependency is planned.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt:
  - [x] Structural failure pattern only
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction confirmed:
  - [ ] Function names removed
  - [ ] Addresses removed
  - [ ] Binary paths removed
  - [ ] Corpus row ids removed
  - [ ] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending.
  - Synthetic invariant test command/result: pending.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
