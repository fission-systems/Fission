# Analysis-DB Address Interval Indexes

## Baseline / issue anchor

- Issue: #32
- Owner: `crates/fission-analysis-db/src/lib.rs`
- Current defect: containment queries scan every function or memory block, and
  `build_functions` repeats a memory-block scan for every function.
- Observable invariant: address queries return the same deterministic record as
  the current scan while avoiding a full-table walk for the normal disjoint
  interval case.

## Owner proof

`ProgramSnapshot` owns the sorted immutable function and memory-block tables,
and the O(n) scans are implemented directly in its query methods and snapshot
builder. No loader or decompiler layer needs to own a second address index.

## Generalized rule

Build private sorted interval indexes at snapshot construction. Each index
stores intervals sorted by start address plus a prefix maximum end address, so a
query binary-searches the latest possible start and scans only overlapping
candidates. Functions preserve exact-entry precedence and choose the smallest
containing range with stable-ID tie breaks; memory blocks preserve the first
canonical block order. Invalid/overflowing or zero-sized ranges are omitted
from the index just as the old predicates omitted them.

The indexes are derived, skipped from serialized snapshot data, and do not
change the public fact tables or their schema.

## Validation matrix

- Existing range-query and snapshot-integrity tests remain green.
- Overlapping interval tests preserve function specificity and block order.
- `build_functions` uses the block index for import filtering and ownership.
- `cargo nextest run -p fission-analysis-db` plus downstream checks, format,
  and diff checks.
