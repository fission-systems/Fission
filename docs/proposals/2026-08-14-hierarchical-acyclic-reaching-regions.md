# Hierarchical acyclic reaching-condition regions

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_068.elf`
- Function/address: `sub_8070` at `0x8070`.
- Measured CFG: 50 blocks, 74 edges.
- Shipped output: NIR 26 gotos; HIR 26 gotos.
- References on the same function: angr 2 gotos; Ghidra 4 gotos.
- DREAM without restored virtual transfers stops on an unlowerable `IfNoExit`.
  The virtual-transfer candidate reaches 4 raw gotos but is correctly rejected:

```text
DREAM+virtual-gotos rejected: gotos 31 -> 4,
guards 86 -> 3776, max 4 -> 553,
worse on ["guard_formula_size"]
```

The candidate removes transfers but expands local decisions into whole-function
path formulas. Relaxing the guard comparator is therefore not the fix.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Reaching-condition region identification
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

After loop folding, `reaching_driver::describe_region` computes one condition
system over every live node in the function. angr's `RegionIdentifier` instead
identifies and abstracts the smallest acyclic regions repeatedly before the
outer region is structured. The missing hierarchy is in the Fission-owned
reaching driver, before condition materialization and emission.

## 3. General Invariant

In an acyclic graph, a subgraph may be emitted and collapsed independently when:

1. one head owns every edge entering the member set;
2. every edge leaving the member set reaches the same real follow node;
3. every member is reachable from the head without crossing the follow; and
4. no member terminates before that follow.

The implementation starts with branch-headed regions, chooses the smallest
valid common-descendant follow, emits reaching conditions only over those
members, then replaces them with one live graph node. The rule uses only graph
reachability, entry ownership, and exit closure; it has no binary, compiler,
address, or ISA predicate.

## 4. Safety Contract

- Region discovery is pure. Lowering starts only after entry and exit closure
  have been proven.
- `CollapseGraph::check_single_entry` remains the authoritative side-entry
  proof, and `collapse` rechecks it on commit.
- All member bodies and branch conditions must be placeable; any failure leaves
  the graph unchanged and falls back to the existing whole-region driver.
- The outer isolated/observed host contract remains unchanged. No rollback or
  shared speculative builder mutation is introduced.
- Existing nesting, switch, empty-shell, total-guard, maximum-guard, raw, and
  cleaned candidate comparison still decide whether the complete result ships.

## 5. Validation Matrix

- [x] Pure positive diamond, nested-region ordering, side-entry negative, and
      multiple-exit negative tests.
- [x] Focused `bin_068` NIR/HIR diagnostics and guard sizes.
- [x] Control rows: `bin_021`, `bin_063`, `bin_179`, `bin_190`, `bin_217`, and top gap
      rows currently declined for guard growth.
- [x] `cargo nextest run -p fission-midend-structuring -p fission-pcode`
      (1237 passed, 1 skipped).
- [x] `cargo check -p fission-pcode` and `cargo check -p fission-decompiler`.
- [x] DecBench 224 binaries / 250 functions, NIR and HIR, per-file comparison.
- [x] Phase-2 corpus ground truth with zero divergence.
- [x] Owner-boundary scan passed; fast benchmark-smell scan found zero smells.
- [x] External local Docker benchmark, non-published: 10/10 adapter-clean,
      100% coverage, 10/10 semantic attempts, `valid=true`.

The full workspace run finished with 2497 passes, 5 skips, and the same seven
pre-existing `fission-emulator` failures at the SLEIGH `addr64` decode gap. No
new failure was introduced.

## 6. Quality Claim Gate

The motivating row remains unchanged at 26 gotos: its hierarchical candidate
still concentrates too much control into guards and is correctly rejected.
That is a safety result, not a quality gain. The same invariant independently
improves `bin_103` from 25 to 1 goto in both NIR and HIR. Full-corpus measured
totals move NIR 1037 to 1013 and HIR 1029 to 1005, with no per-file goto
regressions. On the 204-function three-tool intersection, Fission now emits
971 NIR and 963 HIR gotos, against Ghidra's 691 and angr's 420.

The accepted `bin_103` result increases source bytes from 30,541 to 37,609 and
lines from 1,015 to 1,048 while removing 24 transfers. Its maximum guard shrinks
from 859 to 162 nodes. In contrast, the raw hierarchical candidates for
`bin_068`, `bin_179`, and `bin_217` are rejected by the raw and cleaned
single-guard admission check; `bin_217` therefore stays at 2 gotos instead of
regressing to 3 after cleanup.
