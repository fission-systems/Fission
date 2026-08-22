# Proven-void call results are machine clobbers, not source returns

## 1. Baseline Row Anchor

- Binary: `bin_094.elf` (cronie `crontab`, O0)
- Function: `env_free`
- Address: `0x9449`
- Corpus row or benchmark command:
  `fission_cli decomp .../bin_094.elf --addr 0x9449 --layer nir --json --no-header --no-warnings`
- Current output summary: the final declared-void `free(envp)` is followed by
  `return xVar24;`; the function is printed as `ulonglong` instead of `void`.
- Semantic cases passed / total: not locally executable; static DecBench row only.
- Failure category: type/prototype recovery; ABI result-storage clobber treated as a
  source return value.
- Relevant benchmark/static/readability observations: Fission v0.2.1 reports
  GED distance 4, type match 0, and byte match 0.3333 on this row.

Comparable measured rows:

- `bin_164.elf`, `0x7b30`, kmod O2 `mod_free`: source ends in
  `free(mod);`; Fission emits `return free(param_1);` and a non-void signature.
  GED is already perfect, while type match and byte match are both 0.
- `bin_174.elf`, `0x4800`, dpkg O2 `statdb_write`: source ends in
  `free(dbname);`; Fission emits `return sub_35d0();`. GED distance is 4,
  type match 0, byte match 0.3134. The unresolved PLT identity means the first
  implementation slice may not reach this row.
- `bin_078.elf`, `0x3630`, coreutils O2-noinline `describe_change`: source ends
  in two `free` calls; Fission returns the unresolved `sub_2480()` on one exit.
  GED distance is 480, type match 0.0909, byte match 0. The unresolved PLT and
  multi-exit structure remain separate blockers.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
bin_094 final block:
  Call free
  ... ordinary epilogue ...
  Return <stack return address>

lower_return_terminator_impl observes no primary-return definition after the
call, but function_has_primary_return_def() finds unrelated earlier RAX scratch
definitions and synthesizes live RAX as the source return.

The signature owner already knows `free` has return type `void`, but that fact
does not reach the builder. By normalize time a Return(Some(...)) has already
been created. The result-value fact must be transported independently from
arity: the packed signature records do not preserve variadic `...`, so using
the same payload would risk constraining unrelated call arguments.
```

Glaurung's reference contract keeps the ABI result-register definition as a
machine clobber while setting `result_is_source_value = false` for an exact
declared-void call. This is an invariant reference, not an output-style target.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A call still clobbers ABI result storage even when its exact prototype returns
void. If a direct call's typed prototype proves that its result is not a source
value, no later instruction defines the primary return slot, and control then
returns from the current function, the call cannot supply a value-returning C
return. A proven-void tail call is likewise an expression statement followed by
a bare return, not Return(Some(Call)). Unknown and non-void prototypes retain
the existing behavior.
```

ISA-agnostic check:

- [x] The condition is stated over the ABI primary-return slot and typed call
      contract, not an ISA enum.
- [x] ISA-specific result storage remains in cspec/register naming.
- [x] Synthetic tests use call/return dataflow without a compiler tuple or row id.

Comparable coverage:

- Similar shape 1: ordinary call to exact-void callee, epilogue, machine return.
- Similar shape 2: exact-void direct tail call.
- Synthetic invariant test: exact-void call suppresses only call-sourced return;
  an unknown call and an explicit post-call return-register definition remain
  value-bearing.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `PreviewBuilder::lower_return_terminator_impl` and
  `PreviewBuilder::emit_unsupported_control_surface`.
- Shared analysis/substrate candidate:
  - [x] Type constraint / calling-convention fact
  - [x] Def-use / reaching-definition fact
  - [ ] CFG / dominance / postdominance fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
- Why extending that owner is sufficient: the signature layer supplies a typed
  `call_result_is_source_value` fact; the builder already owns both machine-return
  lowering paths and the post-call return-slot def check.
- New helper/pass: no new pass. Add one result-fact map to the existing type
  context and one owner-local query. Arity summaries remain unchanged.
- Possible interaction: tail wrappers and value-returning library calls must keep
  their current source returns. ABI clobber/liveness behavior is unchanged.
- New or changed owner-to-owner dependency:
  - [ ] None
  - [x] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact: none.
- Known cases that must not change: unknown calls, non-void calls, explicit
  primary-return definitions after a void call, and non-tail ordinary calls.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(proven_void)'`
  - Result: 3/3 passed, including the explicit post-call result-register
    definition negative case.
- [x] Signature transport test:
  - Command:
    `cargo nextest run -p fission-decompiler -E 'test(exact_void_signature_marks_abi_result_as_non_source)'`
  - Result: 1/1 passed.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1010/1010 passed, 1 skipped.
- [x] Focused benchmark rows:
  - Command: cache-disabled decompile of `bin_094@0x9449` and
    `bin_164@0x7b30` in NIR and HIR.
  - Result: both print `void` with bare returns; body calls remain present and
    ordered. `bin_094` also loses two dead return-temp assignments.
- [x] Linux GCC recompilation check:
  - Command: DecBench `ByteMatchMetric` in the rebuilt
    `fission-benchmark-oracle` Debian container, GCC 12.2, cache disabled,
    against the exact unstripped `cronie/crontab` function bytes.
  - Result for `env_free`: similarity 0.341463 -> 0.388889; changed assembly
    lines 27 -> 22; recompiled size 114 -> 98 bytes; both outputs compile.
- [x] Smoke or automation sample:
  - Command: full 250-function sample-set NIR/HIR sweep, fixed denominator.
  - Result: NIR 250/250 functions and 224/224 binaries. The arity-independent
    final implementation is byte-identical on all 224 NIR files to the initial
    measured implementation; goto total remains 1079. HIR also completed
    250/250 before the transport-only refactor; both focused HIR rows were
    rechecked afterward and remained stable.
- [x] Optional related checks:
  - Command: `cargo check -p fission-decompiler && cargo build --release -p fission-cli`
  - Result: both passed.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: 0 findings, 0 violations, 0 migration debt.

## 6. AI Review / Prompt Firewall

- Was an external AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in an external AI prompt: none.
- Unseen or synthetic validation evidence: targeted invariant tests plus the
  unscored validation pool before any broad quality claim.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed

DecBench metric scope note: current `type_match.py` scores formal parameters
and locals, not function return types. Therefore this is a measured
source/ABI correctness and recompilation-distance improvement, but it does not
claim a direct Types-perfect gain. GED/goto counts also remain unchanged.
