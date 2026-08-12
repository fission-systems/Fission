# Virtual-exit complex-arm commit gate

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_073.elf`
- Function: `sub_6780`
- Address: `0x6780`
- Command: release `fission_cli decomp .../bin_073.elf --layer nir --json --no-header --no-warnings --addr 0x6780`
- Current output: 559 lines, 16,501 bytes, and 42 `goto` statements; entry emits `if (!rax) { goto block_6f60; }`.
- Semantic cases: unavailable in this DecBench sample-set lane.
- Failure category: complex conditional arms with distinct real returns have only a virtual common postdominator, and recursive arm lowering is unsafe while it runs inside an uncommitted candidate.
- Measurement history: a reverted recursive-arm experiment changed the 224-binary total from 2,135 to 2,296 gotos and changed unrelated whole-function types. No part of that experiment is retained.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
ImmPostDomTree::compute constructs a virtual super-exit for multiple returns,
then truncates it and remaps every real node whose idom is the virtual exit to
self. nearest_common_postdominator therefore cannot distinguish "no common
postdominator" from "common postdominator is the virtual function exit".

Temporary field fingerprints on bin_073 idx=0 also proved that failed collapse
rules mutate locals, stack-slot ownership, temps, materialized_vns, and lowering
caches. A broad rollback prototype kept 42 gotos but substantially rewrote
whole-function register/temp bindings, proving that the current pipeline also
depends on pre-lowering side effects. It was reverted.
```

## 3. Generality / Invariant Proof

Generalized rules:

```text
1. In a CFG with multiple real exits, the common postdominator of arms that end
   at different exits is a typed VirtualExit fact, not an absent fact and not a
   fabricated real block index.
2. Each arm owns only the nodes exclusively reachable from its successor. Nodes
   reachable from both successors form a shared terminal tail owned once after
   the conditional; they are not duplicated into either arm.
3. Arm ownership is a membership set, not a lexical block-index interval. Every
   exclusive node must have no side entry, every exclusive exit must enter the
   shared tail or leave the region, and the shared tail must be closed toward
   the region exit.
4. Recursive complex-arm structuring may execute only after shape and admission
   have established that its enclosing stable-first conditional will commit.
   Failed probes must not recursively lower arbitrary blocks.
```

ISA-agnostic check:

- [x] Both rules use CFG/postdominance and dispatch facts only.
- [x] No ISA or calling-convention gate is added.
- [x] Synthetic tests use only graph shape and candidate commit behavior.

Comparable coverage:

- Multiple exits: two conditional arms reach distinct returns.
- Mixed arm: one arm is linear and the other contains a loop/nested conditional before a distinct return.
- Shared terminal tail: interleaved lexical layout where both exclusive arm
  sets enter common tail nodes that remain outside the emitted `if/else`.
- Negative: an arm trapped in a non-terminating SCC has no exit-reachable postdominator.

## 4. Risk And Ownership Check

- Existing owners: `cfg_analysis::ImmPostDomTree`, `conditionals::if_else`, and `sese_driver` stable-first dispatch.
- Shared substrate: CFG / immediate postdominance.
- Why extension is sufficient: retain the already-computed virtual root and expose it through a typed query; do not add a pass or vendor dependency.
- Interaction risk: existing real-block follow callers must keep receiving `Option<usize>` and must never treat the virtual exit as an index. Recursive lowering is a separate measured slice after the substrate lands.
- Telemetry: none for the virtual-exit substrate. A later recursive slice may reuse existing rejection telemetry.
- Known cases that must not change: single-exit diamonds, real join follows, loops disconnected from every exit, current NIR/HIR text and goto totals for the substrate-only commit.

## 5. Validation Matrix

- [x] Targeted invariant tests
  - Command: `cargo nextest run -p fission-midend-structuring virtual_exit`
  - Expected: distinct-return arms report `VirtualExit`; legacy real-follow query remains `None`; real diamonds still report their join.
- [x] Crate gates
  - Command: `cargo nextest run -p fission-midend-structuring && cargo nextest run -p fission-pcode`
- [x] Focused real row
  - Command: rebuild release CLI and rerun `bin_073.elf@0x6780`.
  - Expected for substrate-only slice: byte-identical NIR/HIR, 42 gotos.
- [x] External sample-set smoke
  - Command: `vendor/decbench-evalkit/decbench-evalkit-sample-set/run_fission.py`
  - Result: NIR and HIR each completed 224/224 binaries and 250/250 functions.
    NIR: 2,135 gotos, 38,444 lines, 1,022,738 bytes. HIR: 2,034 gotos,
    35,576 lines, 942,938 bytes. The focused `bin_073@0x6780` output remained
    byte-identical (NIR 42 gotos; HIR 40 gotos).
- [x] Related checks
  - Command: `cargo check -p fission-pcode && cargo check -p fission-decompiler`
  - Result: both checks passed; `nir_boundary_scan.py` also reported zero findings.

## 6. AI Review / Prompt Firewall

- Another AI model was not asked for implementation advice.
- No external prompt or vendor implementation is used.
- Production conditions contain no binary, function, address, corpus, compiler, or ISA identity.
- A later recursive-arm implementation needs both this focused row and an unseen/synthetic regression signal before any quality claim.

## 7. Review Notes

- [x] No hardcoded row identity in production.
- [x] No benchmark/printer-only semantic claim.
- [x] Existing postdom and conditional owners are extended; no duplicate pass.

## 8. Isolated Complex-Arm Slice (2026-08-12)

Implementation:

- Proves exclusive arm membership and the shared terminal tail from CFG facts
  before statement lowering.
