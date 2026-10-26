# Remaining PR admission corrections (2026-10-26)

## Baseline row anchor

Baseline commit: 5cf2a8c11. Real corpus observation: external dev PE
`string_utils_gcc_O0.exe`, reverse_string at 0x140001530, source
`corpus/dev/source/c/string_utils.c` in the external benchmark repository.
The debug CLI `decomp --layer both --no-db --json` succeeds with a 501-byte
selected body and zero gotos. Raw P-code for 40 instructions is retained.
Artifacts live under `benchmark/artifacts/pr-review-final-2026-10-26/`.
The baseline Linux CLI is independently built from that clean commit; external
local runs disable caches and checkpoint reuse. The first external three-row baseline produced 5/15 semantic cases passed, with
compile errors on two rows. No semantic or readability quality claim is made.
This corpus row is a no-regression observation, not evidence that the synthetic
failure shapes occur in that row.

## Owner and invariant proof

- Normalize double_precision (#677): comparing structurally identical pointer
  expressions does not prove repeatable evaluation. Two pointer calls or loads
  may differ; merging accesses must not reduce their evaluation count. Reject
  memory-dependent pointer expressions through the existing owner predicate.
- Normalize byte_sum_index (#678): an i8 sum copied into i32 must sign-extend.
  `& 255` changes -56 into 200. A byte-sized operation alone is not evidence of
  zero extension. Restrict wider-copy masks to unsigned byte operation AND
  unsigned binding (or no contradicting declared binding); truncating destination assignments remain admissible.
- SLEIGH context (#587): preserve the existing 64-bit runtime and fail-closed
  word bounds. The user explicitly excluded #176; no FSL or context widening
  from that PR is included. Expansion remains an open follow-up.
- SLEIGH BUILD exports (#587): exports belong to the active constructor scope.
  Save and restore the map on recursive entry, resolve the child's export while
  its own scope is active, then publish only that result into the parent. Do not
  select another scope using pointer-address ordering.

These are common evaluation, integer-width and constructor-scope invariants.
There are no ISA/function/address conditions, new passes, metrics or production
owner dependencies. Existing helpers and owners suffice. The normalizer changes
only narrow optimization admission; they are mechanical corrections supported by
counterexamples, not a new quality pass or a measured quality gain.

## Risk and validation

Preserve ordinary pure LE/BE word pairs, unsigned byte copies, supported low
context words, caller export maps, and raw x86 P-code. Exclude the FSL crate and all #176 branch changes. Keep issue #519 open for
its separate carrier-model question; do not revive rejected pointer propagation.

Validation matrix:

1. Red/green invariant tests for signed widening, pointer call/load evaluation,
   nested/sibling BUILD scopes and error-path restoration.
2. Full normalize/pcode/SLEIGH and workspace nextest; check/decompiler build;
   formatting and the exact CI clippy flags.
3. SLEIGH runtime/packed-context and raw-byte coverage; no context expansion or
   inference of whole-program equivalence.
4. Compare raw P-code and the real corpus observation against the saved baseline.
5. External Docker local smoke on the same rows with caches disabled; retain
   metrics/output and explicitly report missing oracle coverage.
6. Require final-head GitHub CI for affected packages and CLI smoke. The native
   debugger failure on excluded #176 is outside this candidate.

## AI review and limits

No external model or delegated agent is used. Names/addresses above document
measurement provenance only and never enter implementation conditions. Synthetic
counterexamples supply invariant coverage. No benchmark ranking, release tag,
or official latest/Pages update is authorized by this admission work.
