# Terminal do-while latch recovery

## 1. Baseline Row Anchor

- Binary: DecBench UNOPTIMIZED `bzip2/bzip2`
- Function: `BZ2_indexIntoF`
- Address: `0x85fd`
- Corpus row or benchmark command: local HF snapshot, `CONFIG=unoptimized`,
  release `fission_cli`, HIR layer, source CFG from
  `pipeline_data/source_cfgs/O0/bzip2/bzip2.json`
- Current output summary: source is a `do { ... } while (na - nb != 1)`;
  Fission emits `while (1)` with a terminal `if (...) break; else continue;`
- Semantic cases passed / total: baseline full O0 HIR GED perfect `12,482 /
  32,196` (`38.8%`); this row has GED `2`, with six source and six output
  blocks
- Failure category: multi-block do-while is represented as an infinite loop
  plus explicit latch controls
- Relevant benchmark/static/readability observations: source CFG has 7 edges;
  the current output CFG has 8. The latch has one natural tail and exactly two
  outgoing edges: loop exit and the back edge.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [x] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence: `try_lower_multiblock_dowhile` currently calls
`lower_loop_body_subgraph` and always wraps the result in `PreHirStmt::While {
cond: 1 }`, even when the proven single natural tail is lowered as a total
`break`/`continue` conditional. The raw CFG and lowered HIR both preserve the
correct condition; only the structured statement kind is lost.

## 3. Generality / Invariant Proof

Generalized rule:

> For a proven natural loop with exactly one tail, if the lowered terminal
> statement is a two-arm conditional whose arms are exactly `break` and
> `continue`, use the corresponding latch condition as a `DoWhile` condition.
> If the break arm is the true arm, negate the condition; if the continue arm
> is the true arm, retain it. Otherwise keep the conservative `while (1)` form.

ISA-agnostic check:

- [x] The rule uses CFG loop membership and structured control statements;
  it has no ISA, function, address, or binary guard.
- [x] ISA-specific data remains in the existing p-code/ABI layers.
- [x] Synthetic tests state only the terminal latch shape.

Comparable coverage:

- ChibiOS `chSysGetStatusAndLockX`: same-node GED near-miss with a conditional
  branch followed by a common return.
- Betaflight `pwmWriteBeeper`: same-node GED near-miss with branch-arm control
  and an extra output edge.
- Synthetic invariant tests cover both condition polarities and reject a
  partial one-arm break.

## 4. Risk And Ownership Check

- Existing pass/owner:
  `fission-midend-structuring::loops::try_lower_multiblock_dowhile`.
- Shared analysis/substrate candidate: [x] CFG / dominance / postdominance fact
- Why extending that owner is sufficient: the loop reducer already proves the
  natural loop, exit, preferred latch, and body membership before lowering.
  The new helper only interprets the terminal control it just emitted.
- Possible interaction with existing passes: a `DoWhile` is already a native
  PreHir/HIR statement handled by cleanup, normalization, naming, and printing.
- New or changed dependency: [x] None
- Telemetry impact: none; existing loop lowering counters remain valid.
- Known cases that must not change: multi-tail loops, non-total latch
  conditionals, while-style heads with an independent exit, and any body whose
  terminal statement is not exactly the two control arms.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-structuring terminal_latch`
  - Observed: both polarity/rejection tests pass.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Observed: 1,052 passed, 1 skipped.
- [x] Focused benchmark row:
  - Command: local HF DecBench `bzip2/bzip2`, address `0x85fd`, HIR GED
  - Observed: `GED 2 -> 0`; the emitted `while (1)`/latch shape becomes
    `do ... while` and matches the six-node/seven-edge source CFG.
- [x] Focused binary sweep:
  - Command: five affected projects, 16,589 functions, same local HF O0
    harness and fresh GED cache.
  - Observed: GED changed on 16 rows: 11 improved and 5 regressed; the total
    GED distance delta was `-47`, type score did not change. The regressions
    are the documented trade-off of representing an equivalent terminal
    latch as a native `do ... while` rather than `while (1)` plus controls.
- [x] Full corpus sweep:
  - Command: local HF DecBench `CONFIG=unoptimized`, 267 binaries and 34,406
    manifest functions, HIR layer, fresh GED cache.
  - Observed: 32,196 GED rows; perfect `12,482 -> 12,489` (`38.8%`), mean
    GED changed by `-0.004690024`, type_match stayed `3,569 / 31,382`, and
    decompilation/measurement denominators were unchanged.
- [x] Optional related checks:
  - `cargo check`, `cargo fmt --all --check`, `git diff --check`, release CLI
    build all passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Synthetic invariant test command/result: recorded after implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard-only edits:
  - [x] Confirmed; the baseline and after result are measured on the same
    local HF corpus row.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; this extends the existing do-while reducer.
