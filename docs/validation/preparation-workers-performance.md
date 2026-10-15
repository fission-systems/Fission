# Bounded preparation workers and immutable runtime metadata

Mechanical performance measurement, 2026-10-15. Kuna-level speed is **not achieved**; no semantic/readability/score/ranking improvement is claimed.

## Source and invariant

Base main `3daf458fef337a2951e000a17f13fac61d4a2e24`, identical tree to `588e5c74821101258652408c3ccd7a0955e19d37`. Baseline release SHA256 `5ec5b83100b10e1d5280ea22a92f6b4fce8b3894c8d50f1e18038b67e0ac1430`; candidate `5fde8204f59b90a4fba7ca2af4c32c08c41efcb066df8586a9085cf7022f65d7`. [Compact evidence index](preparation-workers-performance.json) records exact production hashes, complete source-patch hash/tree, ranges, controls and limitations.

The canonical immutable registry indexes variant positions by exact entry ID, preserves manifest order for duplicate IDs, and verifies the original processor/spec/ID predicate. Hot status/engine queries borrow the descriptor; the public owned API still clones. Entry-invariant endianness is projected once per lift. Missing-entry errors and engine selection are unchanged.

Only a cold FID transaction capturing raw seeds uses the new no-return queue. Ordinary callers remain sequential. Each function reads immutable binary/frontend data and loader-only bounds/context; no helper calls CF/no-return or waits on the global initializing slot. A local atomic queue assigns input positions once; the caller also processes items. Scoped OS joins cannot steal unrelated outer Rayon jobs, unlike a nested Rayon iterator or ThreadPool::install wait. Results are restored to original input order before existing map insertion, preserving duplicate-address last-success behavior and the exact existing fixed point.

The existing Rayon width bounds each transaction including its caller. A nonblocking process-wide permit counter bounds concurrent extra helpers; unavailable permits or thread-creation failure reduce parallelism while remaining workers drain all work. RAII returns permits on failure/panic; panic propagation remains loud. This does not claim to cap every application thread. Helpers and seeds do not enter global caches. Function population, contexts, instruction budgets, FID/rendering policies, matching rules, completed decode cache and downstream fallback stay intact. No CLI/schema/adapter/dependency change.

## Population and timing method

