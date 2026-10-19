# Reuse immutable FID candidate records

Mechanical follow-up to bounded preparation (source 0f0e8f75a536fcb6ec9b349dec372d44d7c65d0c / PR #246, still pending integration at proposal time). Baseline immutable release SHA256 5fde8204f59b90a4fba7ca2af4c32c08c41efcb066df8586a9085cf7022f65d7. Large three-pair median 15.4257 s / 136.2194 CPU s / 2,416,246,784 RSS bytes. No semantic/readability/ranking claim.

## Fresh owner evidence

The prototype A1 native sample (before the CAS portability fix; FPK owner code is unchanged) at benchmark/artifacts/performance/2026-10-15-preparation-workers/startup-large-profile-12.txt shows main-thread LazyFidDatabase::has_force_relation_candidate repeatedly entering FpkReader::records_by_key, payload/index decompression and row decoding. Other workers subsequently query the same full-hash candidates for matching. This is fresh evidence after no-return preparation was parallelized, not reuse of an old rejected index-chunk-cache experiment. Do not infer a whole-run percentage or speedup from sample counts.

## Invariant

A LazyFidDatabase owns one immutable function archive. The raw candidates for a full hash depend only on that archive/hash. Specific-hash filtering, auto flags, confidence thresholds, relation scores, library lookup and final ordering still run for every query. Cache parsed candidate records, never final matches or force-relation eligibility. Preserve original returned row order, malformed-row filtering and all matching fields.

Keep the cache at the signature-resource owner, private to the database instance. Positive candidate entries only: absent/malformed-empty results and read/decode errors remain uncached and retry on each call. Thus arbitrary missing hashes cannot grow a negative cache; retained positive keys are bounded by the archive's finite hash keys. No cache format, new dependency, capacity knob, CLI option or altered matching policy.

Release the map mutex before archive access and parsing. First successful insertion wins; immutable competing results are equal and callers receive the chosen Arc. Drop losing parsed records outside the map lock. Poisoning falls back to uncached loading. Warm records stay immutable; returned match names remain owned copies. No FPK payload-cache, relation-index, program-fact or worker-policy change.

## Verification / acceptance

Candidate field/order round-trip and Unicode/escaped names; warm positive reuse, misses/malformed rows/errors retry without insertion, distinct resource ownership, concurrent insertion, poisoning fallback and owned match independence. Independent uncached lazy reference comparison across specific hashes, all auto/force flags, threshold boundaries, duplicate/tied names, missing libraries and changing child/parent contexts. Existing eager/lazy relation tests remain green.

Targeted nextest -> relevant full suites/checks/release/fmt/owner scan. New immutable candidate compared against the frozen #246 source executable; alternate clean timing/CPU/RSS controls and fresh 250-function complete non-time parity (same seven durations only), plus focused determinism. Use the same worker lanes when comparing Kuna; report the faster observed setting per tool rather than selecting a slower competitor configuration. Discard the experiment if no useful measured gain or output preservation is obtained. No guest execution, rescore or ranking claim.

## Accepted local evidence

Final immutable candidate: large medians 14.6610 -> 13.8580 s (-5.5%), CPU -9.0%, RSS +2.7%. Small changes are only 0.2–1.3%. Five focused and 1,952 related tests pass (six skipped), plus checks/release/Clippy/fmt/owner scan. Fresh 250-function complete non-time comparison changes zero records; both focused worker lanes repeat. Kuna parity is not achieved. See docs/validation/fid-candidate-performance.{md,json} for source/input hashes, ranges, worker choices and limits.
