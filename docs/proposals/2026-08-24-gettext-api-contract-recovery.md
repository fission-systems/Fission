# Gettext API Contract Recovery

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_105.elf`
- Function: `try_help`
- Address: `0x3489`
- Corpus row or benchmark command: full 250-function sample-set NIR/HIR sweep and fixed-denominator 109-row Types census
- Current output summary: `gettext(local_18, 13489)` and `gettext("Try ...", 13541)` preserve phantom second operands, return an untyped integer, and leave both source `char *` parameters rendered as integers.
- Semantic cases passed / total: no public per-row behavior oracle; external fixed-40 baseline is semantic-perfect `22/40`, mean `0.6442`.
- Failure category: the shipped imported-API database contains no `gettext` or `dcgettext` prototype, so exact arity and pointer return/parameter constraints are unavailable.
- Relevant benchmark/static/readability observations: current fixed-109 Types is `6` perfect, distance `1187`, with `66` pointer-miss rows. Current NIR/HIR gotos are `1079`/`1077`. The sample output contains 17 `gettext` and 141 `dcgettext` calls across 34 binaries.

Comparable real rows:

- `bin_125.elf` / `allocerr` / `0x9e210` (bash O2): `dcgettext(0, message, 5)` returns an integer and the function's `char *` argument remains `ulonglong`; Types distance 2.
- `bin_195.elf` / `sparse_offset_decoder` / `0x1fad8` (tar O0): `gettext(message, 129937)` retains a phantom operand; three source character-pointer arguments remain integers; Types distance 5.
- `bin_046.elf` / `close_files` / `0x6116` (shadow O0): four two-argument `gettext` calls; Types distance 1.
- `bin_044.elf` / `usage` / `0x31f0` (coreutils O2): fifteen `dcgettext` calls; Types distance 4.
- `bin_118.elf` / `flush_line` / `0x52d6` (diffutils O0): one `gettext` call on a row already Types-perfect, serving as a no-regression sentinel.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
Neither generic_clib packed table contains a gettext or dcgettext record.
Calls therefore keep unrelated lifted register operands:
  gettext(local_18, 13489)
  gettext("Malformed extended header: excess %s=%s", 129937)

ApiTypeDatabase is the existing declaration owner. interproc_sig_prop consumes
its return and parameter lattices, while callsite_type_prop uses its exact arity.
The Glaurung reference output for the same O0 family declares
`extern char * gettext(const char *)`, confirming the missing declaration shape
without becoming a runtime dependency.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
GNU gettext has one message-id character-pointer parameter and returns the
translated character pointer. dcgettext has domain/message character-pointer
parameters plus an integer category and returns the translated character
pointer. Exact imported declarations constrain result/argument types and bound
the number of lifted ABI operands retained at those calls.
```

ISA-agnostic check:

- [x] Production condition is independent of ISA and calling-convention enum.
- [x] Calling-convention placement remains in cspec/ABI owners; this change supplies declaration data only.
- [x] Synthetic coverage names API contracts, not compiler tuples, binaries, addresses, or corpus rows.

Comparable coverage:

- Similar shape 1: 17 `gettext` calls in six sample binaries across diffutils, dpkg, and tar at O0/O2.
- Similar shape 2: 141 `dcgettext` calls in 28 sample binaries across at least coreutils, shadow, dpkg, bash, libacl, and findutils.
- Synthetic invariant test: load shipped packed resources and assert exact arity, return type, and character-pointer parameters for both contracts.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `ApiTypeDatabase` and the existing import-signature consumers.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: two records in the existing data owner activate existing exact-arity and type propagation; no new code path is required.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: none added.
- Possible interaction with existing normalize/structuring/materialize passes: phantom operands will be removed and pointer evidence may rename/retype existing values; control-flow must remain unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: existing exact-arity/type refinement counters may increase; no schema change.
- Known cases that must not change: variadic formatting consumers of the translated string, non-gettext imports, the Types-perfect `bin_118` sentinel, and all control-flow.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-signatures -E 'test(gettext_contracts_are_available)'`
  - Expected signal: `gettext` arity 1 and `dcgettext` arity 3, with `char*` return/message surfaces.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-signatures -p fission-midend-normalize -p fission-pcode -p fission-decompiler`
  - Expected signal: no new failures.
- [x] Focused benchmark row:
  - Command: rebuild release CLI and decompile `bin_105`, `bin_125`, `bin_195`, and the perfect-row sentinel `bin_118` in both layers.
  - Expected row-level improvement: documented arities and pointer surfaces appear; at least one repeated pointer/type miss improves and the perfect sentinel remains perfect.
- [x] Smoke or automation sample:
  - Command: cache-disabled fixed-109 Types census, full 250-function NIR/HIR sample sweep, and external Docker fixed-40 runner.
  - Expected no-regression signal: no perfect-row loss, lower or equal type distance, 250/250 outputs, and unchanged or improved semantic/GED/recompile fields.
- [x] Optional related checks:
  - Command: mixed-optimization non-scored corpus calls plus `python3 scripts/audit/benchmark_smell_scan.py --root .`.
  - Expected signal: declaration behavior repeats outside scored rows; no benchmark identity leaks.
- [x] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: clean.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: non-scored `corpus/scale` direct
    decompilation passed on O0 coreutils, O0 diffutils, and O2 shadow functions.
- Synthetic invariant test command/result: `gettext_contracts_are_available`
  passed, and the four-crate nextest gate passed 1502/1502 with one unrelated
  skipped test.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; none is added.

## 8. Measured Result

- Full sample-set coverage remained 224/224 binaries and 250/250 functions in
  both NIR and HIR. Goto totals were unchanged at NIR 1079 and HIR 1077.
- The fixed-denominator 109-row Types census improved total distance from 1187
  to 1186. Perfect rows remained 6, pointer-miss rows remained 66, and no row
  regressed. `bin_105` improved from distance 2 to 1 because its first
  parameter became `char*`; the Types-perfect `bin_118` sentinel remained
  perfect.
- Exactly 31 NIR files and the same 31 HIR files changed, all containing a
  `gettext` or `dcgettext` call. All 17 emitted `gettext` calls now have the
  documented single argument. Of 141 `dcgettext` calls, 131 have all three
  arguments; ten remain one- or two-argument calls because those ABI operands
  were already absent before exact-arity pruning. Synthesizing those missing
  operands is an upstream call-operand recovery problem and is not claimed by
  this change.
- Eight fresh-process repeats of `bin_039` were byte-identical independently
  for NIR and HIR. This focused check did not reproduce the reported
  cross-run naming nondeterminism.
- The external Docker fixed-40 run was cache-disabled, limited to one variant
  per function, and compared by `(function_name, compiler_variant)`. All
  normalized core fields were identical to the preceding baseline: semantic
  perfect 22/40 (mean 0.6442), Types perfect 22/40 (mean 0.8127), GED perfect
  15/40 (mean distance 5.225), and recompilation perfect 0/40.
- Non-scored `corpus/scale` validation covered three functions in three
  binaries, two projects, and both optimization levels: O0 coreutils `usage`
  and O0 diffutils `try_help` emitted one-argument `gettext` calls; O2 shadow
  `close_files` emitted three-argument `dcgettext` calls.
- `nir_boundary_scan`, `benchmark_smell_scan`, `git diff --check`, and the
  relevant crate checks all passed.
