# Transient no-return / FID raw decode reuse

Mechanical performance work, not a semantic/readability/ranking change.

## Measured anchor / owner

Baseline main 533c80c94aedc578ab53c0dea77c55243764b2a7 (tree identical to a2bda6d744ce599f5c91bdd0eb9b9bd6fa7a8936). Immutable release SHA256 a4e3eafa56886bfc9dc7948364b291bc683dd69e2327c0e587eda1c6467f5e6d. Prior large six-request median 29.9963 s / 2,409,381,888 RSS bytes. Separate symbolized native samples at 12/24 seconds still show sequential ControlFlowFacts::assemble / no_return_functions_for holding global preparation while FID callers wait. See benchmark/artifacts/performance/2026-10-15-decode-storage/startup-large-profile-{12,24}.txt. This is a hypothesis about duplicated work, not a measured cache hit rate yet.

## Invariant / ownership proof

Raw instruction execution consumes the compiled frontend, exact remaining byte window, PC and full context override. DecodeMemoryContext changes CFG/reachability after raw decoding. Reuse only success tuples captured before op renumbering and annotation; rebuild the complete FID graph with its original context, limits and inputs. Own and validate the complete function byte window and entry, language/EntrySpec/status, retained compiled Arc identity, and per-instruction PC + Option(context bits, mask bits). Changed windows/content/context are ordinary misses. Do not reuse no-return CFGs, failed results or guess a tighter key from consumed bytes.

The runtime owns opaque, consuming per-function seeds. Existing bincode/serde encodes instruction/ops/length/template source/context commits; userop catalog identity must match the retained frontend catalog and is restored by Arc, never serialized per instruction. Exhaustive details destructuring/literals force field additions to be handled. Codec failure on capture skips that record; private immutable archive corruption fails loudly. Each record is removed on reuse; repeated requests fall back to native decode.

Only a FID transaction winning cold CF and no-return initialization captures seeds. Ordinary callers and both global LRU caches retain no raw traces. Warm caches provide no seed and force no recomputation. FID drains one function seed outside the map lock and drops leftovers after ingestion. Keep no-return sequential; no nested Rayon under OnceLock. Before capture, project its temporary full PcodeFunction corpus into ordered Return/Call-or-Branch-target events and successors, preserving the existing predicate exactly (including its offset-only target test and malformed successor behavior). This replaces retained data, not the fixed-point policy.

## Validation / stop conditions

Differential old/new no-return predicates across ordered terminators, known-target subsets, cycles and invalid successors. Runtime complete tuple/function equality; changed byte windows/content/frontend/entry/context, context commits, sequence numbers, additional CFG entries and decode errors. Warm-cache no-recompute and transaction lifetime tests. Focused nextest, relevant full suites/checks, release, fmt and owner scan. Frozen 250 stripped-function fresh replay must preserve every non-time record under the existing seven-duration exclusion only. Alternate immutable baseline/candidate runs with no own concurrent builds/profilers, measure wall/CPU/RSS and deterministic repeats. Record actual eligibility/hits with local probes, outside timed runs. Reject regressions or unproven key simplifications; report Kuna comparison as bounded local timing only.

## Accepted local evidence

Implemented the conservative transaction-local path with exact full-window/frontend/context checks and consuming raw records. Nine focused tests and 1,847 related tests pass (six skipped). Fresh 250-function replay has zero changed non-time records. Three alternating large pairs: wall median 31.5073 -> 26.9139 s (-14.6%), CPU 141.8253 -> 118.2984 s (-16.6%), RSS 2,294,677,504 -> 2,325,839,872 bytes (+1.4%). Small medians differ by only 0.5–2.5%. Pinned Kuna remains 2.96–10.76x faster across four serial batches. Whole-function hot-cache memory growth remains unchanged. See docs/validation/transient-decode-performance.{md,json} for exact source hashes, measurement bounds, context probes and separate native profiling.
