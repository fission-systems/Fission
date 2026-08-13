# Virtualized terminal-goto materialization at the DREAM boundary

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_190.elf`
- Function: `sub_3d20` (`main` in the reference outputs)
- Address: `0x3d20`
- Corpus row or benchmark command:
  `FISSION_PREVIEW_DIAG=1 ./target/release/fission_cli decomp .../bin_190.elf --layer nir --json --no-header --no-warnings --timeout-ms 45000 --addr 0x3d20`
- Current output summary: 136 CFG blocks; NIR 62 gotos/881 lines, HIR 61
  gotos/830 lines. The saved references contain Ghidra 20 gotos and angr 17.
- Semantic cases passed / total: not available for this DecBench readability row;
  the corpus runner decompiled 250/250 functions. The independent phase-2
  ground-truth suite is included in the validation matrix.
- Failure category: DREAM declines with `cycle-survived` after an unlowerable
  `Sequence` whose tail is a terminal internal `Goto`.
- Relevant benchmark/static/readability observations: current full-corpus
  baseline is NIR 1214 and HIR 1207 gotos. On the 204 files shared with both
  references, `bin_190` is the largest Fission-minus-Ghidra gap: 62 versus 20
  (angr: 17).
- Late-admission safety observation: a driver currently returns a successful
  `Some(candidate)` through `lower_isolated`, which commits minted semantic
  identities before `try_alternative_structurings` decides whether that
  candidate wins. Focused `FISSION_DREAM=0` runs proved this was not the cause
  of the two initial guard-veto regressions (`bin_052` and `bin_177`); those
  were established DREAM wins being rejected by an over-tight relative
  budget. The ordering remains unsafe independently and is corrected without
  attributing those measured regressions to it.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
Raw p-code topology has block 135 -> block 134.
The structuring host has already virtualized that edge, so block 135 has no
successor but lower_block_terminator(135) is Goto(0x4e07), an address owned by
block 134 in the same function.

[DIAG] node lowering failed: node=135 block=135 terminal=true
       terminator=Goto(19975) graph_successors=[]
[DIAG] DREAM cycle fold stopped: reason=unlowerable-shape
       shape=Sequence members=[131, 135]
[DIAG] DREAM declined: cycle-survived

materialize_virtual_gotos identifies exactly this fact, then calls
node_statements for its source. node_statements deliberately rejects a
terminal Goto. Therefore the recovery path cannot materialize the condition it
was written to recover.

Alternative-driver admission currently has a second owner-local ordering bug:
`lower_isolated` commits identities on `Ok(Some(_))`, while the quality
comparator runs only after that return. A successful-but-rejected candidate is
therefore not isolated from the baseline path. `StructuringHost::lower_observed`
already defines the required no-commit preview contract.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
If a CFG edge was virtualized before an alternative structuring driver runs,
and the source terminator still resolves to a block owned by the same function,
the alternative graph must represent that transfer exactly once as a goto at
the source and a label at the target before shape folding. Materialization must
not ask the ordinary terminal-node lowering path to accept the transfer it is
specifically responsible for placing. Multiple virtualized gotos may form a
chain, so all source/target facts are collected before any node body is marked
materialized.

Alternative candidates are first produced with `lower_observed`, compared, and
ranked without committing host state. Only the stable winner is rerun through
`lower_isolated`; that committed rerun is the only candidate body that may be
emitted. A preview that loses admission cannot mutate the host, and no global
snapshot/rollback is required.
```

ISA-agnostic check:

- [x] Production condition is a CFG ownership/terminator fact, not an ISA or
      calling-convention gate.
- [x] No ISA-specific data is introduced.
- [x] Synthetic tests state only CFG topology, block ownership, and lowered
      terminators.

Comparable coverage:

- Similar shape 1: another terminal internal goto at node/block 98 in the same
  measured function.
- Similar shape 2: a chain where a virtual-goto target is itself a
  virtual-goto source; both transfers must remain present.
