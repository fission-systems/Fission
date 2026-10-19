# Decompiler startup performance — 2026-10-15

Mechanical preparation work. This does not claim semantic/readability quality, execution parity, official ranking, or a corpus-wide speedup. No analysis pass, extra output layer, precision guard, worker policy, timeout or producer schema is changed.

## Changes and invariants

- Borrow the existing immutable control-flow cache for internal projections; preserve the public independently mutable owned snapshot API.
- Bound sorted vectors only through a private view of facts assembled by the canonical owner. Both general/public and bounded/private views use one projection implementation.
- Skip a redundant relocation rescan only when the cached loader storage token still matches. A weak COW identity token does not retain image bytes; patches/metadata mutation detach it and preserve the old current-byte rescan. It is not a content hash or program ID. Cache key/capacity/initialization policies are unchanged.
- Publish one successful register model per language under concurrent cold reads. Construction is synchronous, does not use Rayon, and does not re-enter the cache. Release the map lock before construction, retry failures, and discard empty failed slots.
- Return literal lines directly when no macro substitution needle can match. Preserve ordered/cascaded substitution when tokens are present.

The earlier FPK index cache did not establish a useful gain and is excluded. Pure snapshot borrowing left small-batch times essentially unchanged. Bounded ranges and redundant-relocation removal are necessary parts of the measured large-input result. See [the proposal](../proposals/2026-10-14-decompiler-preparation-costs.md).

## Source and fixed inputs

Measured baseline: PR #204 head `efe35ee6929ec16851f44f283c1ffef3ced70fb9`, CLI SHA256 `63e11b25c2412e510351e03396434ba2f51103300eab52ba83a5b60e684d1db5`. Candidate: exactly that source plus this production patch, CLI SHA256 `4c0485b7da200135afbc2e0bfbdbb9de8a7058549a1f9a1d6af3e91c9c6dc320`.

All modified implementation preimages are identical on main `52c81dd15e9b2c9efa8cd29f4b5153f1a2690f45` and on the measured baseline. The patch can be reviewed/applied independently on main, without merging draft PRs #203/#204. The measured executables include those drafts; native main performance without them is not separately claimed. Remote CI validates the independent main-based source separately.

Frozen [qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489). All 45 stripped-input hashes and address files were verified before replay. Only stripped bytes and requested addresses reach the producer. The [compact evidence](decompiler-startup-performance.json) includes the complete population, binary hashes and raw timing samples.

Apple M4 Pro / 14 CPUs / 24 GiB / native arm64 / rustc 1.97.0 / repository release profile (thin LTO, one codegen unit). Executables, cwd, resources and inputs were pinned. No task-owned builds/tests/profilers overlap timing. OS caches were not flushed and desktop scheduling remains uncontrolled.

## Small batches

Five alternating fresh-process samples per build/batch, requested-function workers fixed to one. Existing FID preparation retains its existing Rayon policy. CPU seconds in the compact evidence are process user + system time across all threads.

| Batch | Functions | Baseline median, s | Candidate median, s | Wall decrease |
|---|---:|---:|---:|---:|
| Linux AArch64 / Snappy O0 (03) | 6 | 2.3296 | 2.2239 | 4.54% |
| Windows x86-64 / Snappy O2 (27) | 5 | 1.2530 | 1.1714 | 6.52% |
| macOS arm64 / Ninja O2 (40) | 5 | 1.4916 | 1.3795 | 7.51% |

All paired outputs match; repeated timed function records are deterministic after removing only duration fields. These 16 functions do not establish a platform-wide or whole-corpus speedup.

## Large control

One sequential candidate-first/baseline-second Tracy O0 pair, six requests, default requested-function worker policy, CLI timeout 30,000 ms and external process cap 600 s. Both outputs match the frozen reference.

| Build | Wall, s | CPU, s |
|---|---:|---:|
| Baseline | 148.593 | 1584.775 |
| Candidate | 103.820 | 1015.641 |

