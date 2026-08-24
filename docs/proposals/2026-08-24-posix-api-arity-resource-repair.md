# POSIX API Arity Resource Repair

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_191.elf`
- Function: `is_owner`
- Address: `0x5720`
- Corpus row or benchmark command: cache-disabled fixed-denominator sample-set type census (`109` measured rows), plus direct NIR/HIR decompilation
- Current output summary: the body emits `stat()` although the machine-level call supplies a pathname and output buffer. The missing pathname also leaves the function's second parameter rendered as `ulonglong` instead of `char *`.
- Semantic cases passed / total: not available for this individual public row; the external fixed-40 baseline is `22/40` semantic-perfect with mean `0.6442`.
- Failure category: imported API prototype resource records a real multi-argument POSIX function as exactly zero arguments.
- Relevant benchmark/static/readability observations: fixed-109 Types baseline is `6` perfect, distance `1187`, and `67` pointer-miss rows. The sample-set NIR/HIR baselines are `1079`/`1077` gotos. Empty `stat()` occurs ten times in seven binaries; empty `sigaction()` and `wait()` occur three times each. The affected rows span sysvinit, shadow, cronie, libacl, and coreutils at both O0 and O2-family optimization levels.

Comparable real rows:

- `bin_030.elf` / `check_init_fifo` / `0x90ae` (sysvinit O0): two `stat()` calls; Types distance 8.
- `bin_114.elf` / `restore` / `0x5480` (libacl O2): `stat()` and a `char * -> ulonglong` argument miss; Types distance 25.
- `bin_097.elf` / `cron_pclose` / `0x7930` (cronie O2): `wait()` loses its status pointer; Types distance 5.
- `bin_012.elf` / `wall` / `0x2d50` (sysvinit O2): `sigaction()` loses all three arguments; Types distance 35.

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
utils/source/typeinfo/generic_clib_signatures.txt:
  sigaction|int|void
  stat|int|void
  wait|int|void

The 64-bit table contains the same three records. `ApiTypeDatabase::parse_record`
correctly interprets `void` as zero parameters. `apply_import_signature_seed`
then locks that exact arity, and `prune_known_api_call_args_expr` truncates the
call expression to the locked arity. Direct pre-HIR output therefore already
contains `stat()`, `sigaction()`, and `wait()`; this is not a printer defect.
Neighboring records such as `lstat`, `fstat`, `waitpid`, and `wait4` retain their
documented parameter lists.
```

The first wrong fact is the signature resource. Normalize is behaving according
to that contract, so the production repair belongs in the source resource and
its packed runtime form rather than in a downstream exception.

## 3. Generality / Invariant Proof

Generalized rule:

```text
An imported function's exact arity may be used to remove lifted call operands
only when the signature resource represents the documented function prototype.
For POSIX stat, sigaction, and wait, preserve their pathname/buffer,
signal/action, and status-pointer parameters respectively. A C `(void)`
prototype remains exactly zero-argument for genuinely zero-argument functions.
```

ISA-agnostic check:

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data remains in calling-convention/SLEIGH owners; this change is declaration data.
- [x] The invariant test names API contracts, not compiler tuples, binaries, addresses, or rows.

Comparable coverage:

- Similar shape 1: ten `stat()` occurrences across seven sample binaries and multiple projects.
- Similar shape 2: three occurrences each for `sigaction()` and `wait()` across five additional binaries.
- Synthetic invariant test: load the shipped resource and assert arity and pointer surface types for the three corrected contracts while a genuine `(void)` API remains zero-argument.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `ApiTypeDatabase` owns the imported declaration contract; `interproc_sig_prop` and `callsite_type_prop` consume it.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: correcting the authoritative records fixes both type seeding and exact-arity pruning without adding a rule or pass.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: none added.
- Possible interaction with existing normalize/structuring/materialize passes: restored arguments may expose existing type constraints and prevent incorrect dead argument deletion; control-flow structure should remain unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: none.
- Known cases that must not change: genuine zero-argument functions such as `fork`, `getpid`, and `__errno_location`; variadic-call handling; all non-corrected signature records.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-signatures -E 'test(posix_multi_argument_contracts_are_not_void)'`
  - Expected signal: passed; corrected arities/types load from the shipped packed resources and `getpid` remains zero-argument.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-signatures -p fission-midend-normalize -p fission-pcode -p fission-decompiler`
  - Expected signal: `1501` passed, `1` skipped; checks passed.
- [x] Focused benchmark row:
  - Command: rebuild release CLI and decompile the listed real rows with NIR and HIR.
  - Expected row-level improvement: all 16 measured calls now retain their documented arguments. `bin_191` recovers `char *` and moves Types distance `2 -> 1`.
- [x] Smoke or automation sample:
  - Command: cache-disabled fixed-109 Types census, full 250-function NIR/HIR sample sweep, and external Docker fixed-40 runner.
  - Expected no-regression signal: NIR and HIR each produced `250/250` functions in `224/224` binaries; gotos remained `1079`/`1077`. Fixed-109 Types remained `6` perfect and distance `1187`, with pointer-miss rows `67 -> 66`: `bin_191` improved `2 -> 1`, while `bin_000` moved `13 -> 14` because the now-explicit stack buffer displaced a name-only local match. The external cache-disabled fixed-40 run was unchanged on every core field: semantic `22/40` (mean `0.6442`), Types `22/40` (`0.8127`), GED `15/40` (mean distance `5.225`), and no semantic/type/GED/recompile/failure-category row changes.
- [x] Optional related checks:
  - Command: held-out mixed-optimization non-scored corpus rows containing the same APIs; `python3 scripts/audit/benchmark_smell_scan.py --root .`
  - Expected signal: benchmark smell scan found zero findings. Three non-scored, mixed-optimization functions across coreutils and shadow retained arguments (`stat(file, __buf)`, `stat(format_copy, __buf)`, and two three-argument `sigaction` calls). A fourth O2 function still emitted `stat()` because its call operands were never recovered before API pruning; that is a separate upstream argument-discovery defect, not evidence against the corrected contract. Repeated NIR/HIR rendering of three focused rows was byte-identical after excluding timing telemetry.
- [x] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: clean; zero findings and zero violations.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: not applicable.
- Redaction confirmed: not applicable; no external model prompt was used.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: four non-scored O0/O2 functions were inspected; three exercised the corrected contract and one exposed an independent earlier blocker.
  - Synthetic invariant test command/result: `posix_multi_argument_contracts_are_not_void` passed and also proved `getpid(void)` remains zero-argument.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; none is added.

The fixed-109 type-distance total is deliberately reported as neutral, not as a
type leaderboard gain. The change remains correct independently of that metric:
the previous output claimed that calls with one to three real operands had no
arguments at all. The corrected output represents those calls and their data
dependencies instead of deleting them.