- Synthetic invariant test: a virtual terminal-goto source can be lowered and
  a two-edge virtual chain produces both goto/label pairs without losing a
  terminal return at the final target.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `reaching_driver::materialize_virtual_gotos`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: it already detects virtualized
  internal terminal gotos and owns the alternative graph/body representation;
  the defect is only how it initializes source and target bodies.
- If adding a new pass/helper/metric: no new pass or metric. A small owner-local
  helper may share the terminal-node lowering contract without changing
  ownership.
- Possible interaction: destination blocks that return or contain another
  virtualized goto must retain their own terminal control exactly once.
- Possible interaction: the winning driver is lowered twice. The observed run
  is charged to the shared work budget; a failed committed rerun falls back to
  the untouched baseline.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact: retain concise stop-reason diagnostics under the existing
  `FISSION_PREVIEW_DIAG` gate; no `NirBuildStats` change.
- Known cases that must not change: switch count, return statements, existing
  accepted DREAM candidates, `bin_217`/`bin_027`, and all files not winning the
  raw-plus-cleaned comparator.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-structuring -E 'test(<new virtual-goto tests>)'`
  - Expected signal: source transfer, destination label, chained transfer, and
    final return are each present exactly once.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode -p fission-midend-structuring`
  - Expected signal: no new failures.
- [x] Focused benchmark row:
  - Command: current release CLI on `bin_190.elf@0x3d20`, NIR and HIR, caches
    bypassed by direct invocation.
  - Expected row-level improvement: DREAM no longer declines at the
    contradictory terminal-Goto source; any chosen candidate must win the
    existing raw and cleaned quality comparison.
- [x] Smoke or automation sample:
  - Command: NIR/HIR DecBench 224-binary runner with per-file baseline diff.
  - Expected no-regression signal: 250/250 functions, total gotos do not rise,
    and no file rises without explicit review.
- [x] Optional related checks:
  - Command: `cargo nextest run --workspace --no-fail-fast`; phase-2 corpus
    ground truth included.
  - Expected signal: only the seven documented pre-existing emulator failures,
    with no new failure.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: zero findings.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model review. Codex performed local repository
    diagnosis from the measured row and reference source.
- Information exposed outside this local task: none.
- Ghidra/angr guidance: algorithm and invariant reference only; no output-style
  reproduction request.
- Unseen or synthetic validation evidence: synthetic CFG tests plus the
  independent phase-2 corpus ground-truth test.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed

## 8. Measured Result

- Focused `bin_190.elf@0x3d20`: NIR 62 -> 37 gotos; HIR 61 -> 37.
- DecBench 224 binaries / 250 functions:
  - NIR 1214 -> 1058 (-156), no per-file goto regressions.
  - HIR 1207 -> 1051 (-156), no per-file goto regressions.
- Shared 204-file comparison:
  - Fission NIR 1016, HIR 1009; Ghidra 691; angr 420.
  - Fission/Ghidra: 1.47x NIR, 1.46x HIR.
  - Fission/angr: 2.42x NIR, 2.40x HIR.
- Guard-cost audit rejected four newly unlocked outputs whose guard forests
  grew to 2,755--4,974 nodes. Existing healthy candidates remain inside the
  measured <=2,008-node envelope, or must satisfy the proportional budget.
- Tests:
  - `fission-midend-structuring` + `fission-pcode`: 1232 passed, 1 skipped.
  - Phase-2 ground truth: 250 attempted, 13 equivalent, 190 not checkable,
    zero divergence.
  - Workspace: 2492 passed, 7 known `fission-emulator` SLEIGH-decode failures,
    5 skipped; no new failure.
  - NIR boundary scan: zero findings.
- External local Docker benchmark (`dev`, 40 functions, one variant): 40/40
  rows observed, 36 clean, 152/198 semantic cases, mean pass rate 0.762,
  24/36 perfect semantic rows, 38 gotos. Artifact validity is true; publication
  is false as required for a local run.
