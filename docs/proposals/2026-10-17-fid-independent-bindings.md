# Reuse native FID bindings with a witnessed address-independent path

Mechanical performance work against main `3082064e747cd73dda2dd98777a5eb25e6c1c9d5`. The original proposal, diagnostic patch and pre-implementation controls are retained in `benchmark/artifacts/performance/2026-10-17-fid-independent-bindings/`. This document consolidates that proposal and the final ownership review. No decompilation-quality, execution or ranking claim.

## Observation and owner

A native probe on the frozen qualification corpus's large input reached 1,921,487 FID binding requests, all at distinct addresses. There were 219,980 exact byte sequences. An address-use witness over the actual native selection/binding path found 1,561,875 successful requests that did not consume PC, from 42,436 byte sequences; 359,612 consumed PC and no binding failed. Complete six-function records remained unchanged after seven duration exclusions. Instrumented runs establish work duplication, not latency or CPU percentages.

`RuntimeSleighFrontend::fid_hashes` calls the compiled-table FID owner, which binds each instruction's isolated bytes at the compiled default context. Existing shared function-lift/raw-p-code storage does not share these FID-local bindings. The implementation therefore belongs in `runtime/spine/compiled_table`, with immutable frontend ownership supplied by the existing runtime facade.

## Invariant and proof

For the same immutable frontend, exact isolated bytes and default context, a complete successful native binding can be reused at another PC only when its entire selection/bind attempt never consumed PC and every observed checked PC-plus-cursor addition succeeds at both addresses. Include discarded/error candidate paths in the witness. The source-address requirement prevents a discarded overflow from silently selecting a different winner when reused at a lower PC.

Record actual PC reads in the shared InstNext getter and the InstStart export/expression arms. Record checked subtable address additions. Parser address seeds do not survive in `RuntimeConstructState`; other selection, token and handle inputs are bytes, cursors, context and immutable metadata. New PC reads at this owner must extend the witness and its tests. The proof depends on these owner boundaries; byte repetition alone is insufficient.

Only the runtime's immutable frontend Arc establishes the thread-local scope. A retained Weak identifies the allocation without owning the frontend; COW changes identity. Nested scopes restore on unwind and cannot cross threads. Callers without this ownership proof retain native binding.

## Retention and effects

Worker-local direct map: 256 slots, one frontend, exact keys up to 64 bytes, at most 128 state nodes and a 64 KiB owned-payload estimate per entry, 2 MiB total payload estimate per worker. Bounded probe traversal and owned strings/vectors are accounted for. Shared immutable compiler metadata, fixed table storage and allocator overhead are outside the estimate; RSS is measured separately. Owner replacement or budget overflow clears entries. Key collisions, over-budget states, PC-sensitive paths, failures and address overflow retain native behavior; analysis work limits are unchanged.

Retain the complete immutable constructor state through Arc. Clear the existing per-instruction BIND_CACHE on every lookup, including hits. Trace modes retain native replay. Hash operand traversal, current-PC p-code completion/descriptions, masks, register resolver calls/order, digest mixing and failure paths remain in their original owner. Do not cache p-code, resolver results, hashes or final matches.

No ISA-specific equivalence rule, unsafe code, production dependency, archive/CLI/JSON change, runtime vendor dependency or parallel program-fact map. Ghidra `context.hh` and `sleigh.cc` were consulted only for context/constructor-state identity invariants; no implementation was copied.

## Validation and acceptance declared before timing

- Exact shared/native states across addresses; InstStart/InstNext sensitivity; discarded checked overflow; malformed/missing bytes; stale bind-cache clearing.
- Frontend COW/replacement, nested scopes/unwind/thread isolation and reentrant resolver behavior; collisions, eviction and retention limits.
- Four-language complete-state differential plus canonical raw-state/lift/error and FID hash/resolver-order matrices. Explicitly enable immutable scope in the latter matrix.
- Targeted nextest, all related crate nextest/checks, scoped Clippy, release CLI, workspace fmt and owner scan.
- Freeze exact source and binaries. Three alternating large-input pairs require >=2% separate-median wall reduction; five independent pairs with inverted starting order require >=2% median paired wall reduction and <=2% CPU regression. Report RSS. Reject and restore a failed candidate.
- After acceptance: small paired controls, complete identity and function-list comparisons, fresh 250-function non-duration replay, integrated detection and fixed Kuna 1.808 at both worker settings. No own build/test/profiler overlaps timing. OS scheduling/caches remain uncontrolled.

Preliminary controls/diagnostics used Rust 1.96.1. Final full validation and both accepted comparison binaries use Rust 1.97.0 (`2d8144b7880597b6e6d3dfd63a9a9efae3f533d3`), with a separately built clean-main baseline. Historical 1.96.1 times are excluded from this source-only speed comparison. The previous nine rejected experiments were themselves compiler-matched and remain rejected.

See the [validation report](../validation/fid-independent-bindings-performance.md) for actual results and limits. Commit only an accepted change from linked main, preserve unrelated work, push a review PR and merge after exact-head required CI.
