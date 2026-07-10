# X86-32 Incoming Stack Parameter Recovery

## 1. Baseline Row Anchor

- Binary: `corpus/dev/binaries/control_flow_gcc-m32_O0.exe`
- Function: `count_bits` (representative; the same signature failure affects all sampled x86-32 rows)
- Address: `0x4015b0`
- Corpus row or benchmark command: `FISSION_ENDPOINT=http://localhost:8007 python runner/runner.py --corpus dev --decompilers fission --limit 10 --output results/local_85867ecd.json`
- Current output summary: the body reads `param_8`, but the function is emitted as `uint _count_bits(void)`.
- Semantic cases passed / total: 0 / 6
- Failure category: `compile_error` (`too many arguments to function call, expected 0, have 1`)
- Relevant observations: all 20 sampled `gcc-m32` rows fail compilation; the common signature symptom is missing formal parameters while positive EBP stack slots are present in HIR.

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
uint _count_bits(void) {
    ...
    uint param_8;
    ...
    if (param_8) ...
}

PreviewBuilder::run_incremental_heritage classifies the EBP+8 slot as a local
StackOffset. build_hir then serializes it through HirFunction.locals, so the
printer correctly emits an empty formal parameter list from the wrong builder
fact.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
For the x86-32 stack calling convention, incoming argument slots are ordered
pointer-sized locations above the return-address/frame-record boundary. With an
EBP frame, slot 0 starts at EBP + 2 * pointer_size. With an ESP-relative frame,
slot 0 starts at ESP + stack_frame_size + pointer_size. Aligned slots at or
above that boundary are formal parameters, not locals.
```

Comparable coverage:

- Similar shape 1: one incoming EBP-relative scalar argument.
- Similar shape 2: three ordered EBP-relative arguments, including pointer-typed uses.
- Synthetic invariant test: x86-32 EBP+8/12 classify as parameter slots 0/1; EBP+4, negative offsets, x64, and non-x86-32 conventions do not.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `AbiState` stack-slot classification and `PreviewBuilder` incremental stack-slot materialization.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [x] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the builder already resolves EBP/ESP-relative addresses and owns both parameter and local binding tables.
- If adding a new pass/helper/metric: no pass or metric is added; one ABI query and one builder binding helper extend existing owners.
- Possible interaction: stack type hints and later normalize passes will see `ParamIndex` instead of `StackOffset`; outgoing call-argument slots must remain locals/outgoing slots.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact: none.
- Known cases that must not change: x86-64 register parameters/home slots, x86-32 outgoing call arguments, negative local offsets, ARM/MIPS/PPC stack behavior.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode x86_32_incoming_stack --no-fail-fast`
  - Result: 2 / 2 passed, including the frameless-EBP negative case.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1133 passed, 9 skipped.
- [x] Focused benchmark row:
  - Command: rerun the local `gcc-m32 -O0` representative rows with SHA provenance and no stale result cache.
  - Result: `count_bits` gcc-m32 O0 moved from compile error 0/6 to runtime error 1/6; its signature changed from `fn(void)` to one formal parameter. GCC x64 O0/O2 stayed 6/6.
- [x] Smoke or automation sample:
  - Command: rerun the first 10 dev functions across compiler variants into a local-only result path.
  - Expected no-regression signal: existing GCC x64 O0 pass rows remain pass.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode && cargo check -p fission-decompiler`
  - Result: both checks passed.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: no new owner-boundary violation.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external model; this local implementation follows the repository skill and ADR gate.
- Information exposed in the AI prompt:
  - [x] Structural failure pattern only for implementation reasoning
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction confirmed for external prompts:
  - [x] Function names removed
  - [x] Addresses removed
  - [x] Binary paths removed
  - [x] Corpus row ids removed
  - [x] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending after focused validation.
  - Synthetic invariant test command/result: 2 / 2 targeted tests passed.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
