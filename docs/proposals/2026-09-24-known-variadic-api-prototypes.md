# Known Variadic API Prototype Change Proposal

## 1. Baseline Row Anchor

- Binary: `advanced_patterns_gcc_O1.exe` from the local dev corpus.
- Function: `_matherr`.
- Address: `0x140001960`.
- Reproduction: `target/release/fission_cli decomp <binary> --project --no-header --no-warnings --no-db -o /tmp/fission-issue57-baseline.c`.
- Current output: the call retains seven operands, but the assembled unit declares and defines `int fprintf(FILE* __stream, char* __format)` with no variadic tail. Repeated project runs also vary between that typed two-parameter hint and an inferred seven-parameter `undefined` signature.
- Semantic cases / benchmark score: not applicable; this is a C declaration and compilation-contract defect, not a source-semantic benchmark claim.
- Compiler evidence: an isolated `fprintf(FILE *, char *)` declaration with the observed seven-argument call fails with `too many arguments to function call, expected 2, have 7`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize / signature and callsite contract
- [ ] Structuring
- [x] Type/data recovery and API signature transport
- [x] Printer / project assembly
- [ ] Benchmark/automation

Evidence:

```text
The HIR call at _matherr keeps fprintf(stream, format, five conversions).
Known API signature hints supply the fixed prefix, but observed call-arity
facts can otherwise be recorded on a same-named target. The runtime-symbol
contract must therefore prevent those observations from becoming a fixed
arity hint. print_hir_function_impl emits only named params, and
fission-cli's unit::assemble derives the project prototype verbatim from the
definition line. Existing call_arity.rs already exempts known variadic
runtime symbols from exact-arity pruning; preserve that behavior.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a known variadic runtime symbol, call-site observed arity must not replace
the API's declared fixed parameter prefix. The function renders that prefix
followed by `...`, preserving every call operand without inventing fixed
parameters from one observed call.
```

Comparable coverage: printf / sprintf / snprintf and scanf-family runtime
symbols already share `is_known_variadic_runtime_symbol` metadata. Synthetic
test: an API-backed two-fixed-parameter `fprintf` definition and a seven-
operand call retain the two fixed parameters, print `...`, and compile under
GNU C17.

## 4. Risk And Ownership Check

- Existing owner: `fission-signatures` owns known variadic runtime-symbol metadata; `fission-decompiler` owns call-target/arity fact seeding; `fission-pcode` owns HIR declaration rendering; `fission-cli::unit` owns project prototype assembly.
- Shared substrate: typed signature / call-target facts; no new pass or metric.
- Existing behavior to preserve: exact-arity pruning for genuinely fixed prototypes, user/debug signature precedence, and all call operands at known variadic call sites.
- Main risk: imported thunk definitions may collide with system headers unless their fixed-prefix surface types remain compatible with the public C declarations.

## 5. Validation Matrix

- [x] Targeted invariant tests: `fprintf` call rendering retains all seven operands for a five-conversion format; known variadic call arity does not seed a fixed signature; the fixed prefix, `const` format pointer, and ellipsis render for `fprintf` and `snprintf`; the project `main` and ordinary internal arity-seeding tests remain valid.
- [x] Relevant crate suites were run together: `fission-signatures`, `fission-pcode`, `fission-decompiler`, `fission-project`, and `fission-cli`. Result: 1,407 passed, 3 failed, 1 skipped. The three failures are existing p-code expression fixtures; all three were reproduced on a clean worktree at the pre-change `d8b6b2c56` commit.
- [x] Focused real-binary check: release CLI decompiled `_matherr` and the full project. Four separate project runs kept the same two-parameter-plus-ellipsis `fprintf` declaration/definition and the seven-operand call.
- [x] Compiler contract: the extracted `_matherr` call with the old fixed-two-parameter declaration fails GNU C17 with “expected 2, have 7”; the same call with the system-compatible variadic declaration compiles. Whole-project Clang compilation remains blocked by other project declaration/type errors (147–154 errors over four output runs), but none of those diagnostics names `fprintf` or reports excess `fprintf` arguments; the baseline project output had two conflicting-`fprintf` diagnostics.
- [x] Regression: focused known-variadic callsite tests (3), the new multi-conversion renderer test, arity fact tests, signature surface test, project arity tests, workspace `cargo check`, release CLI build, formatting, and `git diff --check` passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? [x] No
- No benchmark identity was sent to another model.

## 7. Review Notes

- Production guards will use signature/call-target facts only; no function address, binary, or corpus-row condition.
- No quality-score improvement will be claimed from this declaration fix.
