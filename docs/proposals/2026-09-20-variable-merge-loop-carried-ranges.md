# Loop-Carried Variable Merge Ranges

## Baseline / issue anchor

- Issue: #39
- Owner: `crates/fission-midend-normalize/src/recovery/variable_merge.rs`
- Current defect: `LiveRangeCollector::extend_loop_ranges` only widens ranges
  that started before a loop, so a value first read and then updated inside a
  loop can be merged with an earlier temporary.
- Observable invariant: a variable read and written within one loop region is
  conservatively live across the loop back-edge and must not share storage with
  another value whose range ends earlier in that region.
- This is a mechanical semantic-correctness repair. The focused regression is
  synthetic and is not presented as a broad decompiler-quality measurement.

## Owner proof

The incorrect merge is created in the normalize variable-merge owner. The
collector records only the first and last lexical occurrence of each variable;
the loop extension predicate has no read/write information and therefore
cannot recognize a value carried from one iteration to the next. The later
merge decision uses those intervals directly.

## Generalized rule

Record read and write statement positions independently while collecting ranges.
When closing a structured loop or resolving an unstructured back-edge, widen a
variable to the complete loop interval if it is both read and written inside
that interval. Keep the existing live-in widening rule for values that began
before the loop. The rule is based on structured control-flow/dataflow facts,
not an ISA, function, address, or binary identity.

## Risk and ownership

- Extend the existing collector; do not add a new pass or public API.
- The rule is deliberately conservative for loop-local temporaries that are
  initialized and consumed in the same iteration: preventing an unsafe storage
  merge is preferable to assuming iteration independence from lexical ranges.
- Existing non-loop disjoint-range merging and explicit copy barriers remain
  unchanged.

## Validation matrix

- A loop-carried local first appearing inside the loop must not merge with an
  earlier loop temporary.
- Existing structured and unstructured loop live-range tests remain green.
- `cargo nextest run -p fission-midend-normalize`.
- `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, format,
  and diff checks.
- Report the result as a correctness/mechanical repair unless a separate real
  corpus measurement is performed.
