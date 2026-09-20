# Decompiler Change Proposal: Sound CircleRange Widening

Date: 2026-09-20
Issue: #34

## Owner and defect

The canonical owner is `crates/fission-midend-prehir/src/vsa/circle_range.rs`.
`CircleRange::widen` currently compares only cardinality. Equal-sized sliding
arcs can therefore replace the previous approximation without containing it,
and a shrinking arc can narrow the current approximation. Both behaviors break
the widening contract used by the VSA solver at loop back-edges.

## Invariant

For `new.widen(old)`, the result must contain both `new` and `old`. If the new
arc is contained in the previous arc, retain the previous arc; otherwise jump
to `top`. This is the standard finite-height escape needed to guarantee
convergence for growing or laterally moving loop ranges. Top and bottom retain
their lattice identities.

The implementation will use modular arc distance, not architecture,
instruction, function, or benchmark-specific conditions.

## Focused regression

Add direct `CircleRange` tests for:

- equal-cardinality lateral sliding ranges widening to top;
- shrinking ranges retaining the previous approximation;
- growing ranges widening to top;
- contained wrapping arcs remaining precise;
- top and bottom boundary behavior.

These tests establish the abstract-domain contract. They are mechanical
correctness evidence; no decompiler readability or benchmark-quality claim is
made from them alone.

## Validation

- `cargo nextest run -p fission-midend-prehir`
- `cargo check -p fission-midend-prehir`
- `cargo fmt --all --check`
- `git diff --check`
- workspace consumers as needed after the focused gate
