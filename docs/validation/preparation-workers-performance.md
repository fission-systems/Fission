# Bounded preparation workers and immutable runtime metadata

Mechanical performance measurement, 2026-10-15. Kuna-level speed is **not achieved**; no semantic/readability/score/ranking improvement is claimed.

## Source and invariant

Base main `3daf458fef337a2951e000a17f13fac61d4a2e24`, identical tree to `588e5c74821101258652408c3ccd7a0955e19d37`. Baseline release SHA256 `5ec5b83100b10e1d5280ea22a92f6b4fce8b3894c8d50f1e18038b67e0ac1430`; candidate `c6f8c88a17c804d1388f2b99f8dfb3532b3af731a7576fd1c9445852318b25ae`. [Compact evidence index](preparation-workers-performance.json) records exact production hashes, complete source-patch hash/tree, ranges, controls and limitations.

The canonical immutable registry indexes variant positions by exact entry ID, preserves manifest order for duplicate IDs, and verifies the original processor/spec/ID predicate. Hot status/engine queries borrow the descriptor; the public owned API still clones. Entry-invariant endianness is projected once per lift. Missing-entry errors and engine selection are unchanged.

Only a cold FID transaction capturing raw seeds uses the new no-return queue. Ordinary callers remain sequential. Each function reads immutable binary/frontend data and loader-only bounds/context; no helper calls CF/no-return or waits on the global initializing slot. A local atomic queue assigns input positions once; the caller also processes items. Scoped OS joins cannot steal unrelated outer Rayon jobs, unlike a nested Rayon iterator or ThreadPool::install wait. Results are restored to original input order before existing map insertion, preserving duplicate-address last-success behavior and the exact existing fixed point.

The existing Rayon width bounds each transaction including its caller. A nonblocking process-wide permit counter bounds concurrent extra helpers; unavailable permits or thread-creation failure reduce parallelism while remaining workers drain all work. RAII returns permits on failure/panic; panic propagation remains loud. This does not claim to cap every application thread. Helpers and seeds do not enter global caches. Function population, contexts, instruction budgets, FID/rendering policies, matching rules, completed decode cache and downstream fallback stay intact. No CLI/schema/adapter/dependency change.

## Population and timing method

