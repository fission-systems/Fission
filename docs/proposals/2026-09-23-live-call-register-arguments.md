# Decompiler Change Proposal: Live ABI Argument Slots Across Calls

## 1. Baseline Row Anchor

- Binary: `math_gcc_O2.exe` in the DecBench dev corpus
- Function: `main`
- Address: `0x140002980`
- Corpus row / reproduction: direct real-binary render with `target/release/fission_cli decomp ... --addr 0x140002980`; whole-binary comparison via `decbench evaluate corpus/dev/binaries/c/math_gcc_O2.exe --source corpus/dev/source/c/math.c --output /tmp/fission_issue69_math_before --decompiler fission`.
- Current output summary: Fission emits `linear_search(values, 5, xVar4)` followed by `binary_search()`. The source calls `binary_search(values, 5, 7)`.
- Semantic cases passed / total: main is not a row in the dev per-function manifest, and `decbench evaluate` omitted the entrypoint from its 14 rendered functions; it reports no semantic cases for this caller. Do not present its aggregate metrics as a main-function semantic score.
- Failure category: missing live ABI call inputs / behaviorally incorrect call.
- Relevant evidence: in the PE disassembly, `RCX` is set to the `values` array before `linear_search`; `R8D` is set to 7 before `max`; immediately before `binary_search`, only `EDX` is reset to 5. `linear_search` reads RCX and R8D and does not write either; it does write RDX. The exact callee preview already establishes binary_search arity 3. Whole-binary DecBench baseline: GED mean 11.22, byte-match mean 0.00; neither is a score for the omitted `main` row.
- Second issue-comment anchor: the root-level `advanced_patterns_gcc_O2.exe` (not the distinct `binaries/c/` copy), `main@0x140002990`. The issue comment's `0x1400029d0` is inside that function, not its entry. Before this change Fission emitted `kv_lookup(&table)` and `apply_binop(&add_ints)`; the disassembly stages `RDX=3` and `R8D=2` before `list_sum`, then leaves them live through that call, and later stages `R8D=4` for the first `apply_binop`. After this change the same real-binary HIR emits `kv_lookup(&table, xVar7, xVar8)` where those values are 3 and 2, and `apply_binop(&add_ints, xVar7, 4)`.

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
The call has no explicit register operands in raw p-code. call_recovery already
has an exact three-slot prototype summary, but check_ancestor_realistic rejects
RCX and R8D after seeing the preceding Call because both are caller-saved.
It accepts only the newly assigned RDX slot; the later contiguous-prefix gate
then returns no call arguments at all. The leaf callee's complete p-code shows
that it writes the RDX argument slot but leaves the RCX and R8 slots untouched.
```

The issue's `__mingw_setusermatherr` subcase is not a call-argument case: its
actual bytes store RCX to `stUserMathErr`, then jump to an import thunk for
`__setusermatherr` without staging an argument. NIR and pre-structuring output
retain the global write, but default HIR currently omits it. That is a separate
HIR effect-preservation defect and remains unresolved; do not manufacture a
call argument or claim issue #69 is fully closed until that symptom is
separately triaged.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A reaching ABI argument slot may cross an intervening call only when a
complete callee effect summary proves that the callee does not write that
slot. Derive writes from the callee's register outputs and the active
register-namer/cspec ABI-slot model. If decoding is incomplete, a nested call
or indirect call exists, or the target/effect is unknown, keep the existing
conservative caller-saved invalidation. Merge same-symbol summaries
conservatively: any unknown summary makes the merged effect unknown; known
write sets are unioned.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] The rule uses ABI argument-slot indices, not register names or ISA enums.
- [x] ABI-specific register families are resolved through `RegisterNamer` and cspec.
- [x] Synthetic tests will describe reaching definitions, calls, and slot write sets without names or addresses.

Comparable coverage:

- Similar shape 1: an intervening leaf call writes one argument slot while leaving other live slots unchanged.
- Similar shape 2: an unknown or nested-call target has no complete register-effect proof and must retain current conservative invalidation.
- Synthetic invariant test: an exact-arity call recovers unchanged slots 0 and 2 across a callee known to write only slot 1; unknown effects do not permit that recovery.

## 4. Risk And Ownership Check

- Existing pass/owner: `PreviewBuilder` call-argument recovery and the existing `NirCallEffectSummary` produced by direct-callee preview.
- Shared analysis/substrate candidate: ABI-slot/register-namer fact plus the existing call-effect summary; no new pipeline pass.
- Why extending the owner is sufficient: callee p-code is already decoded and summarized for each direct caller. The same pass can collect writes to integer ABI parameter slots, and call recovery can consult the effect already attached to its resolved target.
- Possible interaction with existing call-argument recovery: unknown calls, incomplete callee bodies, indirect calls, callother operations, and tail-call/indirect-branch shapes must not become optimistic. The old callee-saved rule remains the fallback.
- New or changed owner-to-owner dependency:
  - [x] None; add one field to the existing core call-effect contract and consume it in the existing builder owner.
- Telemetry impact: none.
- Known cases that must not change: previous staged arguments must not leak across calls with unknown effects; a callee that writes a slot still invalidates it; calls without exact current-callee arity remain subject to the current contiguous-prefix rule.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-pcode <focused-filter>` plus the `fission-decompiler` facts-summary test.
  - Result: facts accepts a complete one-edge fallthrough block and rejects an ambiguous multi-edge block; the builder recovers untouched slots 0 and 2 across a proven slot-1 write. Unknown effects and a slot-0 write remain conservative. Focused tests passed.
