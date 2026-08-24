# Known-Prototype Call Argument Recovery

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_000.elf`
- Function: `check_init_fifo`
- Address: `0x8350`
- Corpus row or benchmark command: full 250-function sample-set NIR sweep plus an exact non-variadic API-arity census over the rendered calls.
- Current output summary: two `fstat` calls are rendered with one argument and one `dup2` call is rendered with one argument, although their exact declarations require two. The sole rendered operand is the lifted call return address (`0x83e0`, `0x8480`, or `0x84ab`), not a source argument.
- Semantic cases passed / total: no public per-row behavior oracle; the preceding external fixed-40 baseline is semantic-perfect `22/40`, mean `0.6442`.
- Failure category: builder call-argument recovery keeps only the contiguous ABI-register prefix written in the call's basic block. It does not fill an earlier missing slot from a dominating predecessor when a later slot was staged locally.
- Relevant benchmark/static/readability observations: the current sample has 29 calls whose rendered arity is below an exact shipped declaration, spanning 20 functions, nine projects, and both O2 and O2-noinline. Current fixed-109 Types is 6 perfect at distance 1186; current NIR/HIR gotos are 1079/1077.

Comparable real rows:

- `bin_128.elf` / `dump` / `0x1b50` (sysvinit O2-noinline): `fseek` has one of three declared arguments.
- `bin_168.elf` / sample function at `0x1300` (zlib O2-noinline): `fopen` has one of two declared arguments.
- `bin_184.elf` / `compare_files` / `0x2c30` (coreutils O2-noinline): `memcmp` has one of three declared arguments.
- `bin_209.elf` / sample function at `0x4ff0` (grep O2): `strcmp` has one of two declared arguments.
- `bin_215.elf` / `main` / `0x2720` (sysvinit O2-noinline): `write` has one of three declared arguments.
- Ten short `dcgettext` calls occur in seven functions across coreutils, dpkg, findutils, shadow, and tar at O2/O2-noinline.

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
At the first fstat call in bin_000, raw p-code contains:
  block 5: Copy RSI <- stack-address
  block 5: Store [RSP] <- 0x83e0       (lifted return-address scaffold)
  block 5: Call fstat

RDI was defined in the dominating predecessor. recover_call_args_from_block
builds [None, Some(RSI)] and then keeps only the contiguous prefix, whose
length is zero. recover_call_stack_args_from_block misclassifies the return
address scaffold as one stack argument, yielding fstat(0x83e0).

call_arg_carrier_assignments returns same-block carriers as soon as it sees
any one carrier, so it never consults predecessors to fill the missing RDI
slot. The exact fstat prototype is already present in NirTypeContext; neither
normalize nor the printer can reconstruct the discarded ABI value.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a direct call with a locked exact prototype, each declared register-backed
ABI slot is live at the call even when its last definition is in a dominating
predecessor. Recover a missing slot only from a unique realistic reaching
definition for that slot at the call site. Do not synthesize a value when the
definition is absent, non-dominating, or ambiguous. Existing ABI/cspec tables
choose the carrier slots; the rule does not name an ISA, function, address,
symbol, binary, or corpus row.
```

ISA-agnostic check:

- [x] Production condition uses an exact prototype, ABI slots, and dominance rather than an ISA enum.
- [x] Register placement remains in the existing cspec/calling-convention model.
- [x] Synthetic coverage states the predecessor-definition and call-site shape without a compiler tuple or benchmark identity.

Comparable coverage:

