# Keep instruction-local branches out of the function CFG

## 1. Baseline Row Anchor

- Issue: #110
- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_Os.exe`
- Function: `dot_product_stride`
- Address: `0x14000157e` (the issue's older address no longer names this
  function in the current binary)
- Corpus row or benchmark command:
  `runner/runner.py --corpus dev --function dot_product_stride --decompilers fission --run-mode local --no-resume`, with `FISSION_BENCHMARK_NO_CACHE=1`
- Current output summary: the raw p-code CFG has the normal six-block shape,
  but the emitted HIR starts with an unconditional-looking `do { ... } while
  (stride)` around the prologue and then emits the actual row loop separately.
  The first loop can repeat forever after `stride` is normalized to `1`, and it
  also assigns the input pointer `a` to zero before the row loop.
- Semantic cases passed / total: `gcc -Os` `0/5`, timeout. Across the nine
  current `dot_product_stride` rows, three pass all cases; the `gcc -Os` row
  is the directly anchored nontermination failure.
- Failure category: CFG construction creates a false entry self-edge from an
  instruction-local x86 `cmove` branch; loop analysis and structuring then
  consume that false edge as a natural prologue loop.
- Relevant observations:

  ```text
  raw p-code topology:
    0 -> 1
    1 -> {2, 5}
    2 -> 3
    3 -> {3, 4}
    4 -> 1
    5 -> {}

  block 0 contains the cmove micro-branch:
    seq 30, address 0x140001596: CBranch -> 0x14000159a
    seq 31, address 0x140001596: guarded Copy
    seq 32..61: remaining prologue and compare p-code

  current emitted HIR:
    do { if (!stride) stride = 1; rbx = 0; i = 0; a = 0; }
    while (stride);
  ```

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder / CFG construction
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
Rust-Sleigh emits the correct six raw blocks and seven raw edges. The first
block's explicit/raw fallthrough is block 1. `build_successor_index_map`
instead asks `block_terminator_op` for the last control opcode anywhere in the
block. That returns the CBranch at seq 30 even though its target is a later
instruction in the same PcodeBasicBlock. Resolving that target by address maps
it back to block 0; adding the normal layout successor produces [0, 1] and a
false self-edge. LoopBody therefore identifies block 0 as a loop head before
the structuring reducer runs.
```

The first wrong fact is the extra CFG edge, so changing a structuring reducer or
the printer would only conceal a malformed control-flow substrate.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A Branch/CBranch whose resolved target is a later operation in the same
PcodeBasicBlock is instruction-local control flow (for example a SLEIGH cmov
skip), not a basic-block terminator. It must be excluded from the
instruction-derived inter-block successor map. A real branch that targets a
different block, or a backward/self branch at the end of a block, remains a
CFG terminator.
```

The existing `same_block_forward_branch_target_op_idx` helper already defines
the required structural proof for both relative p-code targets and absolute
code-space targets. The CFG builder should reuse that proof rather than add a
mnemonic, address, function, or architecture guard.

ISA-agnostic check (ADR 0009):

- [x] The production condition is based on CFG block membership and target
  position, not an x86 function or address.
- [x] ISA-specific encodings remain in the existing p-code target resolver and
  SLEIGH lift; the shared CFG rule applies to any instruction-local branch.
- [x] The synthetic test states only a same-block forward branch followed by
  more p-code and an inter-block fallthrough.

Comparable coverage:

- Similar shape 1: absolute-address x86 cmov skip, already covered by
  `same_block_forward_branch_target_op_idx`.
- Similar shape 2: relative same-block forward p-code branch, covered by the
  same target-resolution helper and needed by non-x86 SLEIGH encodings.
- Synthetic invariant test: successor construction must return only the
  layout successor for a block whose last control opcode is a same-block
  forward branch; a real backward/self branch must still remain an edge.

## 4. Risk And Ownership Check

- Existing pass/owner: `build_successor_index_map` and
  `block_terminator_op` in `crates/fission-pcode/src/midend/cfg.rs`.
- Shared analysis/substrate candidate: [x] CFG / dominance / postdominance
  fact.
- Extending that owner is sufficient: all downstream loop and structuring
  consumers already use the builder's successor map. Correcting the map keeps
  the existing loop reducers unchanged and removes the malformed edge at its
  source.
- Possible interaction with existing passes: same-block materialization must
  continue to lower cmov bodies as guarded statements; only inter-block CFG
  edges change. Real tail branches, returns, indirect targets, LSDA edges, and
  backward loops must retain their current behavior.
- New or changed owner-to-owner dependency: [x] None; reuse the existing CFG
  target-position helper.
- Telemetry impact: no new field. Existing loop/structuring counts should no
  longer report a loop for a block without a back edge.
- Known cases that must not change: genuine inter-block conditional branches,
  backward/self branches at a block boundary, indirect branch target lists,
  no-return pruning, and same-block cmov statement lowering.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -- same_block_forward`
  - Expected signal: the synthetic successor map has no fabricated self-edge;
    the existing target-resolution tests remain green.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the two already-known unrelated
    byte-width tests.
- [x] Focused benchmark row:
  - Command: cache-disabled DecBench `dot_product_stride` sweep above.
  - Baseline artifact: `results/issue110_before_37fcfc429.json`.
  - Expected row-level improvement: `gcc -Os` no longer times out and the
    emitted prologue is not a loop; compare the same nine rows after the fix.
- [ ] Smoke or automation sample:
  - Command: the existing focused `dot_product_stride` DecBench sweep plus
    release CLI decompilation of the anchored binary.
  - Expected signal: no new fallbacks or fabricated entry loops.
- [ ] Optional related checks:
  - `cargo check`, `cargo check -p fission-decompiler`,
    `cargo fmt --all --check`, `git diff --check`, and
    `cargo build -p fission-cli --release`.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- No external implementation prompt was used; the proposal contains the
  measured owner evidence and the structural invariant only.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The intended claim is a semantic CFG correction. Any broader decompiler
  quality claim will be made only from the same cache-disabled DecBench rows
  after the implementation is measured.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; this extends the existing CFG terminator classification.