This pair reduces wall time by 30.13% and CPU time by 35.91%. It is a bounded single-pair measurement, not a robust corpus-wide estimate. An earlier CF-only pair was 148.368/105.266 s with unchanged outputs. CF-only small batches were essentially unchanged; register preparation accounts for the separate small-batch improvement.

## Kuna comparison and remaining work

Three alternating fresh-process samples against pinned Kuna 1.808 (not a latest-version claim), same stripped inputs/requested entries, one requested-function worker/job. All exits/counts match expectations; output evidence is deterministic within each tool. Cross-tool semantic equivalence is not claimed.

| Cell | Fission median, s | Kuna median, s | Fission / Kuna |
|---|---:|---:|---:|
| 03 | 2.2259 | 0.1921 | 11.58× |
| 27 | 1.1693 | 0.1128 | 10.37× |
| 40 | 1.3816 | 0.3352 | 4.12× |

Fission remains substantially slower on these requests. Symbolized native startup samples show whole-program no-return decoding and FID preparation before rendering. Sharing proven decode facts across those owners is a next investigation, not implemented here. No FID/no-return pass or output layer is disabled to improve timings.

## Output and local validation

Fresh candidate replay returned **250/250 functions across 45 stripped binaries, with zero changed functions**. The retained baseline outputs were produced from PR #203 and already proved identical to PR #204 for all non-time fields. Compare all code layers, variables/types, direct PCs, scoped line mappings, errors/fallbacks, evidence and non-time counters. Exclude only `build_duration_ms`, `decomp_sec`, `normalize_duration_ms`, `postprocess_sec`, `render_duration_ms`, `structuring_duration_ms`, `total_time_ms`.

Replay uses default requested-function worker policy and two binary processes, matching the reference generation policy. It is an output regression check, not a concurrent speed measurement. Five repeats of the 16 timed functions are deterministic; repeated generation of all 250 is not claimed. No corpus executable ran and no quality/type score was recomputed.

- Nextest: **2,003 passed / 5 skipped** across loader, static, sleigh, pcode, decompiler and CLI. Focused gates: 12 loader/CF tests and four register/macro tests passed.
- Related crate checks, release CLI, workspace fmt, diff check and owner-boundary scan passed. Remote CI is a separate gate.
- Coverage includes shared snapshots/owned mutation isolation, recursive cache reads without holding the map lock, sorted-range equivalence over duplicates and reversed/empty bounds, little/big endian/truncated relocation reads, patched-storage fallback, weak-token detachment/lifetime, concurrent model construction, failure retry, independent slots, and ordered/cascaded macro substitution.

## Reproduction

Local raw artifacts: `benchmark/artifacts/performance/2026-10-14-kuna/` in the primary checkout. This includes immutable binary references, `paired.py`, `replay.py`, `startup_vs_kuna.py`, `startup_large.py`, input hashes, raw stdout/stderr, normalized output digests, profiles, source snapshots and validation logs. Local drivers need path adjustment elsewhere.

For measured-source reproduction, build baseline `efe35ee69`; apply only this PR's `crates/` diff to that source and rebuild for the candidate. Both binaries must use the same external resource tree. Build this PR directly to reproduce the independent main-based CI source. For each compact population entry, put its addresses into a newline-separated file and use its verified stripped input:

```sh
FISSION_RUST_DECOMP_WORKERS=1 /path/to/fission_cli \
  --resource-root /path/to/Fission decomp /path/to/stripped/input \
  --addresses-file /path/to/addresses.txt --json --no-db --layer nir \
  --no-header --no-warnings --timeout-ms 30000
```

Alternate small-run build order. Unset `FISSION_RUST_DECOMP_WORKERS` for the large control and full replay; replay uses two binary processes concurrently, only as a regression check. Regenerate outputs without persisted decompilation-result caches. Exclude only the seven duration keys in the compact evidence and compare all other function JSON fields. No corpus executable is run.
