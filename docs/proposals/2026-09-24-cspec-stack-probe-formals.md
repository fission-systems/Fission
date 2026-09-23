# Decompiler Change Proposal: Cspec Stack-Probe Formal Parameters

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function: `___chkstk_ms`, called by `_pei386_runtime_relocator`
- Address: helper `0x140002810`; reported caller `0x140001c50`
- Corpus row or benchmark command: Real PE32+ binary; full `fission_cli decomp --project` output and Clang syntax check. This compiler-runtime helper is not a source-defined semantic benchmark row.
- Current output summary: The project declares and defines `long long ___chkstk_ms(unsigned long long param_1)`, while the caller emits `___chkstk_ms()`; Clang reports one missing-argument error at the call. The helper bytes read the probe size from RAX and preserve RCX/RAX around page probes. The first builder-only patch did not change this real output: tracing the production route found that `render_finish::apply_spec_overrides` loaded the resolved `.cspec` but copied only its default prototype, not its `alloca_probe` targets into `NirRenderOptions`.
- Semantic cases passed / total: N/A; no source-ground-truth case covers this linked compiler-runtime helper.
- Failure category: Incorrect formal-parameter inference for a cspec-designated special-ABI helper.
- Relevant benchmark/static/readability observations: The x86-64 Windows cspec lists `___chkstk_ms` as an `alloca_probe` callfixup target. The current call has no ordinary C arguments, and the current helper HIR's `param_1` is unused.

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
The x86-64 Windows cspec callfixup target list includes ___chkstk_ms.
`apply_spec_overrides` resolves that cspec in the production Rust-Sleigh
route, but before this change it copied only the default prototype and omitted
the alloca_probe target list already supported by `NirRenderOptions`.
Consequently `PreviewBuilder::should_suppress_entry_register_params` never
saw the ABI fact and generic Win64 entry-use inference treated the helper's
saved/restored RCX as param_1. The caller's p-code Call has only its target
input, and rendered HIR calls ___chkstk_ms() with no C arguments. The defect
therefore crossed both a missing cspec-to-options propagation and the
builder's existing entry-formal inference boundary.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When the active compiler specification identifies a function as a target of
the alloca_probe callfixup, do not infer ordinary entry-register formals for
that helper from register preservation/use. Its stack-probe interface is a
special ABI contract represented by cspec, not the platform's default C
parameter sequence. Keep this rule driven by normalized cspec target facts;
do not name a function, address, binary, or compiler tuple in production code.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production conditions use active cspec helper-target facts rather than an ISA enum or function-name allowlist.
- [x] ISA-specific helper coverage remains in cspec; no duplicate parameter-inference core is proposed.
- [x] Synthetic test proves special-target register preservation does not create a C formal, while an ordinary function with the same entry-use shape still does.

Comparable coverage:

- Similar shape 1: The checked-in x86-32 Windows `alloca_probe` target family models an implicit stack-probe register and nonstandard stack effect.
- Similar shape 2: The x86-64 Windows `alloca_probe` target family includes helpers with hidden RAX probe-size input and no ordinary C argument.
- Synthetic invariant test: A generic callfixup-target helper whose entry p-code reads/saves an ABI argument register has no inferred function parameters; the same p-code without callfixup metadata retains its ordinary formal.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `PreviewBuilder::should_suppress_entry_register_params` already controls whether entry-register uses become C formals; `NirRenderOptions::cspec_alloca_probe_targets` already defines the fact contract. The production Rust-Sleigh adapter previously failed to populate that field from its resolved `.cspec`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: The cspec parser already resolves and normalizes the alloca-probe target list. The production adapter now copies that list into the existing options contract, and the builder consults it before creating entry parameter bindings; no new ABI registry or post-render patch is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: No new pass/helper/metric is planned.
- Possible interaction with existing normalize/structuring/materialize passes: Removing the spurious formal must not discard the helper's raw register-variable expressions or stack-probe body. Other entry registers and all ordinary Win64 functions must keep their existing inference.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact, if any: None.
- Known cases that must not change: Ordinary Win64 functions with the same entry-register read pattern but without matching active cspec callfixup metadata; caller output and stack-probe effect handling.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: Focused builder test for synthetic cspec-target and ordinary-function variants.
  - Expected signal: The cspec-target helper has no ordinary formal; the same p-code without cspec metadata still recovers its ABI parameter.
- [x] Production cspec propagation test:
  - Command: `cargo nextest run -p fission-decompiler apply_spec_overrides_carries_alloca_probe_targets_into_render_options`
  - Result: The Win64 PE fixture resolves its Windows cspec and the render options receive the `alloca_probe` target list. This test failed before the propagation fix with an empty target list.
- [x] Crate-level gate (executed; not green):
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1121 passed, 3 failed, 1 skipped. The failures were `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load`; these are known from the preceding main validation, not in the changed paths. The first two were individually reproduced at base; the third was not independently baseline-tested.
- [x] Focused real-binary reproduction (not a DecBench source-semantic row):
  - Command: Re-run the real binary's `___chkstk_ms` and full `--project` decompilation, then Clang syntax check with caches/DB disabled where applicable.
  - Result: Focused HIR and two full-project renders declare/define `long long ___chkstk_ms(void)` and emit `___chkstk_ms()`. The Clang diagnostic for this helper is absent. The full translation unit remains invalid for unrelated errors (149 diagnostics in one run, 144 in the next); project declarations varied between runs, so those totals are not a stable before/after gate. No DecBench score is claimed.
- [ ] Smoke or automation sample:
  - Result: No checked-in source-semantic/smoke manifest row exists for this linked runtime helper. The local full-project output is not byte-identical across repeated runs; do not claim a broad no-regression result from it.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo build -p fission-cli --release`, `cargo fmt --all --check`, and `git diff --check`.
  - Expected signal: Builder, orchestration, release CLI, and formatting gates pass.
- [ ] Boundary audit, if a new pass/helper/dependency was added: N/A; no new pass, helper, or dependency is planned.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: N/A.
- Redaction confirmed: N/A.
- Ghidra guidance confirmed: N/A.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: No source-semantic row exists for this linked helper; do not claim a benchmark score.
  - Synthetic invariant test: Builder-level special-target and ordinary-function variants pass; production cspec propagation test passes. No source-semantic score is available for the compiler-runtime helper.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; report only the measured signature/call arity and compile-diagnostic change.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] No new metric/pass/helper is planned.