[Frozen qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions / 45 cells, Linux 108 / Windows 97 / macOS 45. All 45 input SHA256s and sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620` rechecked. Existing complete population catalog: `decompiler-startup-performance.json`. Source/debug oracles are not producer inputs.

Native Apple M4 Pro / 14 logical CPUs / 24 GiB, release `lto=true`, `strip=true`, one codegen unit. Fresh producer processes, `--no-db`, `DECBENCH_NO_CACHE=1`; existing internal caches operate normally. Same resource root/cwd, inputs, addresses and limits. OS caches were not flushed and desktop scheduling was uncontrolled. No own builds/tests/profilers ran during primary timing controls. Diagnostic samples follow all controls.

## Controlled baseline/candidate results

Large Windows batch: six requested functions, three alternating pairs, existing default rendering/FID workers. Medians:

| | Baseline | Candidate |
|---|---:|---:|
| Wall seconds | 27.0299 | 15.6517 |
| CPU seconds | 118.7354 | 130.4715 |
| Maximum RSS bytes | 2,336,391,168 | 2,399,371,264 |

Wall decreases **42.1%**; CPU increases **9.9%** and median RSS **2.7%**. This is lower latency at increased CPU cost, not a per-core efficiency or memory improvement. RSS ranges: baseline 2,309,685,248–2,350,465,024; candidate 2,367,291,392–2,411,544,576 bytes. The combined metadata/worker patch was measured; separate attribution was not measured.

Small controls: five alternating pairs, one requested render worker; FID and preparation may still use the existing Rayon width.

| Cell/platform | Baseline seconds | Candidate seconds |
|---|---:|---:|
| 03/Linux | 2.0596 | 1.9774 |
| 27/Windows | 0.9180 | 0.8683 |
| 40/macOS | 1.0144 | 0.9877 |

Differences are about 2.6–5.4%; do not extrapolate the large-batch gain to short tasks. Complete non-time records match across every controlled pair/repeat.

## Kuna 1.808 controls

Same 22 addresses in four batches, three alternating fresh-process runs/tool/batch in each requested-worker lane. Both the historical one-requested-worker lane and a lane with requested jobs/render workers=14 plus `RAYON_NUM_THREADS=14` are retained. One requested Fission render worker does **not** serialize FID/preparation. Kuna's 14-job path was slower here, so it must not be selected to make Fission appear closer.

| Cell | Fission, requested 1 | Kuna, requested 1 | Fission, requested 14 | Kuna, requested 14 |
|---|---:|---:|---:|---:|
| 03 | 2.0126 | 0.1986 | 1.3663 | 0.3275 |
| 22 | 16.7702 | 2.7495 | 15.3214 | 5.5833 |
| 27 | 0.8832 | 0.1152 | 0.6930 | 0.2487 |
| 40 | 0.9912 | 0.3340 | 0.9392 | 0.6996 |

Taking the faster observed median independently for each tool in each batch, Fission remains **6.88x / 5.57x / 6.01x / 2.81x** slower. This is a comparison of two observed settings, not exhaustive tuning, equal output quality, per-core efficiency, latest-release claims or a whole-corpus timing ratio. Both tools return exact address sets, successful exits and nonempty/error-free Kuna output. Kuna code/variable/map digests repeat; complete Fission non-time records match both lanes/all repetitions and the fresh replay. An exploratory 14-worker pilot is retained separately; the table uses `rows-vs-kuna-14-final`.

## Validation and diagnostics

- **17 focused nextest tests pass**; relevant sleigh/static/pcode/decompiler/CLI suites: **1,855 passed / 6 skipped**. Crate checks, release CLI, fmt and owner-boundary scan pass. Existing unrelated example warnings remain.
- Registry differential covers every manifest entry, invalid processor/spec/ID, duplicate ordering, borrowed identity and owned independence.
- Queue tests cover every input once, stable order/skips, caller-only fallback, unavailable permits, OS spawn failure, panic/permit recovery and concurrent callers' helper bound. A saturated outer Rayon pool repeatedly shares one OnceLock initializer (24 rounds × 24 callers, explicit deadline); it completes with one initializer each round.
- Serial/scoped no-return facts match on return/call/loop fixtures; all 16 captured seeds rebuild complete native-equivalent functions. Existing raw tuple/context/delay/partial-capture tests continue passing.
- Fresh **250/250 outputs**, **zero changed non-time function records** against immutable prior main outputs: code/layers, variables/types/PCs/maps, errors/fallbacks/evidence/counters all compared. Only the seven existing duration keys in the JSON are excluded. Whole population was generated once; focused 22-function complete records also repeat across both worker lanes (three runs each), and small controls across five pairs.
- Symbolized native `sample` diagnostics use the identical production source, release LTO/one codegen unit with debug symbols. Their six-function outputs match release. The large later snapshot exposes FPK decompression/lookup and repeated candidate parsing; small samples include preparation and NIR building. A dyld-only initial sample is excluded as unusable for owner attribution. Samples are not timing/RSS controls or whole-run percentages.
- No guest/recovered program execution/recompilation, metric rescore, external quality benchmark or ranking publication. This is latency and producer-output preservation evidence.

## Remaining limits / artifacts

Cold seed-producing FID preparation benefits; a prior ordinary CF/no-return initializer remains sequential and may provide no seed. Completed-function lookups still promote cold IR to strongly retained hot IR, so all-function cache growth is unchanged. The first decode/binding, FID lookup and NIR costs remain; no speed benefit is claimed for proposed follow-ups.

Artifacts: `benchmark/artifacts/performance/2026-10-15-preparation-workers/`, including immutable candidate, source patch/hashes, input digests, paired/large/replay controls, both Kuna lanes, all commands/raw JSON/stderr, test/build logs and separate symbolized samples. Baseline: `../2026-10-15-transient-decode/candidate/fission_cli`; replay reference: `../2026-10-15-transient-decode/replay/`.

Retained drivers reproduce the lane: `large.py`; `paired.py --baseline ../2026-10-15-transient-decode/candidate/fission_cli --candidate candidate/fission_cli --label paired --iterations 5`; `replay.py` with the same binary arguments and `--label replay`; `vs_kuna.py`; `vs_kuna_14_final.py`. Fixed resources/cwd: `/Users/sjkim1127/.codex/worktrees/call-width-verified/Fission`. Remote CI remains a separate post-submission gate.
