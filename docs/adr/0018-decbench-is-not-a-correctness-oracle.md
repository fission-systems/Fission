# ADR 0018: DecBench measures similarity, not correctness

**Status:** Accepted
**Last verified:** 2026-11-23
**Amends:** [ADR 0006](0006-decompiler-quality-change-gate.md) (what counts as
evidence for a quality change) and the measurement rule in `AGENTS.md`
**Related:** [ADR 0017](0017-ghidra-is-not-an-oracle.md) (the same question
for Ghidra)

## Context

`AGENTS.md` made DecBench the measurement surface for every decompiler quality
claim. Quality improved only "if the measurement moved", and a change that
made scores worse needed "an explicit tradeoff". In practice the DecBench
numbers were being treated as the definition of a correct decompiler. They do
not measure that.

- **GED is purely topological.** DecBench's VJ-GED compares the shape of
  Fission's control-flow graph with the source's: node count and in/out degree.
  It does not see values, arguments, constants, call targets or types. A
  function that prints the wrong case value, the wrong argument or an invented
  call can score perfect. Recovering code that was missing adds nodes and can
  make the score worse.
- **The type metric covers only the variables it can match.** Locally it
  matches a variable by its stack location, so at -O2 the register-resident
  majority falls outside it. It also strips signedness.
- **byte_match needs recompilation.** It rewards output that compiles back to
  the same bytes, which is a narrow and noisy proxy. It is also not run
  locally.

The effect was measured on PR #1019. It fixed switch cases attached to the
wrong values, a fabricated argument, lost per-path call arguments and a
conditional overwrite passed as one path's value. On the full dataset:
- Total GED distance moved by −0.27% (1,546,426 → 1,542,247).
- Type scores did not change.
- An earlier revision of the same fix made changed rows 1.0% *worse*, because
  it printed correct values as `c ? a : b` (Joern counts a ternary about as
  much as an `if`) and printed unused jump-table entries as separate cases.
- In the final revision, one function scores 42 worse because its argument is
  now correct.

The distribution of Fission's distance tells the same story. Against Kuna on
the 88,554 rows both tools score:
- Half of Fission's net excess sits in 2% of the rows and 80% in 6%.
- Those rows are dominated by code that is missing (an undecoded jump table, an
  unresolved tail jump) or over-produced (duplicated returns, one register
  coalesced across a whole function).
- A correctness fix elsewhere barely moves the total, however much output it
  repairs.

If DecBench is the oracle, correct fixes look like no-ops or regressions.
Shape-only changes that score well then look like wins: merging blocks,
turning `if/else` into ternaries, duplicating code to remove joins. `AGENTS.md`
already forbids those as metric-fitting, but it gave no positive account of
what correct means.

## Decision

1. **DecBench is a comparative and regression metric, not a correctness
   oracle.**
   - A better GED, type or Union score is not evidence that a change is
     correct.
   - A worse one is not evidence that it is wrong.
   - DecBench answers two questions: how close the output's shape and types
     are to the source's, and how that compares with other decompilers on the
     same rows.

2. **Correctness is shown by evidence that checks what the output means.**
   This uses the same order as ADR 0017, strongest first:
   1. **Native execution:** the binary or fixture run on real hardware,
      compared with Fission's output (`docs/VERIFY_REPLAY.md`, #992).
   2. **`fission_cli verify --tier ground-truth`:** compares, for the same
      inputs, the printed tree against the emulated function:
      - the return value
      - every call with its arguments
      - every store outside the frame
      
      It catches decompiler errors. It shares the SLEIGH lift with the
      decompiler, so it cannot catch a lift error; for that, see (1) and
      ADR 0017.
   3. **Architecture manuals** for instruction semantics.
   4. **Source ground truth:**
      - DWARF or PDB for types and variables
      - the source CFG for control flow the source actually has
      - call sites counted by call-instruction address against the binary
        (not by callee name)
   5. **Reading the changed output.** Read diffs raw, not through a filter
      that can hide `goto` or `return`.

3. **A change that makes wrong output correct can land with a worse DecBench
   score.** The PR states:
   - the correctness evidence from (2)
   - the DecBench numbers
   - the cause of any regression, in terms of what the output now prints
   
   A change with no correctness evidence whose only support is a better score
   must pass the metric-fitting test: say why the output is better without
   mentioning the metric. It must also show it did not make correct output
   wrong, for example with no new `verify` divergences on the functions it
   changed.

4. **DecBench measurement stays mandatory where output can change.** Report
   the full-dataset sweep for:
   - catching regressions (lost jump tables, decompile failures, collapsed
     functions)
   - comparing with other tools
   - any official ranking or release claim
   
   Report the numbers as they are, total distance next to the perfect count,
   and never tune against them.

## Consequences

- **"Quality improved" now means more than a score.** It means the output is
  more correct or more readable with evidence for it. DecBench numbers are
  reported next to that evidence, not in place of it.
- **ADR 0006 row anchors can come from any of the sources in (2),** for
  example a `verify` divergence or a native replay mismatch, not only from a
  benchmark row.
- **PR descriptions for output-changing work gain a correctness block** beside
  the existing benchmark block.
- **A falling GED can still flag a regression to investigate,** for example a
  table that stopped decoding. A flat or rising GED no longer blocks a fix that
  makes the output correct.
- **The metric-fitting rule is unchanged** and matters more. When the score is
  not the goal, there is no reason to make the output's shape worse in order
  to move it.
