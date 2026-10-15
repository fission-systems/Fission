# Reduce repeated decompiler preparation work

Mechanical performance work on the PR #204 head `efe35ee69`; no quality, admission, naming, worker-policy or output-contract change.

## Measured anchor

Frozen qualification revision `07dd95dbe6214b8a78fc1e4a55961ab515986489`, same stripped inputs and explicit address sets, three alternating serial fresh-process samples against pinned Kuna 1.808: Linux 2.3093 / 0.1886 s, Windows 1.2353 / 0.1104 s, macOS 1.4796 / 0.3305 s. All requests returned. Raw evidence lives in `benchmark/artifacts/performance/2026-10-14-kuna` in the primary checkout.

Immediate native samples on the same source show FID preparation before rendering. Windows samples include `LazyFidDatabase::has_force_relation_candidate` and `FpkReader::records_by_key`; macOS also spends substantial preparation in the canonical whole-program no-return analysis. The latter is not bypassed. The previous large Tracy request took 149.095 s for six outputs whose individual final builder durations total only 140 ms; those timers overlap and do not identify the entire gap by themselves.

## Initial FPK experiment (excluded): owner and invariant

`FpkReader` already retains immutable decompressed payload blocks. Its hash directory refers to independently compressed index chunks, but `records_by_key` decompresses and validates a chunk on every lookup, including misses. FID preparation queries the same small directory population repeatedly across all image functions and matching phases.

Retain successfully validated raw index bytes per reader/chunk, using the existing mutex/Arc pattern. Keep the directory search, duplicate-key boundary traversal, locator checks, record order, payload laziness, matching thresholds and errors unchanged. Never cache a failed decode. The source mmap is immutable under the existing reader contract. Memory is bounded by the decoded index size of each opened table; it is released with that reader. Do not introduce another program-fact map or a runtime dependency. Concurrent cold lookups may decode the same chunk before insertion, but publication returns the retained allocation and no lock is held during decompression or record lookup.

## Validation matrix

- Query repeated hits/misses and duplicate keys crossing chunk boundaries; compare exact record order to an independent fixture oracle.
- Confirm warm queries share validated chunk storage, misses do not materialize payload blocks, failures are not retained, and concurrent queries preserve results.
- Targeted signatures nextest, full signatures/static/decompiler/CLI suites and crate checks; release CLI and workspace formatting.
- Alternating same-machine baseline/candidate timings and fresh 250-address stripped replay, comparing all code, type, address/provenance and non-time fields. Repeated timed requests must be deterministic.
- Retain only measured gains; no Kuna-level claim without actual comparable measurements. Preserve all existing worktrees and dependent draft PRs.

## Follow-up candidate: borrow existing control-flow projections

The immediate samples also show `control_flow_facts_for` in per-function FID and render decode preparation. Both `function_max_bytes` and `decode_memory_context_for` immediately discard most of a deep cloned snapshot. The owner already retains that immutable snapshot behind an Arc/OnceLock cache slot. Reuse the earlier private projection experiment while preserving public owned-return behavior, the cache key, LRU policy, initialization and lock release boundaries. No new cache or analysis policy is involved.

The earlier experiment was excluded after a confounded large-input timing pair (250.183 / 331.526 s). Its output parity was not sufficient to establish performance. Reopening it requires fresh comparisons after the unrelated machine workloads stopped, against immutable binaries and the same source/resource/input policy. Keep the FPK-only binary distinct so attribution remains possible. Tests cover storage identity, owned mutation isolation, decode projections, extents and recursive reads. Reject an unresolved regression again.

## FPK-only result: excluded

Five paired small-batch medians (baseline/candidate seconds): Linux 2.3374/2.3397, Windows 1.2597/1.2751, macOS 1.5031/1.5000. All compared output records match. An additional large-input candidate run took 157.905 s / 1660.640 CPU seconds with unchanged six outputs. The earlier PR #204 large control was 149.095 s / 1590.412 CPU seconds, but this is not a contemporaneous paired estimate. No practical gain is established, so the additional memory/cache is excluded. The implementation and its 14 focused / 393 related passing tests remain in local artifacts as a rejected experiment. Production FPK source is restored exactly to `efe35ee69`.

## Canonical cached projection: bounded sorted ranges

The cached `ControlFlowFacts` is built exclusively by `assemble`: labels, indirect targets, no-return callsites and LSDA landing pads come from BTreeSets; flow edges are sorted and deduplicated before publication. The public record remains mutable by owned callers, so the public general projection must continue to accept unsorted/incomplete records.

