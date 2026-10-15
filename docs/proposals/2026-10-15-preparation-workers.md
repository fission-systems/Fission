# Bounded preparation workers and immutable runtime metadata lookup

Mechanical performance work; no semantic/readability/score/ranking claim.

## Measured baseline

Main 3daf458fef337a2951e000a17f13fac61d4a2e24, tree identical to 588e5c74821101258652408c3ccd7a0955e19d37. Frozen release SHA256 5ec5b83100b10e1d5280ea22a92f6b4fce8b3894c8d50f1e18038b67e0ac1430. Prior three-pair large median 26.9139 s / 118.2984 CPU s / 2,325,839,872 RSS bytes. Pinned Kuna remains 2.96–10.76x faster. Post-timing sample at 12/20 seconds is still in sequential first no-return binding/capture; the later sample reaches FID/signature lookup. runtime_variant_for_entry and descriptor cloning also occur on the per-instruction path. Evidence: benchmark/artifacts/performance/2026-10-15-transient-decode/startup-large-profile-{12,20,28}.txt. Loader-only probe: 38,186 successful functions / 1,930,323 raw records; not a runtime hit-rate measurement.

## Immutable metadata invariant

The runtime registry is process-immutable. Index candidate variant positions by exact entry ID, retain manifest order among duplicates, and verify the original processor/spec/ID predicate. Borrow the canonical descriptor for status/engine/endianness while preserving the public owned API and exact missing-entry errors. Hoist entry-invariant endianness out of the instruction loop; do not alter decoder success criteria or engine dispatch rules. No program-fact map or new runtime dependency.

## Preparation concurrency invariant

Keep ordinary no-return callers sequential. Only a FID transaction that captures raw seeds may use bounded scoped OS helpers. Do not use the global Rayon iterator, a nested ThreadPool::install, or a Rayon wait: either can let the OnceLock holder steal another caller of the same global CF slot and wait on itself.

Each function decode reads only the immutable binary/frontend and loader-only bounds/context helper; it cannot call CF/no-return or wait on the initializing slot. A local atomic index assigns each original input position once. The caller also drains this queue and joins helpers through std::thread, which does not execute unrelated Rayon work while waiting. Helpers return independent summary/seed results; sort by input position before inserting into the existing maps, preserving duplicate-address last-writer behavior. Instruction limits, function population, decode contracts/contexts, failures, fixed point and FID outputs remain identical. No selective analysis or changed rendering/FID policy.

Use the existing Rayon width as the per-transaction upper bound including the caller. Process-wide nonblocking helper permits bound concurrent extra workers; unavailable permits or OS thread-creation failure reduce parallelism, never drop work or change analysis results. RAII returns permits on normal exit, panic and spawn failure. No global mutex is held during decode, join, callbacks or final fact projection. No persistent pool or raw seeds enter global caches. Panic propagation remains loud. No CLI/environment option or additional dependency is introduced.

## Verification / acceptance

Exact registry lookup differential against the original scan for every manifest entry and invalid/duplicate keys; borrowed identity and owned independence. Scoped queue ordering, skips, duplicate input positions, one-worker fallback, spawn failure/permit recovery, panic recovery and process-wide helper bound. Stress the same OnceLock initialization from a saturated outer Rayon pool, with an explicit deadline; verify no self-wait and exactly one initialization.

Compare serial and scoped preparation on identical real inputs and synthetic seed/flow invariants. Targeted nextest -> complete relevant suites -> checks/release/fmt/owner scan. Freeze independent candidates. Run clean alternating baseline/candidate controls without concurrent own builds/profilers, record wall/CPU/RSS and output determinism. Fresh 250 stripped functions must preserve every non-time function field under the existing seven-duration exclusion. Repeat pinned Kuna controls and separate profiling. Reject unmeasured wins or material output/memory regressions. No recovered/guest execution, rescore or ranking claim from timing evidence.

## Accepted local evidence

17 focused and 1,855 related nextest tests pass (six skipped); checks/release/fmt/owner scan pass. Fresh 250-function output comparison changes zero non-time records. Large three-pair medians: 27.0299 -> 15.6517 seconds (-42.1%), CPU 118.7354 -> 130.4715 (+9.9%), RSS 2,336,391,168 -> 2,399,371,264 bytes (+2.7%). Small medians improve only 2.6–5.4%. Both requested worker lanes are measured for pinned Kuna; independently choosing each tool's faster observed setting still leaves Fission 2.81–6.88x slower. The source includes metadata and helper changes together; individual attribution was not measured. See docs/validation/preparation-workers-performance.{md,json} for hashes, limits and separate diagnostics.