[Frozen qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions / 45 cells, Linux 108 / Windows 97 / macOS 45. All 45 input SHA256s and sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620` rechecked. Existing complete population catalog: `decompiler-startup-performance.json`. Source/debug oracles are not producer inputs.

Native Apple M4 Pro / 14 logical CPUs / 24 GiB, release `lto=true`, `strip=true`, one codegen unit. Fresh producer processes, `--no-db`, `DECBENCH_NO_CACHE=1`; existing internal caches operate normally. Same resource root/cwd, inputs, addresses and limits. OS caches were not flushed and desktop scheduling was uncontrolled. No own builds/tests/profilers ran during primary timing controls. Diagnostic samples follow all controls.

## Controlled baseline/candidate results

Large Windows batch: six requested functions, three alternating pairs, existing default rendering/FID workers. Final A2 medians:

| | Baseline | Candidate |
|---|---:|---:|
| Wall seconds | 27.2216 | 15.4257 |
| CPU seconds | 122.0768 | 136.2194 |
| Maximum RSS bytes | 2,329,264,128 | 2,416,246,784 |

Wall decreases **43.3%**; CPU increases **11.6%** and median RSS **3.7%**. This is lower latency at increased CPU cost, not a per-core efficiency or memory improvement. RSS ranges: baseline [2300313600, 2344697856]; candidate [2407890944, 2474508288] bytes. The combined metadata/worker patch was measured; separate attribution was not measured.

Small controls: five alternating pairs, one requested render worker; FID/preparation still use the existing Rayon width.

| Cell | Baseline seconds | Candidate seconds |
|---|---:|---:|
| 03 | 2.0569 | 1.9712 |
| 27 | 0.9238 | 0.8618 |
| 40 | 1.0000 | 0.9837 |

Do not extrapolate the large-batch gain to short tasks. Complete non-time records match every controlled pair/repeat.

## Kuna 1.808 controls

Same 22 addresses in four batches; three alternating fresh-process runs/tool/batch in each lane. Requested jobs/render workers are 1 or 14 (the latter also sets `RAYON_NUM_THREADS=14`). One requested Fission render worker does not serialize FID/preparation. Kuna was slower at 14 jobs; compare each tool's faster observed setting.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.9073 | 0.1861 | 1.3464 | 0.3265 | 7.24x |
| 22 | 16.3628 | 2.7467 | 14.8358 | 5.4616 | 5.40x |
| 27 | 0.8436 | 0.1098 | 0.6708 | 0.2461 | 6.11x |
| 40 | 0.9499 | 0.3220 | 0.9117 | 0.6795 | 2.83x |

Fission remains slower. This compares two observed settings, not exhaustive tuning, equal quality, per-core efficiency, latest releases or whole-corpus timing. All exact requested address sets return successfully. Kuna output is nonempty/error-free and its code/variable/map digests repeat; complete Fission non-time records match both lanes, every repetition and fresh replay. Final primary lanes: `rows-vs-kuna-ci` and `rows-vs-kuna-14-ci`; older prototype/pilot controls remain distinct.

## Validation and diagnostics

- **17 focused nextest tests pass**; relevant sleigh/static/pcode/decompiler/CLI suites: **1,855 passed / 6 skipped**. Crate checks, release CLI, fmt and owner-boundary scan pass. Existing unrelated example warnings remain.
- Registry differential covers every manifest entry, invalid processor/spec/ID, duplicate ordering, borrowed identity and owned independence.
- Queue tests cover every input once, stable order/skips, caller-only fallback, unavailable permits, OS spawn failure, panic/permit recovery and concurrent callers' helper bound. A saturated outer Rayon pool repeatedly shares one OnceLock initializer (24 rounds × 24 callers, explicit deadline); it completes with one initializer each round.
- Serial/scoped no-return facts match on return/call/loop fixtures; all 16 captured seeds rebuild complete native-equivalent functions. Existing raw tuple/context/delay/partial-capture tests continue passing.
- Fresh **250/250 outputs**, **zero changed non-time function records** against immutable prior main outputs: code/layers, variables/types/PCs/maps, errors/fallbacks/evidence/counters all compared. Only the seven existing duration keys in the JSON are excluded. Whole population was generated once; focused 22-function complete records also repeat across both worker lanes (three runs each), and small controls across five pairs.
- Symbolized native `sample` diagnostics use prototype A1 source before the CAS portability fix (not identical to final A2 source); the FPK owner is unchanged. They retain release LTO/one codegen unit with debug symbols. Their six-function outputs match the prototype release. The large later snapshot exposes FPK decompression/lookup and repeated candidate parsing; small samples include preparation and NIR building. A dyld-only initial sample is excluded as unusable for owner attribution. Samples are not timing/RSS controls or whole-run percentages.
- No guest/recovered program execution/recompilation, metric rescore, external quality benchmark or ranking publication. This is latency and producer-output preservation evidence.

## Remaining limits / artifacts

Cold seed-producing FID preparation benefits; a prior ordinary CF/no-return initializer remains sequential and may provide no seed. Completed-function lookups still promote cold IR to strongly retained hot IR, so all-function cache growth is unchanged. The first decode/binding, FID lookup and NIR costs remain; no speed benefit is claimed for proposed follow-ups.

Artifacts: `benchmark/artifacts/performance/2026-10-15-preparation-workers/`, including immutable candidate, source patch/hashes, input digests, paired/large/replay controls, both Kuna lanes, all commands/raw JSON/stderr, test/build logs and separate symbolized samples. Baseline: `../2026-10-15-transient-decode/candidate/fission_cli`; replay reference: `../2026-10-15-transient-decode/replay/`.

Final drivers: `large_ci.py`; `paired.py --baseline ../2026-10-15-transient-decode/candidate/fission_cli --candidate candidate-ci/fission_cli --label paired-ci --iterations 5`; `replay.py` with the same binaries and `--label replay-ci`; `vs_kuna_ci.py`; `vs_kuna_14_ci.py`. Fixed resources/cwd: `/Users/sjkim1127/.codex/worktrees/call-width-verified/Fission`.

Initial PR CI passed tests/PE smoke but failed lint because new CI Rust deprecates `fetch_update`. The explicit `compare_exchange_weak` loop retains memory order and the permit limit on both toolchains. Final A2 targeted/full tests, checks/release, scoped CI Clippy and fmt pass; all final producer controls were regenerated after that source change. Remote CI on the new head remains a separate gate.