Use a private typed cached view, constructible only from the pinned assembled slot, to bound these sorted slices by binary search. Feed both the cached slices and the general public slices into one common projection implementation; preserve all filtering, order, duplicates, limit computation and decoder hints.

`assemble.indirect_targets` contains every successful relocation-target read from `relocation_use_sites(binary)`. The general projection additionally rescans those same sites, restricting both use site and target to the function window, and then sorts/deduplicates the union. Every such additional target is already in the canonical cached target population filtered to that window. Thus the second relocation scan adds nothing for an assembled cache view. It is still required for an arbitrary public partial record. Preserve that public path; only the typed complete cached view can omit it.

This removes per-function whole-image sorting/scanning without changing a target or evaluating a binary differently. Compare the optimized projection against the existing general query over empty/reversed/saturated windows, cross-window edges, duplicate data, full and truncated little/big-endian relocation reads, and ordinary unsorted public records. No unproven completeness flag is accepted from callers.

### Storage identity guard

Inspection found that `LoadedBinary::patch_bytes` and general COW metadata mutation do not change `binary.hash`. Therefore cache-key equality alone cannot prove that a relocation rescan is redundant for the current argument. Retain a weak token of the loader's COW inner allocation with the existing cached facts. Skip the redundant relocation scan only while that token matches the current storage. A patch/metadata mutation detaches a weak token through `Arc::make_mut`; otherwise keep the original general relocation rescan. The token must not keep image bytes alive and is not a content hash, program ID or metadata map. This preserves the existing public cache behavior even after mutable callers change a binary without updating its hash.

A focused fixture also exposed the existing short-read behavior: `view_bytes` can return a truncated eight-byte view, and the old relocation helper abstains before its four-byte fallback. This performance patch preserves that abstention; it does not change relocation semantics.

## Follow-up: register-model preparation

The same native startup samples show multiple render workers simultaneously in `register_model_for_language -> build_register_model_for_language -> parse_registers_from_slaspec`, including repeated macro-substitution loops. The existing language-keyed cache publishes successful immutable models, but releases its read lock before building, allowing duplicate cold work.

Keep that cache owner and its language key. Retain one initialization slot per language with a success OnceLock and a serial construction mutex; release the map lock before parsing, double-check after acquiring the slot mutex, and do not retain failures. Model construction parses files/builds tables synchronously, does not invoke Rayon and does not call this cache recursively. Warm reads clone the same Arc, distinct languages do not share the construction mutex. Preserve uncached public construction and register tables exactly. Check concurrent cold reads with a barrier, retry after failure and warm reads; repeat real output and timing comparisons.

For macro expansion, if input text contains no `$(`, no substitution needle can match, for any definition map. Return the same owned text before constructing one needle per definition. Keep the original lexical map order, replacement/cascade semantics and unknown-token behavior whenever `$(` is present. Compare to the original loop for literal, malformed, Unicode, unknown, repeated and cascaded token fixtures. This is mechanical parser preparation only, with no register/ABI interpretation change.

## Measured candidate results before full replay

The bounded cached CF projection alone leaves small batches essentially unchanged, but a controlled sequential large-input pair improves 148.368 s / 1577.850 CPU s to 105.266 s / 1029.536 CPU s, with all six outputs identical after removing timing fields. This is one pair, not a corpus-wide speed estimate.

With register preparation included, five alternating serial-process pairs yield baseline/candidate medians of 2.3296/2.2239 s (Linux), 1.2530/1.1714 s (Windows), and 1.4916/1.3795 s (macOS). All repeated records are deterministic and all non-time fields match their paired baseline. Full 250-function replay and the final large-input control are separate acceptance gates, not inferred from these 16 functions.

## Acceptance evidence

Final candidate: 2,003 relevant nextest tests passed (5 skipped); four register/macro focused tests and 12 loader/CF focused tests passed. Crate checks, release CLI, workspace formatting and owner boundaries passed. Fresh stripped replay returned 250/250 with zero changed non-time function records. The final large control was 148.593 -> 103.820 s, with identical six outputs. See `docs/validation/decompiler-startup-performance.md` and its compact JSON for raw sample times, hashes, population and limitations. The timed source includes PR #204; all changed implementation preimages match main, allowing an independent main-based patch. Kuna-level speed is not achieved; the new small-batch comparison remains 4.12–11.58 times slower than pinned Kuna 1.808.
