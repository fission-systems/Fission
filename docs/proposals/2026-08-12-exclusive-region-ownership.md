# Exclusive region ownership across achieved exits

## 1. Measured row anchor

The current release CLI on DecBench `bin_039.elf`, function `0x8680`, produces:

| Layer | Gotos | Logical lines | Bytes |
|---|---:|---:|---:|
| NIR | 93 | 1,080 | 28,350 |
| HIR | 88 | 1,023 | 26,724 |

The raw CFG has 122 blocks, no irreducible SCC, and maximum SCC size 1. The
structuring diagnostic nevertheless ends with:

```text
final reconstruction: duplicate block ownership block=10 existing_node=0 attempted_node=1
SESE structuring failed, falling back to linear: UnsupportedCfgRegionShape
```

This one fallback accounts for 4.4% of all current NIR gotos and is therefore
a higher-leverage anchor than another printer-level cleanup.

## 2. Canonical owner and root cause

The owner is `fission-midend-structuring` SESE composition/admission:

- A child originally bounded by `[8, 119)` is composed into an enclosing
  `[0, 85)` region after its achieved exit expanded beyond the original tree
  boundary. Its proof owns blocks 8 through 118.
- Sibling child proofs rooted at 21, 25, 41, 50, and 60 remain in the same
  `child_map`, despite being contained by that achieved owner.
- A later virtual-exit candidate rooted at 0 owns a non-contiguous set that
  crosses the existing `[8, 119)` child. The child is not wholly contained in
  either arm, so it was not supplied to isolated lowering, but admission does
  not reject the partial overlap.
- Final reconstruction is the first point that enforces exclusive ownership,
  so the entire structured result is discarded instead of rejecting/pruning
  the conflict earlier.

## 3. Reference invariant

Ghidra's `BlockGraph::identifyInternal` moves every collapsed component out of
the parent graph before inserting the replacement block. A component cannot
remain independently surfaceable after another structure owns it.
`CollapseStructure::collapseInternal` then repeats rules over that updated
graph.

Fission's equivalent invariant is:

```text
At every composition/admission boundary, active RegionProof.members sets are
pairwise disjoint. An achieved child owner replaces later overlapping sibling
children. A speculative virtual-exit candidate is admissible only when every
intersecting existing child is wholly contained in exactly one arm and is
therefore supplied to that arm's isolated lowering.
```

This is CFG ownership logic, independent of ISA, binary identity, and rendered
label names. It does not require global builder rollback.

## 4. Experiments and stop decision

Three ownership-only experiments were rejected after inspecting the focused
NIR/HIR diff:

1. Keeping the earlier expanded child and pruning contained siblings removed
   conditions on error paths, despite reducing NIR gotos from 93 to 82.
2. Letting the later isolated virtual candidate replace intersecting children
   produced an unconditional goto where the baseline had a conditional goto.
3. Dropping only the overlapping ownership component to raw residual retained
   a disjoint child whose lowered if/else duplicated the same body in both arms
   and lost the `local_b0` guard.

The final duplicate-ownership rejection is therefore currently a necessary
semantic safety barrier. No ownership-only implementation from this proposal
may land. The next prerequisite is a semantic completeness proof for child
conditional lowering, including preservation of every arm predicate and exit,
before localized SESE fallback can replace whole-function linear fallback.

## 5. Validation matrix

- Focused release rerun of `bin_039` in NIR and HIR.
- Diff every changed conditional and error path, not only goto totals.
- Require a child-level predicate/exit coverage proof before changing ownership
  conflict handling.
- Type-pollution and complex-arm sentinels `bin_103` and `bin_073` remain
  byte-identical unless their goto structure is directly and safely improved.
- Structuring and full pcode tests, relevant crate checks, release build, then
  full 224-binary / 250-function DecBench NIR and HIR measurement.
