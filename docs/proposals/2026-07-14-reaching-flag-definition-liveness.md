# Reaching Flag Definition Liveness

## Baseline

- Unstructured x86 HIR can contain many writes to the same condition-code
  binding while only one reaching definition is consumed by a later branch.
- The current cleanup counts uses by flag name across the whole function. A
  single surviving `cf` use therefore keeps every unrelated `cf` definition,
  including definitions whose operands have otherwise disappeared.
- The observable O0 failure is undeclared or stale register material in emitted
  C. Optimized timeout behavior is a separate structuring-budget problem and is
  not part of this semantic change.

## Owner Proof

- Raw p-code contains explicit flag writes and control-flow transfers; the lift
  is not missing those operations.
- Flag condition reconstruction and dead flag cleanup already live in
  `normalize/recovery/flag_recovery.rs`.
- The missing fact is definition-specific liveness across labels, gotos,
  branches, and structured loop bodies. Global name-use counts cannot express
  that fact.
- Program-wide loader metadata remains owned by `fission-analysis-db`; HIR
  liveness must not be added to that crate.

## Invariant Proof

1. A pure flag assignment is retained only when that exact definition can reach
   a use of the same flag along a modeled control-flow path.
2. A later definition kills an earlier definition on the same path. Definitions
   from distinct predecessors are unioned at joins.
3. Unknown calls and other potentially effectful right-hand sides are never
   removed by this cleanup.
4. The analysis is driven only by HIR control flow and def-use facts. It does
   not inspect function names, addresses, binaries, corpus rows, or compiler
   tuples.
5. Optimized structuring timeouts remain governed by an independent budgeted
   recovery slice; this change must not claim to solve them.

## Validation Matrix

- Synthetic straight-line overwrite: only the latest reaching flag definition
  survives.
- Synthetic branch join: definitions from both predecessors survive when the
  joined condition reads the flag.
- Synthetic goto edge: a definition reaching a labeled use survives while an
  unrelated overwritten definition is removed.
- Side-effect guard: an unknown call assigned to a dead flag remains.
- `cargo nextest run -p fission-pcode --no-fail-fast`.
- `cargo nextest run -p fission-analysis-db --no-fail-fast`.
- Focused local O0 semantic row and emitted-C inspection for the affected stream
  function, with O2 timeout measured and reported separately.

## Analysis Database Strengthening

- Add deterministic snapshot integrity diagnostics for typed IDs, cross-table
  references, address ranges, and canonical ordering.
- Add provenance/confidence inventory and range-oriented queries without
  introducing mutable graph state or HIR-specific facts.
- Preserve the serialized v1 snapshot shape; the added API derives facts from
  existing immutable records.
- Route the program-metadata CLI through the validated constructor so corrupt
  cross-table facts fail closed instead of being serialized as canonical data.

## Validation Results

- `cargo nextest run -p fission-pcode --no-fail-fast`: 1,186 passed, 9 skipped.
- Flag recovery focused set: 15 passed, including overwrite, label/goto kill,
  two-predecessor join, nested-branch, and effectful-RHS cases.
- `cargo nextest run -p fission-analysis-db --no-fail-fast`: 7 passed.
- `cargo nextest run -p fission-static -p fission-decompiler --no-fail-fast`:
  107 passed, 1 skipped.
- The local Linux adapter was rebuilt from source fingerprint
  `75176f90d9ca8655ef63fd574ca2a153c4d18c22c1f9217e4be5533b3ab4ca0b`.
  The x86-64 GCC O0 stream row passes 5/5 semantic cases, and emitted C contains
  no `cf`, `rsp`, `__carry`, `__scarry`, or `__sborrow` residue.
- The x86-64 GCC O2 row also passes 5/5 and completes below the adapter timeout.
  No structuring-budget production change was made because the timeout is not
  reproducible on the current source fingerprint.
- Across all eight compiler variants, GCC x64 O0/O2, Clang x64 O2, and Clang
  x86 O2 pass 5/5. The remaining x86 O0 and Clang O0 failures are
  pointer/scalar state collapse, while GCC x86 O2 remains a runtime-semantic
  failure. They stay in a separate variable-split slice.

## AI Use

- No external model proposal is used.
- The implementation is specified in terms of reaching definitions and CFG
  joins, not the benchmark row identity.
- Reference decompiler output is correctness evidence only; its formatting or
  local-variable style is not copied.