- Similar shape 1: 29 under-arity exact-API calls in 20 sample functions and nine projects.
- Similar shape 2: O2 and O2-noinline rows both contain the failure; calls range from one to three missing operands.
- Synthetic invariant test: a direct exact-arity call with slot 1 staged locally and slot 0 defined in a dominating predecessor recovers both; a join with no unique realistic definition remains partial.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `builder/calls/call_recovery.rs` and its `check_ancestor_realistic` proof.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: the builder already owns call operands, ABI slot assignment, def-site lookup, dominance, and exact prototype context. The change extends this owner-local recovery before values are discarded; the existing decompiler fact assembly transports exact imported declarations into that context, and no new pass is required.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no pass or metric is added. One owner-local query reads locked arity, while the signature owner centralizes the pre-existing variadic-family fact so builder and normalize cannot disagree about whether a stored parameter count is exact.
- Possible interaction with existing normalize/structuring/materialize passes: recovered arguments receive the existing exact API type constraints and exact-arity pruning. Unknown and variadic calls must keep current behavior.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: existing call-recovery and signature-refinement counters remain sufficient; no schema change.
- Known cases that must not change: calls without locked exact prototypes, variadic calls, indirect/ambiguous targets, slots with non-dominating or ambiguous definitions, control flow, and the existing 131 complete `dcgettext` calls.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(exact_call_arity)'`
  - Expected signal: the proven predecessor slot is restored; unsafe join and unknown-prototype sentinels remain unchanged.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode -p fission-midend-normalize -p fission-decompiler`
  - Expected signal: no new failures.
- [x] Focused benchmark row:
  - Command: release-build and re-decompile `bin_000`, `bin_128`, `bin_168`, `bin_184`, `bin_209`, and `bin_215` in NIR/HIR.
  - Expected row-level improvement: exact calls recover real ABI values rather than return-address constants, without control-flow changes.
- [x] Smoke or automation sample:
  - Command: full 250-function NIR/HIR sample sweep, fixed-109 Types census, exact API-arity census, and external Docker fixed-40 runner with caches disabled.
  - Expected no-regression signal: fewer under-arity calls, no perfect-row loss, 250/250 outputs, and unchanged or improved semantic/GED/recompile fields.
- [x] Optional related checks:
  - Command: non-scored mixed-optimization call-shape validation plus `python3 scripts/audit/benchmark_smell_scan.py --root .`.
  - Expected signal: the invariant reproduces outside scored identities; no benchmark identity leaks.
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
  - Patch validation pool command/result: previous/current release CLIs were compared on 12 non-scored `corpus/scale` rows, followed by a bounded 48-row O0 census. O2-noinline cronie `child_process` restored both operands for two `dup2` calls, and O2-noinline coreutils `have_same_content` restored the real third `memcmp` operand. The 48 O0 rows had no call-line changes (two rows timed out), showing that the change declines when the cross-block shape is absent.
  - Synthetic invariant test command/result: the exact-call tests passed for both a unique dominating predecessor definition and a non-dominating join that must remain unrecovered.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed by design.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; the old calls violate their declarations and carry a return-address scaffold instead of live ABI inputs.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new pass or metric was added, and the variadic-family helper was moved from normalize to the signature owner rather than duplicated.

## 8. Measured Result

- Full sample-set coverage remained 224/224 binaries and 250/250 functions in both NIR and HIR. Goto totals were unchanged at NIR 1079 and HIR 1077.
- Exact non-variadic imported calls rendered below their declarations fell from 29 to 11. Eighteen calls were repaired across 13 functions; exactly 23 NIR files and the same 23 HIR files changed. The remaining 11 calls lack a safe unique reaching definition and were deliberately left partial.
- The fixed-denominator 109-row Types census improved total distance from 1186 to 1184. Perfect rows remained 6 and pointer-miss rows remained 66. No row regressed; `bin_215` improved from distance 40 to 38.
- Focused real outputs replaced lifted return-address constants with live ABI values: the affected `fstat`, `dup2`, `fopen`, `memcmp`, `strcmp`, and `write` calls now carry their declared arguments. `bin_128`'s unresolved `fseek` remained unchanged because its missing slot could not be proven.
- Eight fresh-process repeats of the changed `bin_209` function were byte-identical in the rendered `code` field independently for NIR and HIR. Whole JSON payloads differ only in duration telemetry, which is expected and does not affect variable names or pseudocode.
- The external Docker fixed-40 run used the same cache-disabled `dev`, `limit=40`, `variant_limit=1` contract as the preceding baseline. All row and aggregate fields were unchanged: semantic perfect 22/40 (mean 0.6442), Types perfect 22/40 (mean 0.8127), GED perfect 15/40 (mean distance 5.225), recompilation perfect 0/40, and zero failure-category transitions.
- Non-scored `corpus/scale` previous/current comparison found the same invariant outside the sample identities: O2-noinline cronie `child_process` repaired two `dup2` calls and O2-noinline coreutils `have_same_content` repaired `memcmp`'s length operand. A separate 48-row O0 scan produced no call-line changes, so the implementation did not manufacture arguments where the cross-block optimized shape was absent.
- The two builder invariants, two decompiler fact tests, the 1,506-test four-crate gate, four crate checks, `nir_boundary_scan`, `benchmark_smell_scan`, and `git diff --check` all passed.
