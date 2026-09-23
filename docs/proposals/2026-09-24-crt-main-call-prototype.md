# Preserve the C runtime `main` call contract in project output

## 1. Baseline Row Anchor

- Binary: `advanced_patterns_gcc_O1.exe` from the external dev corpus.
- Functions: `__tmainCRTStartup` at `0x140001180`, application `main` at
  `0x140001664`.
- Issue: GitHub #68; reproduce with `fission_cli decomp <binary> --project`
  piped to `clang -x c -std=gnu17 -fsyntax-only -ferror-limit=0 -`.
- Before any change, startup emitted `main(argc, rbp, envp)` while the
  definition and declaration were `int main(void)`. Clang reported
  “too many arguments to function call, expected 0, have 3”.
- The first project-only arity experiment removed that diagnostic but emitted
  `int main(undefined, undefined, undefined)`. Clang then rejected all three
  `main` parameter types and the two integer-to-pointer call arguments. This
  proves argument count alone is not a usable C callback prototype.
- Project function count: 73. Baseline project command: about 0.79 s wall;
  arity pre-analysis experiment: about 1.65 s wall. These are single-run
  directional measurements, not a controlled benchmark.
- DecBench does not exercise `--project` (its decompile adapter requests
  individual functions); no leaderboard or source-semantic score claim is
  applicable to this fix.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery and project call-prototype assembly
- [ ] Printer
- [ ] Benchmark/automation

The raw call and all three ABI arguments are already recovered. The first
missing fact is the callee's callback prototype: project rendering widens
parameters from observed calls, but those new slots have no type. The caller
also receives no call-prototype summary from typed `FactStore` hints, so it
cannot lower the two pointer arguments to C-compatible expressions. Neither
the lift nor output-only declaration merging owns these facts.

## 3. Generality / Invariant Proof

```text
For a project-wide, direct call to the C program entry symbol `main`, use the
observed direct-call arity to select the documented two- or three-argument
entry prototype. Keep this ABI contract in function facts, transport typed
function facts into internal call summaries, and express an integer/unknown
actual value passed to a specifically typed object-pointer formal as an
explicit `void *` conversion at that call only. Do not change the source
binding or its other uses.
```

The use of the standard C entry symbol is a language/runtime contract, not a
binary, address, compiler-tuple, or corpus-row guard. Other functions continue
to receive only the existing structural arity hint. Existing user signatures
and debug information remain higher priority. The conversion is limited to a
typed object-pointer formal and a non-pointer scalar/unknown actual; function
pointer contracts and already-pointer actuals are left untouched.

Comparable coverage:

- Existing ELF `__libc_start_main` callback facts type the proven callback's
  parameter and return slots without relying on instruction mnemonics.
- Existing internal callee summaries already transport typed pointer facts;
  the missing piece here is trusted `FactStore` type-hint transport plus the
  call-boundary conversion for a scalar-shaped actual.
- Synthetic invariant coverage: direct internal call with three recovered
  ABI arguments to `main`, with the latter two passed as integer-shaped values.

## 4. Risk And Ownership Check

- Existing owners: `FactStore`/`NirFunctionHints`, internal call-prototype
  assembly, and the existing call-site type-propagation owner.
- Shared analysis substrate: existing direct-call target and observed arity
  facts; no new pass or owner-to-owner dependency is needed.
- The `void *` cast is call-local. It cannot retype a caller local/global or
  change pointer arithmetic elsewhere in the caller.
- The call-prototype builder carries explicit pointer-pointee evidence into
  PreHIR. Only that trusted pointer contract may authorize a call-local cast
  when an actual also has a scalar surface type; generic direct-callee pointer
  inference continues to preserve the caller's explicit surface type.
- Non-project output remains outside the whole-program arity prepass. A
  project call with no typed object-pointer contract remains unchanged.
- This resolves the `main` declaration/call mismatch only; the measured
  translation unit has other pre-existing compile diagnostics, which will be
  reported separately rather than claimed fixed here.
- Telemetry: none.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - `cargo nextest run -p fission-midend-normalize typed_object_pointer_call_contract_casts_scalar_actual`
  - A scalar actual gets one call-local pointer cast; pointer actuals and
    unrelated calls remain unchanged; second pass is idempotent.
- [x] CLI integration regression:
  - `cargo nextest run -p fission-cli project_main_call_keeps_runtime_prototype`
  - Project facts preserve the observed two/three-argument `main` contract and
    the rendered direct call uses C-compatible pointer actuals.
- [x] Combined crate-level regression run:
  - `cargo nextest run --no-fail-fast -p fission-pcode -p fission-midend-normalize -p fission-decompiler -p fission-cli`
  - 1,715 passed (1 leaky), 3 failed, 1 skipped. The three failures are builder tests
    independently reproduced with identical output at clean `HEAD`
    `109e09b43`: `diamond_join_lowers_copy_through_join_read_as_select`,
    `movzx_after_byte_add_zero_extends_unsigned`, and
    `x64_byte_add_movzx_does_not_double_add_load`.
- [x] `CARGO_BUILD_JOBS=1 cargo check --workspace`
- [x] `CARGO_BUILD_JOBS=1 cargo nextest run -p fission-cli` — 96 passed.
- [x] Focused real-binary check:
  - Re-run project output for the anchored binary through Clang.
  - Require the old too-many-arguments diagnostic, `main` parameter-type
    diagnostics, and pointer/int diagnostics on the startup-to-`main` call to
    disappear. Record unrelated remaining errors; do not claim whole-unit
    compilability.
  - Final call is `main(argc, (void *)rbp, (void *)envp)` and both declaration
    and definition are `int main(int argc, char ** argv, char ** envp)`.
  - Clang has no too-many-arguments or `argv`/`envp` argument-type diagnostic.
    The full TU still reports 152 errors and 83 warnings; one nearby warning
    casts the recovered `int` return value to the caller's incorrectly
    recovered pointer local, a separate return-value/type-flow issue.
- [x] `CARGO_BUILD_JOBS=1 cargo build --release -p fission-cli --bin fission_cli`
- [x] `cargo fmt --all`
- [x] Final `cargo fmt --all --check` and `git diff --check`.
- [x] No DecBench score claim; its current adapter does not execute project
  translation-unit output.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- This local proposal retains row identities as evidence; production logic is
  based on the standard entry contract and typed call facts, not those row
  identities.

## 7. Review Notes

- The `main` symbol is used only as the standardized C runtime entry identity;
  no arbitrary function name, address, binary, or corpus row is used as a
  behavioral shortcut.
- No semantic/readability score gain is claimed. Acceptance is removal of the
  specific C declaration/call constraint violations on the real project row.
- No generic call framework or parallel signature registry is introduced.