- [x] Crate-level gates:
  - Commands: `cargo nextest run -p fission-pcode`, `cargo nextest run -p fission-decompiler`.
  - Result: `fission-decompiler` 74/74 passed. `fission-pcode` 1,117 passed, 1 skipped, and the same three existing failures remained: `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load`.
- [x] Focused real-binary rows:
  - Commands: release `fission_cli decomp --layer hir` at the exact `main` entries in both math binary copies and the root-level advanced-patterns binary, plus the remaining libc/O1 issue addresses.
  - Result: math emits all three arguments to both `linear_search` and `binary_search`; advanced-patterns emits all three arguments to `kv_lookup` and to the first `apply_binop`; `file_size`, `count_spaces`, `quotient_of`, and `_FindPESectionByName` retain their expected input arguments. The wrapper HIR omission is separately unresolved as noted above.
- [x] DecBench regression context:
  - Commands: cache-disabled single-binary evaluation for root-level `math_gcc_O2.exe` and `advanced_patterns_gcc_O2.exe`.
  - Result: math GED mean 11.22, byte-match mean 0.00, 14 functions, no failures; advanced-patterns GED mean 8.00, byte-match mean 0.00, 9 functions, no failures. DecBench omits each binary's `main`, so these numbers are regression context, not a score for the corrected call sites. Math's generated source matched the recorded baseline; total time changed from 0.905s to 0.874s.
- [x] Optional checks:
  - Commands: `cargo check -p fission-pcode -p fission-decompiler`, `cargo fmt --all --check`, `git diff --check`, `cargo build --release -p fission-cli`.
  - Result: all pass. `scripts/check/owner_boundaries.sh` also passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Unseen or synthetic evidence:
  - Patch-validation pool result: not run; no scoped patch-validation runner was found in the external benchmark checkout. The official holdout is not used for tuning.
  - Synthetic invariant: complete; facts and caller-recovery tests pass.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed by rule design.
- No semantic improvement is claimed from aggregate GED/byte-match metrics:
  - [x] Confirmed.
- Existing effect-summary owner is extended; no parallel pass is proposed:
  - [x] Confirmed.
- Issue status:
  - The live ABI-slot loss reproduced in multiple caller rows is fixed by this change.
  - The issue's MinGW wrapper also has a measured HIR omission of its global callback store; it is not addressed by this call-recovery change and requires a separate owner-native investigation before closing the issue.
