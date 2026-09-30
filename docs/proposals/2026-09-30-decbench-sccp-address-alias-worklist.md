# DecBench SCCP address-alias closure

## 1. Baseline Row Anchor

- Binary: `minigzip64`, DecBench sample-set revision `abee628fdb9f4b1622127bbdfc2ea6437fda288b`
- Function: `inflate_fast`
- Address: `0xdde0`
- Corpus row or benchmark command: Fission tree `cd55be5b36d8618f3574339af4a9c208c2f95bb4`; `fission_cli decomp <binary> --addr 0xdde0 --timeout-ms 45000 --engine rust-sleigh --layer both --json --no-db --no-warnings`
- Current output summary: NIR and HIR both return assembly fallback after the 45-second Rust-Sleigh render budget; no C body is available to the GED parser.
- Semantic cases passed / total: Not supplied by this DecBench sample-set; this row has GED/type scores, not a behavioral case oracle.
- Failure category: HIR normalization timeout while SCCP computes transitive local-address aliases.
- Relevant benchmark/static/readability observations: GED is unscoreable for this row; type-match is `0.0`. The no-watchdog run stayed CPU-bound for 180 seconds. A debug-build thread sample placed the renderer in `fission_midend_normalize::global_opt::sccp::collect_address_aliases::resolve_expr`.

The same SCCP stack recurs on three other functions in independent programs: `copy_reg` in coreutils (`O2-noinline`), `fallbackSort` in bzip2 (`O2`), and `dopass` in coreutils (`O2-noinline`). Each produces the same NIR/HIR timeout fallback in the pinned baseline. The separate `yyparse` fallback is not attributed to this change; its debug build panics in signed div/mod normalization at an oversized shift.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [x] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Fresh one-function render of the anchored binary:
  [DIAG] normalize start: ...
  [DIAG] pipeline start: normalize ...

Debug thread sample:
  fission_midend_normalize::global_opt::sccp::apply_sccp_pass
  fission_midend_normalize::global_opt::sccp::collect_address_aliases
  fission_midend_normalize::global_opt::sccp::collect_address_aliases::resolve_expr
```

The timeout occurs while SCCP resolves pointer aliases from variable definitions. Disabling DREAM and Match-Fold did not change it, and the sampled stack is in the normalize pass rather than structuring or CFG construction.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Address aliases are the finite transitive closure of local-address leaves over
the variable-definition graph. Compute that closure with an explicit worklist,
including cycles and multiple definitions, rather than recursively re-expanding
the same expression and definition subgraphs for each variable.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is not gated on an ISA, calling convention, address, or function name.
- [x] The rule is expressed over HIR variable definitions and local-address expressions.
- [x] Synthetic coverage will state only the alias-graph shape.

Comparable coverage:

- `copy_reg` / coreutils / `O2-noinline`: same resolver on stack and 45-second fallback.
- `fallbackSort` / bzip2 / `O2`: same resolver on stack and 45-second fallback.
- `dopass` / coreutils / `O2-noinline`: same resolver on stack and 45-second fallback.
- Synthetic invariant tests: a long alias chain and a cyclic alias component with an address leaf.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `fission-midend-normalize::global_opt::sccp::collect_address_aliases`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [x] None; this is a small SCCP-local transitive closure over the assignment summaries it already builds.
- Why extending that owner is sufficient: only SCCP consumes this alias set. A finite monotone worklist can preserve its existing conservative union across all assignments without changing the HIR contract.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass, metric, or owner is planned.
- Possible interaction with existing normalize/structuring/materialize passes: the set of escaped locals must remain identical, so call-side invalidation stays conservative and SCCP output semantics stay unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: None.
- Known cases that must not change: direct `&local`, address-through-cast/pointer-offset, multiple definitions, alias cycles, variables with no address leaf, and call-side value invalidation.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize sccp_address_alias_worklist`
  - Expected signal: long chains finish without recursive stack growth; cycles and multiple definitions return the same conservative local-address set.
  - Result: passed (2 tests).
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no NIR/HIR regressions.
  - Result: passed (1,153 tests; 1 skipped).
- [x] Focused benchmark row:
  - Command: pinned DecBench sample-set rows, `--engine rust-sleigh --layer both --timeout-ms 45000 --no-db`, followed by `FISSION_BENCHMARK_NO_CACHE=1` scoring.
  - Result: the anchored row and three comparable SCCP rows returned NIR/HIR C output in 1.05–6.11 seconds each instead of 30–45 second assembly fallbacks. GED became scoreable but remained large/approximate, so this is a robustness recovery, not a measured quality gain. Type-match was 22.7%, 5.9%, 0%, and 15.2% for the four rows in both layers.
- [x] Full pinned sample-set regression:
  - Command: 250 DecBench sample-set functions, NIR/HIR, `--engine rust-sleigh --timeout-ms 45000 --no-db`; GED/type scoring with `FISSION_BENCHMARK_NO_CACHE=1`; package validation and Linux/amd64 GCC 13 compile-rate pass.
  - Result: 250/250 records, 0 fallbacks, 0 missing. NIR GED perfect count stayed 65 (65/233 after vs. 65/228 before); HIR stayed 66 (66/233 vs. 66/228). New C outputs made five GED rows scorable, but the added rows had large approximate distances and raised the means (NIR 26.21→27.23, HIR 28.75→29.84); this is not a quality gain. Structural/text type perfect counts stayed unchanged. GCC compile validity rose from 80/158 to 81/158 for NIR and 81/158 to 82/158 for HIR; byte-match perfect counts stayed 1/158 and 2/158. Both packages validated at 224/224 binaries and 250/250 functions with no warnings.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize`, `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo build -p fission-cli --release`, and `cargo fmt --all --check`.
  - Expected signal: clean.
  - Result: all passed.
- [x] Boundary audit, if a new pass/helper/dependency was added: not applicable; no new pass, helper API, metric, or dependency is planned.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: none.
- Redaction confirmed: no external implementation prompt was used.
- Ghidra guidance confirmed: not applicable; no output-style imitation is proposed.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run; the complete pinned sample set was used for regression checking, not as an unseen tuning pool.
  - Synthetic invariant test command/result: passed; long-chain and cyclic/multiple-definition cases.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed in the worklist implementation and its tests.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; focused rows escape timeout fallback, while GED remains poor/approximate and no quality gain is claimed.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; implementation extends SCCP's existing alias closure.
