# No-return Terminal Fallthrough

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_044.elf`
- Function: source `usage`, Fission `sub_31f0`
- Address: `0x31f0`
- Corpus row or benchmark command: DecBench sample-set NIR output followed by the
  scorer's Joern CFG extraction and topology comparison.
- Current output summary: the CFG-proven terminal block ends in
  `rax = exit(rbp);`, but is placed before the other conditional arm without an
  explicit non-fallthrough statement. C/Joern therefore connects the `exit`
  block to that following arm. The following arm jumps back to the `exit`
  block, producing a false cycle.
- Semantic cases passed / total: no executable source-semantic cases are
  published for this row; the focused structural baseline is 4 source nodes / 5
  source edges versus 4 output nodes / 5 output edges, non-isomorphic.
- Failure category: builder CFG reconstruction discards the lifter's no-return
  successor fact, reconstructs a lexical fallthrough after the call, and then
  lowers that invented edge as ordinary control flow.
- Relevant benchmark/static/readability observations: one residual goto; exact
  node and edge cardinality but wrong connectivity. The local NIR corpus
  baseline is 51 exact topologies after the preceding loop-test slice.

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
block_3234:
    rax = exit(rbp);
}
... the other conditional arm ...
goto block_3234;

With the benchmark's actual environment, the raw lifted blocks are:
  block 0x320d successors=[0x3234]
  block 0x3234 successors=[] terminal_opcode=Call target=exit
  block 0x323b successors=[0x3234]

`build_successor_index_map` ignores `PcodeBasicBlock.successors` and derives an
edge from every block without an explicit Branch/Return to its lexical next
block. That recreates 0x3234 -> 0x323b after the lifter deliberately removed it.
The existing call-effect summary independently proves `exit` as
may_exit=true/GhidraNoReturnData, but `lower_block_terminator` does not consult
that fact and returns Fallthrough. The first wrong fact is therefore the
builder's CFG/terminator contract, before structuring or printing.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When the lifted CFG gives a p-code block no successors and its last instruction
is a direct call whose typed effect summary proves that it does not return,
builder CFG reconstruction must not recreate a lexical fallthrough and
terminator lowering must emit an explicit terminal return. The lifted
zero-successor fact is stronger than layout. Conversely, an explicit lifted
successor outweighs the summary and is preserved. The return is unreachable by
the call contract; it changes no machine behavior but preserves the terminal
fact for every structuring driver and downstream C CFG consumer.
```

ISA-agnostic check:

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data remains in the existing call-effect/control-flow facts.
- [x] Synthetic test states only the CFG and typed call-effect shape.

Comparable coverage:

- Similar shape 1: `bin_033.elf`, iproute2 `main`, O2, source/output size delta
  116; a terminal `exit` arm is followed by another laid-out arm.
- Similar shape 2: `bin_186.elf`, gnutls `print_list`, O2-noinline,
  source/output size delta 39; a shared terminal `exit` block is followed by a
  sibling that jumps back to it.
- Broader census: 7 functions across 5 projects and both O2 and O2-noinline
  contain a labelled no-return terminal tail referenced by a goto
  (`bin_021`, `bin_033`, `bin_044`, `bin_186`, `bin_190`, `bin_215`,
  `bin_217`).
- Synthetic invariant tests: a terminal graph node whose last statement is a
  direct no-return call receives an explicit return; an ordinary terminal call
  and a lifted block with an explicit successor do not.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `midend::cfg::build_successor_index_map` reconstructs the builder CFG and
  `PreviewBuilder::lower_block_terminator` creates the shared terminator used by
  every structuring driver.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: direct-call resolution and the typed
  call-effect summary already exist in builder/scalar SSA. Reusing them while
  building successors and terminators makes the correction before any
  speculative structuring and requires no new pass or fact.
- Possible interaction: all structuring strategies see one fewer CFG edge and
  may select a different, now-valid terminal region. The inserted return is
  unreachable by the no-return fact and must not be inserted for indirect,
  unknown, preview-only, or merely possibly-exiting calls.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: terminal ordinary calls, empty function-end
  fallthrough, explicit returns, unresolved transfers, and nonterminal calls.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode proven_noreturn_call_prunes_lexical_fallthrough possible_exit_without_noreturn_provenance_keeps_fallthrough lifted_successor_outweighs_noreturn_summary`
  - Actual signal: 3/3 passed. A raw-terminal direct code-space call with
    GhidraNoReturnData loses the builder's synthetic lexical edge; identical
    `may_exit=true` from CallTargetRef and a block with an explicit lifted
    successor retain their edges.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Actual signal: 1005/1005 passed, one skipped.
- [x] Focused benchmark row:
  - Command: rebuild release CLI, decompile `bin_044.elf` at `0x31f0`, rerun
    Joern CFG extraction and exact topology comparison.
  - Actual row-level improvement: output changed from a false `exit` cycle and
    one goto to `exit(...); return;` with no goto. Joern GED improved 8 -> 2;
    source remains 4 nodes / 5 edges, output changed 4/5 -> 4/4. This is not an
    exact-topology gain yet.
- [x] Smoke or automation sample:
  - Command: cache-empty DecBench sample-set NIR and HIR sweeps.
  - Actual signal: both layers produced 250/250 functions and 224/224
    binaries. NIR gotos changed 1119 -> 1080 and HIR 1116 -> 1078. Three
    files gained gotos (`bin_033` +4, `bin_069` +2, `bin_217` +10), while the
    corpus total fell by 39/38 respectively.
  - Changed-row Joern comparison: 21 rows measurable before and after, no new
    output parse failure, GED sum 1488 -> 1439 (-49). The large correctness
    repair is `bin_217`, 206 -> 52: the old output omitted almost the entire
    reachable body. The principal measured regressions are `bin_069`, 18 ->
    55, and one exact-topology loss on `bin_162`, 0 -> 6.
  - Explicit tradeoff: the source parser models `exit()` as an ordinary call
    with fallthrough in `bin_162`; retaining that edge would reproduce a path
    the lifted CFG and typed no-return contract both prove impossible. This
    slice therefore is a correctness repair, not a leaderboard-perfect-rate
    improvement claim.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode && cargo build -p fission-cli --release`
  - Actual signal: both completed successfully.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not required; no pass or dependency is added.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in the AI prompt: none.
- Redaction confirmed: not applicable; investigation and implementation are
  local to this repository.
- Ghidra guidance confirmed:
  - [x] Existing Ghidra-derived fact data is used only as a correctness fact.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: DecBench sample-set NIR and HIR,
    250/250 functions in both layers.
  - Synthetic invariant test command/result: the three focused CFG tests pass.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; no new pass or metric.
