# Pointer-Valued Definition Role

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `bubble_sort`
- Address: `0x4007f0`
- Corpus row or benchmark command: external DecBench local evaluation and semantic verifier, x86 `O2`, caches disabled
- Current output summary: `xVar1 = (uint8_t *)(arr) + 4` is emitted while `xVar1` is declared `ulonglong`.
- Semantic cases passed / total: NIR `0/5`; HIR `0/5`
- Failure category: `compile_error`
- Relevant benchmark/static/readability observations: compiler rejects pointer-to-integer assignment. NIR GED/type/byte is `34/0.4/0.0`; HIR is `35/0.4/0.0`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [x] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
FISSION_NORM_TRACE=1 on the anchored row:
type_inference          changed=true  hash=<pointer state>
use_driven_type_infer   changed=true  hash=<scalar state>

The same two hashes alternate through the type fixed-point rounds. Pointer
arithmetic recovery leaves a pointer-valued definition, but use-role analysis
marks its later Add operand as scalar-only and demotes the binding.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
A local whose observed definitions are exclusively pointer-valued is not a
scalar-only local merely because the pointer later participates in address
addition/subtraction. Scalar-only restoration may demote it only when there is
no address use, no protected pointer provenance, and no pointer-valued
definition. Mixed pointer/scalar definitions remain unprotected so register
reuse can still be represented as a scalar role.
```

ISA-agnostic check:

- [x] Production condition is not gated on an ISA, calling convention, function, or address.
- [x] No ISA-specific data or control rule is added.
- [x] Synthetic test expresses typed definition/use roles only.

Comparable coverage:

- Similar shape 1: byte-pointer offset stored in a local and then added to a dynamic span.
- Similar shape 2: pointer cast stored in a local and then compared or adjusted arithmetically.
- Synthetic invariant test: an exclusively pointer-defined local used by `Add` remains pointer; an existing scalar-only pointer-cast source still demotes.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `apply_use_driven_type_infer_pass` and `restore_scalar_only_pointer_locals`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: its existing one-walk `BindingUseRole` is the canonical input to scalar-only restoration; recording definition roles there avoids a parallel pass.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass/helper/metric is added.
- Possible interaction with existing normalize/structuring/materialize passes: stops type-state oscillation for exclusively pointer-defined locals; mixed-definition locals retain current behavior.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact, if any: none.
- Known cases that must not change: pointer values used only as cast sources for a separate pointer local, mixed scalar/pointer definitions, explicit surface types, and direct address-use protection.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize pointer_defined_local_with_scalar_address_arithmetic_stays_pointer scalar_only_local_pointer_constraint_converges_once`
  - Expected signal: passed `2/2`; new pointer-definition protection and existing scalar-only demotion are both green.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize && cargo nextest run -p fission-pcode`
  - Expected signal: normalize passed `297/297`; pcode passed `991/991` with one skipped test.
- [x] Focused benchmark row:
  - Command: same local decompile, semantic verifier, and DecBench row with caches disabled
  - Expected row-level improvement: `xVar1` changed from `ulonglong` to `ulonglong *`, removing its pointer-to-integer assignment error. Semantic remains NIR/HIR `0/5` compile error at the next incompatible `longlong *rax = int *arr` assignment. DecBench remains NIR `34/0.4/0.0`, HIR `35/0.4/0.0`.
- [x] Smoke or automation sample:
  - Command: external DecBench x86 O2 18-function sample
  - Expected no-regression signal: NIR and HIR each produced `18/18`, zero errors, GED0 `2`, Type1 `1`, Byte1 `0`, Union `3`; unchanged from baseline.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize -p fission-pcode -p fission-decompiler` and release CLI build
  - Expected signal: checks and release CLI build passed.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not required; existing role analysis is extended.
  - Expected signal: n/a.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: none.
- Redaction confirmed: not applicable; no external model prompt.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: external x86 O2 18-function sample after implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
