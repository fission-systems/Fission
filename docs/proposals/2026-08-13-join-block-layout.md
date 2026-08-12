# Join-block layout

## 1. Baseline row anchor

- Binary: DecBench sample-set `bin_039.elf`, `bin_073.elf`, `bin_209.elf`
- Command: release `fission_cli decomp` in `nir` and `hir`, plus the
  224-binary / 250-function sample-set runner
- Aggregate baseline (commit `b65c79664`): NIR 1,782 gotos / 38,294 lines;
  HIR 1,771 gotos / 35,391 lines
- Failure category: a block every predecessor *jumps* to, so it never claims
  the one free adjacency every block is entitled to

## 2. Owner proof and the structural bound

A block reached from `P` predecessors can be lexically adjacent to at most
one of them, so at least `P - 1` of its in-edges must be written as a jump.
Summing that over the corpus gives a hard floor for the current CFG shape:

| Quantity | NIR gotos |
|---|---:|
| Emitted | 1,781 |
| Structural floor, `sum(P - 1)` | 1,365 |
| **Layout loss** | **416 (23.4%)** |

The 416 are blocks whose predecessors *all* jump — they pay `P` where `P - 1`
would do. This is the entire remaining opportunity for any layout-class
transform, and it is measured rather than assumed.

The same measurement settles what is *not* worth attempting. 80.5% of the
remaining gotos target a true join (`P >= 2`). Those cannot be removed by
duplication (one copy per predecessor), by threading (several predecessors
must reach one copy), or by layout beyond the single free adjacency. Getting
below the 1,365 floor requires *changing the CFG shape* — fewer joins — which
is a structuring-quality problem, not a goto-rewriting one.

## 3. Reference invariant

Ghidra makes this exact decision in `ActionFinalStructure`
(`blockaction.cc:2191`):

```cpp
graph.orderBlocks();      // FlowBlock::compareFinalOrder
graph.finalizePrinting(data);
graph.scopeBreak(-1,-1);
graph.markUnstructured(); // gotos are decided *after* ordering
```

Layout is chosen first; jumps are whatever the layout could not make
adjacent. Fission emits residual blocks in index order with no such step, so
a block can end up with every predecessor jumping to it. This pass applies
the ordering principle at the level where the residual placement survives.

Generalized rule:

```text
When a labelled block has no adjacent predecessor -- nothing falls into it --
and its statements end the path, relocate it to sit immediately after one of
the jumps that reaches it, replacing that jump. The label stays with the
block so the remaining predecessors still reach it.
```

## 4. Why the move is sound

- **Nothing falls in**: every predecessor is a jump, which is the admission
  condition, so deleting the block from its old position removes no reachable
  path. Index 0 of a sequence is refused, since control enters there by
  entering the enclosing block.
- **Nothing falls out**: the region must end in `Return` or `Goto`, so
  wherever it lands it terminates the enclosing sequence exactly as the jump
  it replaced did.
- **The destination is a sequence end**: only a jump that is the last
  statement of its sequence may be replaced, so the sequence keeps its exit
  behaviour and no statements behind the jump are revived.
- **No rebinding**: `Break`/`Continue` anywhere in the region are refused,
  because they bind to the nearest enclosing loop and the move can change it.
- **Termination**: each label is relocated at most once per function, so the
  block cannot shuttle between predecessors.

Labels *nested inside* the moved region travel with it and keep their single
definition; their in-jumps still resolve, since Fission emits all locals at
function scope and C labels are function-scoped.

- [x] AST shape only; no binary, compiler, or name guard.
- [x] Synthetic coverage uses hand-built statement trees.

Comparable coverage (7 unit tests): relocation retires exactly one jump;
a second predecessor keeps jumping to the moved label; fallthrough-reachable
block refused; region that falls out refused; region containing `break`
refused; protected label refused; predecessor whose jump does not end its
sequence refused.

## 5. Validation matrix

- [x] Unit tests: `cargo nextest run -p fission-midend-structuring
  join_layout` — 7 passed.
- [x] Crate gates: `cargo nextest run -p fission-midend-structuring
  -p fission-pcode` — 1,163 passed, 1 skipped, no expectation changes.
- [x] Ground-truth semantics: `count_bits_matches_real_machine_code` passed.
- [x] Full sample-set rerun, both layers, release CLI, against a baseline
  regenerated from the committed parent.

## 6. Measured result

224/224 binaries and 250/250 functions in both layers:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 1,782 | 1,629 | **-153 (-8.59%)** | 38,294 | 38,153 (**-141**) |
| HIR | 1,771 | 1,615 | **-156 (-8.81%)** | 35,391 | 35,121 (**-270**) |

Per-file: NIR **56 improved, 0 regressed**; HIR **55 improved, 0 regressed**.
Largest gains: `bin_039` -15, `bin_209` -12, `bin_073` -10, `bin_190` -8.

Both layers improve and lines fall, as expected for a transform that moves
code rather than copying it. 153 of the 416 available layout wins are taken;
the remainder are blocked by the admission rules above (regions that fall
through, predecessors whose jump does not end a sequence, `break`/`continue`
in the region).

## 7. What remains

After this pass the corpus sits at 1,629 NIR gotos against a structural floor
of roughly 1,365 for the current CFG shape. Ghidra reaches 716 on the same
functions — *below* Fission's floor — which means the remaining gap is not
reachable by any goto-rewriting pass. It requires producing fewer join points
in the first place: condition merging, node splitting, and richer region
schemas in the structuring engine itself.
