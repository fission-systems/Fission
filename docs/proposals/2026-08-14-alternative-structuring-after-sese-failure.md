# Alternative structuring after a failed SESE baseline

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_039.elf`
- Function/address: `sub_8680` at `0x8680`
- Direct command:
  `FISSION_PREVIEW_DIAG=1 target/release/fission_cli decomp .../bin_039.elf --layer nir --no-header --no-warnings --addr 0x8680`
- Measured CFG: 122 blocks, 191 edges, admitted as `GraphCollapse`.
- Measured output: NIR 68 gotos / 1047 lines; HIR 68 gotos.
- References on the same function: angr 30 gotos; Ghidra 32 gotos.
- Current control-flow result:

```text
[DIAG] structuring start: blocks=122 edges=191 force_linear=false
[DIAG] SESE structuring failed, falling back to linear: UnsupportedCfgRegionShape
[DIAG] structuring linear done: ... admission=GraphCollapse
```

No match-fold or DREAM diagnostic is emitted. The alternatives are called only
inside the successful `sese_result` arm; a failed SESE baseline bypasses them.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring admission/dispatch
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

`SeseStructuringPass` intentionally leaves the body unset on
`UnsupportedCfgRegionShape`. `OrphanGotoRepairPass` then creates the linear
multiblock body and commits it without calling `try_alternative_structurings`.
The CFG was admitted for graph collapse, so this is not an extreme/irreducible
budget refusal. The owner is the fallback dispatch between these two passes.

## 3. General Invariant

When graph structuring is admitted but the primary SESE strategy cannot produce
a baseline, the successfully lowered linear body is still a valid baseline for
independent alternative strategies. Alternative candidates must be previewed
without committing builder state, compared against that exact linear baseline,
and only the stable winner may be rerun on an isolated host and emitted.

The rule is CFG- and strategy-based. It contains no binary, address, compiler,
or ISA predicate. Explicit force-linear, extreme-budget, and irreducible-budget
admissions remain excluded.

## 4. Safety Contract

- The linear body is produced first and remains the fallback on every decline,
  comparison loss, or committed-rerun failure.
- `try_alternative_structurings` already uses `lower_observed` for previews and
  `lower_isolated` only for the admitted winner; no rollback is introduced.
- The existing raw comparator protects switches, empty-if shells, nesting, and
  guard-formula growth; the cleaned comparison prevents a downstream goto loss.
- Candidates admitted through this newly opened fallback funnel must also keep
  the largest single guard proportional to the transfers removed. This is
  scoped to the new funnel: applying it retroactively to established candidates
  measured 16 corpus regressions and was rejected.
- The graph-collapse admission budget remains authoritative. Alternatives are
  not attempted for a forced-linear admission.

## 5. Validation Matrix

- [x] Focused `bin_039` NIR/HIR before/after with diagnostics.
- [x] Focused comparable rows that currently miss alternatives (`bin_063`) and
      rows already handled by the successful-SESE path (`bin_190`).
- [x] Targeted dispatch regression test if an owner-local fixture exists;
      otherwise cover the candidate safety contracts in their existing tests.
- [x] `cargo nextest run -p fission-midend-structuring -p fission-pcode`.
- [x] `cargo check -p fission-pcode` and `cargo check -p fission-decompiler`.
- [x] DecBench 224 binaries / 250 functions, NIR and HIR, with per-file diff.
- [x] Phase-2 corpus ground truth with zero divergence.
- [x] External local Docker benchmark, non-published.

## 6. Quality Claim Gate

No improvement is claimed until the focused row and full NIR/HIR corpus are
remeasured. A candidate that reduces aggregate gotos but introduces a per-file
regression will be revised or rejected.

## 7. Measured Outcome

- The motivating `bin_039` remains NIR/HIR 68 gotos: alternatives now run, but
  DREAM declines it at the existing decision-complexity boundary. The dispatch
  diagnosis was correct, but this row itself did not improve.
- `bin_021`: NIR/HIR 12 -> 3 gotos.
- `bin_063`: NIR 24 -> 12 and HIR 24 -> 11 gotos.
- `bin_190`, an established-path control row, remains NIR/HIR 37 gotos.
- A newly exposed candidate for `bin_179` removed all 14 shipped gotos but
  concentrated a 643-node guard forest into one 149-node condition and nearly
  doubled the text. The fallback-only maximum-guard admission rejects it and
  preserves the prior output.
- Full corpus: NIR 1058 -> 1037; HIR 1051 -> 1029; zero per-file regressions.
- Common 204-function reference subset: NIR 995, HIR 987, Ghidra 691, angr 420.
- Targeted structuring/pcode: 1,233 passed; phase-2 ground truth: one corpus
  test passed with zero divergence. Full workspace: 2,493 passed and only the
  seven pre-existing emulator SLEIGH decode failures remained.
- External local Docker matrix: 344 rows completed, result `valid: true`, 322
  adapter-clean rows and 322 semantic-tested rows. The 22 non-clean rows were
  classified as address/boundary output or connection-timeout failures; the
  validity envelope reported no invalidity reasons. As required for a local
  run, it is not publishable and was not promoted to `latest`.
