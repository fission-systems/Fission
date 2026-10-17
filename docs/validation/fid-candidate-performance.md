# Immutable FID candidate reuse

Mechanical performance measurement, 2026-10-15. **Kuna-level speed is not achieved.** No semantic/readability/score/ranking improvement is claimed.

## Source and contract

Baseline source `0f0e8f75a536fcb6ec9b349dec372d44d7c65d0c` (PR #246); baseline release SHA256 `5fde8204f59b90a4fba7ca2af4c32c08c41efcb066df8586a9085cf7022f65d7`; candidate `26dc790eed98282e603e8ee6b3ad91ba40590c9c61b25c51a25d72f38bdb8d0c`. [Compact evidence](fid-candidate-performance.json) records exact source files, full source-patch hash/tree, ranges and both worker lanes. No runtime dependency, CLI/schema/adapter, archive format, function population, instruction budget, matching policy or worker setting is changed.

The immutable `LazyFidDatabase` resource owner reuses positive parsed full-hash candidate records. It preserves record order/duplicates, malformed-row filtering and every field read by the existing match decoder. The cache is private to one database, independent of program facts. Specific hashes, auto/force flags, relation contexts, confidence thresholds, library lookup, stable sorting and final matches are recomputed on each call. Returned names remain owned copies.

Archive reads and parsing happen outside the map mutex. Only positive parsed results are inserted; absent/malformed-empty/error results retry. Positive keys are bounded by the archive's finite keys, but retained records are not a fixed memory-cap cache. Concurrent cold queries can duplicate loading; the first complete inserted Arc wins. Losing allocations drop outside the lock. Poisoning falls back to uncached loading. This does not cache final decisions or add per-key singleflight.

The motivating native sample is the prior preparation prototype A1 at `benchmark/artifacts/performance/2026-10-15-preparation-workers/startup-large-profile-12.txt`, before A2's CAS portability fix. Its FPK owner is unchanged from A2. It exposes force-candidate reads/decompression/row parsing followed by later candidate lookup. These bounded samples are diagnostic evidence, not whole-run percentages or the final candidate's timing data.

## Population / controls

[Frozen qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions in 45 cells (Linux 108 / Windows 97 / macOS 45). All 45 stripped input hashes and sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620` were verified. Existing population catalog: `decompiler-startup-performance.json`. Source/debug oracles are not producer inputs.

Native M4 Pro / 14 logical CPUs / 24 GiB; release LTO=true, strip=true, codegen-units=1. Fresh processes, `--no-db`, `DECBENCH_NO_CACHE=1`, identical resources/cwd/addresses/limits. Internal caches remain active. No own builds/tests/profilers overlap primary timing controls; OS caches were not flushed and desktop scheduling is uncontrolled.

## Controlled before / after

Six requested functions in the large Windows input, three alternating pairs, existing default render/FID workers. Medians:

| | Baseline A2 | Candidate |
|---|---:|---:|
| Wall seconds | 14.6610 | 13.8580 |
| CPU seconds | 133.6656 | 121.6471 |
| Maximum RSS bytes | 2,387,591,168 | 2,452,586,496 |

Wall **-5.5%**, CPU **-9.0%**, RSS **+2.7%**. RSS ranges: baseline [2373091328, 2404827136]; candidate [2441265152, 2453405696] bytes. This is a scoped time/CPU reduction at increased memory cost. Do not compound percentages from separate earlier control runs.

Small controls: five alternating pairs, one requested render worker (FID/preparation still use the existing Rayon width).

| Cell | Baseline seconds | Candidate seconds |
|---|---:|---:|
| 03 | 2.0387 | 2.0349 |
| 27 | 0.8944 | 0.8823 |
| 40 | 1.0088 | 0.9994 |

Small differences are about 0.2–1.3%; no broad short-task gain is claimed. Complete non-time records match all pairs/repetitions.

## Pinned Kuna 1.808

Same 22 addresses in four batches, three alternating runs/tool/batch per lane. Requested jobs/render workers=1 or 14; the 14 lane also fixes `RAYON_NUM_THREADS=14`. One requested Fission render worker does not serialize FID/preparation. Kuna is slower at 14 here; each tool's faster observed median is selected separately.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 2.0496 | 0.2296 | 1.3918 | 0.3426 | 6.06x |
| 22 | 15.6591 | 2.8571 | 14.6156 | 5.6366 | 5.12x |
| 27 | 0.8724 | 0.1148 | 0.6999 | 0.2495 | 6.10x |
| 40 | 1.0035 | 0.3555 | 0.9714 | 0.7154 | 2.73x |

Fission remains **2.73–6.10x slower**. This compares two observed settings, not exhaustive tuning, equal output quality, a latest-version or whole-corpus ratio. Both tools return exact address sets with successful exits; Kuna output is nonempty/error-free and its code/variable/map digest repeats. Complete Fission non-time records match every repetition in both lanes and fresh replay.

## Validation / bounds

- Five focused differential/cache tests pass: warm reuse/full fields/order/Unicode/duplicate keys, resource isolation, missing/malformed/error retries, concurrent insertion, poisoning and returned ownership. The matrix compares an independent pre-cache implementation across all 16 flags, eight threshold boundaries, changing specific hashes and parent/child contexts, duplicate ties and missing libraries.
- **1,952 related nextest tests pass, six skipped** (signatures/sleigh/static/pcode/decompiler/CLI). Crate checks, scoped CI Clippy, release CLI, workspace fmt and owner-boundary scan pass. Existing example warnings are unchanged.
- Fresh **250/250** outputs: **zero changed non-time function records** against immutable A2 outputs. All function fields are compared recursively, including code/layers, variables/types/PCs/maps, errors/fallbacks/evidence/counters; exactly the seven existing duration keys are excluded. Whole population was generated once; focused 22 functions also repeat three times per worker lane and small controls five pairs.
- No recovered/guest execution, recompilation, metric rescore, external quality benchmark or ranking publication. This is performance and producer-output preservation evidence.
- Initial frontend preparation, FID misses and completed-function hot-cache growth remain. More speed work is required; the cache does not establish Kuna parity.

Artifacts: `benchmark/artifacts/performance/2026-10-15-fid-candidates/`, including immutable binaries/source snapshots/patch/hashes, all commands/JSON/stderr, complete comparisons, test/build logs and control drivers. Baseline executable: `../2026-10-15-preparation-workers/candidate-ci/fission_cli`; replay reference: `../2026-10-15-preparation-workers/replay-ci/`.

Reproduction: `large.py`; `paired.py --baseline ../2026-10-15-preparation-workers/candidate-ci/fission_cli --candidate candidate/fission_cli --label paired --iterations 5`; `replay.py` with the same binaries and `--label replay`; `vs_kuna.py`; `vs_kuna_14.py`. Fixed resources/cwd: `/Users/sjkim1127/.codex/worktrees/call-width-verified/Fission`. Remote CI on the submitted head remains a separate gate.