- Places the deferred candidate immediately before the plain-if/goto fallback
  in stable Conditional order.
- Clones `PreviewBuilder` and uses a private register-origin side channel;
  commits the isolated host only after both membership-limited arms lower.
- Does not snapshot or roll back global builder state.
- Preserves shared-tail blocks outside the `if/else` and normalizes overlapping
  bottom-up SESE children back to raw parent reconstruction.
- Fixes HIR presentation's subtree-local label pruning/recovery so an enclosing
  or sibling goto cannot lose its target label.

Measured focused row, release build:

| Layer | Before goto / lines / bytes | After goto / lines / bytes |
|---|---:|---:|
| NIR | 42 / 559 / 16,501 | 41 / 599 / 21,689 |
| HIR | 40 / 529 / 14,835 | 39 / 563 / 17,817 |

Measured 224-binary / 250-function DecBench sample set:

| Layer | Before goto / lines / bytes | After goto / lines / bytes |
|---|---:|---:|
| NIR | 2,135 / 38,444 / 1,022,738 | 2,130 / 38,850 / 1,072,419 |
| HIR | 2,034 / 35,576 / 942,938 | 2,023 / 35,931 / 973,271 |

Interpretation: goto density improved in both layers and all rows completed,
but output size increased (NIR +49,681 bytes; HIR +30,333 bytes), and individual
files include both improvements and regressions. This is a measured trade-off,
not an unqualified readability win. The earlier type-pollution reproduction
`bin_103` remained byte-identical in both NIR and HIR, including
`uint sub_72f1(uint param_1)`.

Regression gates:

- `cargo nextest run -p fission-midend-structuring`: 124 passed.
- `cargo nextest run -p fission-pcode`: 998 passed, 1 skipped.
- `cargo check -p fission-pcode`, `fission-decompiler`, and
  `fission-automation`: passed.
- `scripts/check/owner_boundaries.sh`: passed.
- External local Docker service built from the dirty tree and passed `/health`.
  The requested 40-row dev runner did not produce valid semantic rows because
  pyjoern's fast parser repeatedly omitted every decompiled function (and one
  row reported `adapter_error`); the isolation loop was stopped rather than
  treating adapter/coverage failures as zero semantic scores. This local run
  was not published.

## 9. Global Fixed-Point Admission Follow-up

Reference-owner comparison:

- Ghidra `CollapseStructure::collapseInternal` repeatedly applies goto,
  concatenation, proper-if, if/else, loop, and switch collapses across the whole
  graph. Only when that inner loop reaches a fixed point does it try
  `ruleBlockIfNoExit`, then restart normal collapse.
- Fission's first isolated complex-arm slice admitted the equivalent deferred
  candidate immediately at the current block. On an early entry block this can
  recursively lower a large arm before later blocks have been collapsed.

Invariant for the follow-up:

```text
A VirtualExit complex-arm candidate may reserve its stable Conditional slot
ahead of plain-if/goto, but isolated recursive execution occurs only after the
entire current SESE scan has no ordinary structured candidate. Already accepted
child regions wholly owned by an arm are inputs to that arm's isolated lowering;
they are not lowered a second time.
```

Measured motivation is the slice above: aggregate goto improved, but NIR grew
49,681 bytes and HIR grew 30,333 bytes, with per-file goto regressions up to
+11 NIR and +8 HIR. The follow-up must remeasure the same 224/250 corpus and
must not be called an improvement unless both layers retain their goto movement
without the observed broad expansion/regression pattern.

Implementation and admission:

- Deferred execution now runs only after a graph-wide ordinary-rule fixed
  point, matching the reference owner order.
- Arm-local child regions accepted before that point are reused by the
  membership-limited recursive build.
- A second isolated host reconstructs the already-admitted graph without
  another collapse pass. The complex candidate commits only when its recursive
  explicit-goto cost is no greater than that stable alternative.
- A successful candidate merges only identity-bearing binding state required
  by its emitted AST. Trial CFG mutations and structuring/lowering caches remain
  isolated; no global snapshot or rollback is used.

Measured 224-binary / 250-function result relative to the isolated-arm commit:

| Layer | Isolated-arm goto / lines / bytes | Fixed-point goto / lines / bytes | Delta |
|---|---:|---:|---:|
| NIR | 2,130 / 38,850 / 1,072,419 | 2,115 / 38,736 / 1,065,829 | -15 goto, -114 lines, -6,590 bytes |
| HIR | 2,023 / 35,931 / 973,271 | 2,009 / 35,769 / 965,428 | -14 goto, -162 lines, -7,843 bytes |

Both runs completed 224/224 binaries and 250/250 functions. Relative to the
original pre-complex-arm baseline, the final totals moved from 2,135 to 2,115
NIR gotos and from 2,034 to 2,009 HIR gotos. The focused `bin_073` row retained
its one-goto reduction in both layers. The prior type-pollution reproduction
`bin_103` remained byte-identical to the original baseline in both NIR and HIR.

The fixed-point follow-up improves goto count and reverses part of the first
slice's output expansion in both layers. It does not erase every per-file
trade-off inherited from the isolated-arm slice, so this is an aggregate
measured improvement rather than a claim that every function became shorter.

Follow-up regression gates:

- `cargo nextest run -p fission-midend-structuring`: 125 passed.
- `cargo nextest run -p fission-pcode`: 998 passed, 1 skipped.
- `cargo check -p fission-pcode`, `fission-decompiler`, and
  `fission-automation`: passed.
- Release `fission-cli` build and `scripts/check/owner_boundaries.sh`: passed.
