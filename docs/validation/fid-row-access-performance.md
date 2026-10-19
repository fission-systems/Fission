# FID payload row access — 2026-10-15

Mechanical follow-up to [PR #205](https://github.com/fission-systems/Fission/pull/205). This is a measured producer-performance change, not a semantic/readability, execution-parity or ranking claim. FID admission, hashes, record order, errors, analysis/worker policies and output schemas are unchanged.

## Owner and representation

`FpkReader::records_by_key` already knows the payload block and row from the hash locator. Previously, `text.lines().nth(row)` scanned the same immutable prefix for every lookup. The existing payload-cache entry now retains an optional line-range index, initialized synchronously once from its immutable text. Row access uses a checked string slice.

Public `block_at`/`block_for` still return the shared `Arc<String>`; ordinary block reads do not initialize line ranges. LF, CRLF, bare CR, empty lines, Unicode and final unterminated rows match `str::lines`. Directory searches, decompression/validation, locator bounds, duplicate handling and record ordering retain their original behavior. Failed decompression is not cached. No new dependency, program-fact map or global cache was added.

The added retained storage is two `usize` offsets per indexed line, plus vector spare capacity and the small lazy-cache header. It lives with the existing decoded payload and is not a second text copy. The earlier rejected compressed-index cache remains excluded. See [the proposal](../proposals/2026-10-15-fid-row-access.md).

## Fixed source and population

Baseline CLI SHA256: `4c0485b7da200135afbc2e0bfbdbb9de8a7058549a1f9a1d6af3e91c9c6dc320` (the PR #205 measured candidate). Candidate: `28ec116dcb4966fa53546dcbf2e76977efcfb7a4b6bec380ba14174b2df14aba`.

As in [the preceding report](decompiler-startup-performance.md), native measurements retain the PR #203/#204 context: PR #204 head `efe35ee69`, then PR #205 production changes, then this FPK patch. The FPK preimage is identical on independent main `b33e4e125`; this PR therefore needs no draft prerequisite. Native performance on main without those drafts is not inferred from these measurements. CI validates that independent source separately.

Same Apple M4 Pro / 14 CPUs / 24 GiB / rustc 1.97.0 / native arm64 release profile, fixed resources/cwd, stripped inputs, addresses and CLI options. No task-owned builds, tests or profiler overlap timing. OS caches were not flushed and desktop scheduling remains uncontrolled.

Frozen [qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489). All 45 input hashes were rechecked before replay. The complete population is in [the preceding compact evidence](decompiler-startup-performance.json); [this compact evidence](fid-row-access-performance.json) records raw times, hashes and output digests.

## Five-pair small controls

Alternating fresh processes, one requested-function worker, existing FID Rayon policy. All paired non-time function records match; repeated timed records are deterministic.

| Batch | Baseline median, s | Candidate median, s | Interpretation |
|---|---:|---:|---|
| Linux AArch64 / Snappy O0, 6 functions | 2.2162 | 2.2104 | Essentially unchanged |
| Windows x86-64 / Snappy O2, 5 functions | 1.1763 | 0.9614 | 18.27% lower wall time |
| macOS arm64 / Ninja O2, 5 functions | 1.3783 | 1.3715 | Essentially unchanged |

A separate direct five-pair comparison against the original PR #204 binary (`63e11b25...`) measures both patches together: Linux 2.3214/2.2082 s, Windows 1.2544/0.9609 s, macOS 1.4802/1.3693 s. These are selected-batch results, not a corpus-wide estimate.

## Large control and memory

One sequential candidate-first/baseline-second Tracy O0 pair, six requests, default requested-function worker policy. Both complete outputs match the frozen reference. `/usr/bin/time -l` wraps both commands to record process maximum RSS. CPU is child user + system time across threads. CLI timeout is 30,000 ms per function; external process cap is 600 seconds.

| Build | Wall, s | CPU, s | Maximum RSS, GiB |
|---|---:|---:|---:|
| Baseline | 112.924 | 1044.333 | 3.624 |
| Candidate | 42.235 | 233.619 | 3.598 |

This pair reduces wall time by 62.60% and CPU by 77.63%. Peak RSS is similar in this one control; it does not establish reduced memory usage. The lazy row index adds retained metadata as described above. This is not a statistically robust or whole-corpus speed claim. The earlier unwrapped baseline control was 103.820 s; use the fresh wrapped pair for the incremental percentage.

## Pinned Kuna comparison

Three alternating fresh-process runs per tool, one requested-function worker/job, exact requested entry sets and successful returned counts checked. Output evidence is deterministic within each tool; cross-tool semantic equivalence is not claimed. The six large Fission records also match across all three runs after excluding only duration fields.

| Batch | Fission median, s | Kuna 1.808 median, s | Fission / Kuna |
|---|---:|---:|---:|
| Linux Snappy O0 | 2.220 | 0.189 | 11.73× |
| Windows Tracy O0 | 44.355 | 2.868 | 15.47× |
| Windows Snappy O2 | 0.957 | 0.111 | 8.62× |
| macOS Ninja O2 | 1.376 | 0.330 | 4.17× |

Fission has not reached Kuna-level speed. The remaining whole-program decode/preparation cost needs further investigation; this patch does not bypass it.

## Validation and reproduction

Fresh stripped replay returned **250/250 functions across 45 binaries with zero changed non-time function records** against the frozen reference already verified for the preceding candidate. All code layers, variables/types, direct PCs, scoped maps, errors/fallbacks, evidence and non-time counters were compared. Only the seven duration keys in the compact evidence were excluded. Replay uses default requested-function workers and two binary processes as a regression check; no concurrent speed claim follows. Timed subset repetitions are deterministic, but repeated generation of all 250 is not claimed.

- Focused FPK: **15 passed**; related signatures/static/decompiler/CLI: **398 passed / 1 skipped** on the measured source.
- Crate checks, release CLI and workspace formatting passed. Coverage includes line-iterator equivalence, lazy public ownership, repeated/concurrent ordered hash lookups, invalid row errors and existing cross-block/chunk duplicate cases.
- Remote independent-main CI is a separate gate.

Raw artifacts remain in `benchmark/artifacts/performance/2026-10-14-kuna/`: `rows-only`, `cumulative-final`, `rows-large`, `rows-vs-kuna`, `rows-replay`, frozen binaries, profiles and build/test logs. Reuse the exact producer invocation and duration exclusions in the preceding report. For measured-source reproduction, apply this PR's `crates/` diff on the PR #205 measured context; for independent-main CI reproduction, build this PR directly. No corpus executable runs and no quality metric or denominator changes.
