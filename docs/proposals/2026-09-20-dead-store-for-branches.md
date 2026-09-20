# Decompiler Change Proposal: Dead-Store Removal Across For Headers

Date: 2026-09-20
Issue: #33

## Owner and defect

The canonical owner is
`crates/fission-midend-normalize/src/global_opt/dead_store.rs`.
`DeadStoreCollector` records `For::init`, `For::body`, and `For::update`
paths using branch indices `0`, `1`, and `2`, but `recurse_remove` only routes
branch `1` into the loop body. Dead stores in either header are therefore
silently retained even though the collector proved them removable.

## Invariant

Every path emitted by the collector must be consumed by the corresponding
statement-tree branch. A single-statement optional header is removed only when
its own path is selected; nested statements in a header continue through the
same recursive path walker. The body keeps its existing vector-based removal.

No semantic behavior is changed: only stores already proven dead by MemSSA
are removed.

## Regression and validation

Add a `For` fixture with independently dead init and update stores, assert both
are removed, and keep the existing body/escape tests. Run:

- `cargo nextest run -p fission-midend-normalize dead_store`
- `cargo nextest run -p fission-midend-normalize`
- `cargo fmt --all --check`
- `git diff --check`
