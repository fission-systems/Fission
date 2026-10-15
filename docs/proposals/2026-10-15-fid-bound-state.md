# Reuse the bound instruction while FID requests operand p-code

Mechanical performance work. Baseline main 416f441a1c341bb6ffddec081b2492ddb969f0ac / immutable executable 26dc790eed98282e603e8ee6b3ad91ba40590c9c61b25c51a25d72f38bdb8d0c. Three prior prototypes (borrowed error frames, probe buffer, equal fallthrough reuse) were rejected after complete real controls showed no useful latency gain; none is included.

## Measured owner evidence

The new continuous-process diagnostic at benchmark/artifacts/performance/2026-10-15-whole-preparation-profile/whole.txt exposes RuntimeSleighFrontend::fid_hashes with compiled-table binding/lifting, in addition to raw preparation. Source fid_hash.rs first obtains an owned raw binding for the exact instruction bytes/address/default compiled context, then calls decode_and_lift_with_details again when an operand needs p-code. Multiple unresolved operands can repeat that full decode/lift. These are owner observations, not whole-run percentages.

## Invariant / change

An instruction's owned binding already carries the result of the same bytes/address/default context/spec selection. Reuse it to complete the ordinary p-code/description stages without selecting/binding again. Preserve context-commit validation, length checks, exact emitter FlowEmitOptions, default context, byte/memory base, userop/details transport and native instruction-display validation; a successful emit alone must not newly make an old failed lift/hash eligible. The existing description helper only reads state and may borrow it. Preserve normal decoder error-selection boundaries.

Within one FID instruction, retain the successfully completed p-code for subsequent operand tracing. Do not cache final matches, resolver-dependent mixes or full hash digests. Preserve operand iteration and register-resolver call order, hash inputs, None/failure policy, caps and address classification. No new program/global cache, dependency, byte budget, CLI/schema, archive format or ISA guard. Under existing terminal reselect/verify/build-debug flags retain the old replay path/count so diagnostic logs stay comparable.

## Verification / acceptance

Before implementation, capture the existing multi-ISA raw-state and full lift tuples/errors plus FID hashes; compare after with the new completion path. Direct complete-bound vs original rebind tests must include success/failure, memory/register/SIB/RIP, context commits, crossbuild/delay behavior and description failure controls; resolver callbacks keep their order. Existing Ghidra FID vectors remain unchanged. Targeted -> related full suites -> checks/CI Clippy/release/fmt/owner scan. Freeze exact source/binary, clean alternating wall/CPU/RSS controls without own build/profiler overlap, fresh 250 complete non-time outputs, repeated worker lanes and fixed Kuna comparison. Retain only a useful measured improvement, with all memory/time tradeoffs and measurement scope explicit. No quality/ranking or recovered-program execution claim.

## Measured outcome

Accepted after 3 paired large-input controls: wall -9.2%, CPU -12.5%, maximum RSS +0.6%. Small inputs show no material gain. Fresh 250 complete non-duration outputs are unchanged. See ../validation/fid-bound-state-performance.md and its compact evidence. Nine targeted / 1,956 related tests pass (six skipped). Fixed Kuna rerun remains 2.81–7.23x faster; no parity/quality/ranking claim. Dedicated memory/context/delay and invalid-description controls are included; crossbuild dispatch/options are unchanged but no dedicated crossbuild fixture was exercised. The pre-implementation proposal snapshot is retained with the artifacts.
