# Completed-decode cache: compact cold storage

## Scope and invariant

Mechanical cache-storage optimization. Keep every completed FID result available with the same first-insertion and complete-only rules. Eviction or weaker retention can change the smaller-budget decode and switch-fallback paths, so neither is used.

An opaque in-process codec in `fission-sleigh` uses its existing bincode/serde dependencies to pack every canonical `DecodedPcodeFunction` field with varint encoding. There is no external byte constructor, disk format, new dependency or CLI/JSON output change. The per-session address map stays in `fission-static`.

Cold entries retain packed bytes and a weak reference to the original graph. The first lookup either returns that exact live Arc or restores its exact value, promotes the entry to a shared strong Arc, and drops the packed representation. Subsequent lookups share the original hot-cache behavior. Encoding runs outside the map mutex; restoration runs under a per-entry mutex after releasing the map. There is no Rayon, inference, I/O or recursive cache lookup inside the codec. Encoding failure retains the previous strong-Arc representation. Invalid private archive data is an internal invariant failure, not a silent cache miss or a fabricated decode.

**This does not add no-return/FID cross-analysis decode sharing.** Those analyses still use different byte-window/memory-context contracts. It makes the existing FID/consumer shared cache cheaper to retain. Hot entries are not evicted: querying every function can return to the original expanded-IR memory cost. Both representations are not permanently retained for hot entries. No uniform memory cap or all-function memory reduction is claimed.

[Proposal](../proposals/2026-10-15-completed-decode-storage.md), [compact raw measurements](completed-decode-storage-performance.json).

## Source and population

Base main `064a85747d2d1c6c0726464dd77456cb66a7e9a1` has the same full tree as source `400dfbe022a884a19e58abadd2b0ba21aef29a41` from PR #207. Its immutable release executable is the control. This candidate contains only the measured three-source-file patch plus proposal/validation documents; draft #203/#204 and DecBench #103 remain separate.

- Baseline executable SHA256: `b2731b579df71ecd640d3026b388c25be15ccf0ddf6b56c4724817170b927749`.
- Candidate executable SHA256: `a4e3eafa56886bfc9dc7948364b291bc683dd69e2327c0e587eda1c6467f5e6d`.
- [Qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [frozen qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).
- The 45-cell / 250-address population and stripped-input hashes are already checked in under `population` in [decompiler-startup-performance.json](decompiler-startup-performance.json). Source/debug inputs are not passed to producers.

## Measurements

Apple M4 Pro, 14 logical CPUs, 24 GiB RAM; native arm64 binaries with the repository release profile (`lto=true`, one codegen unit). Fresh producer processes use `--no-db` and `DECBENCH_NO_CACHE=1`; process-local preparation caches retain their normal operation. Same resource root/cwd, input and function sets, time limits and worker policies. OS caches are not flushed, desktop scheduling is uncontrolled. No own build, test or profiler ran during timing comparisons.

Three alternating pairs on the large Windows x64 Tracy O0 batch (cell 22, six requested functions), default workers, one producer at a time, `/usr/bin/time -l`:

| Median | Baseline | Candidate | Reduction |
| --- | ---: | ---: | ---: |
| Whole-batch wall seconds | 32.4801 | 29.9963 | 7.6% |
| Child CPU seconds | 150.3190 | 143.7246 | 4.4% |
| Maximum RSS bytes | 3,724,197,888 | 2,409,381,888 | 35.3% |

RSS ranges are 3.462–3.739 GB for the control and 2.407–2.435 GB for the candidate. These are contemporary process peaks, not heap-byte estimates or a comparison against the older 3.9 GB run. No whole-corpus speed claim.

Five alternating pairs per small batch, `FISSION_RUST_DECOMP_WORKERS=1` (function rendering only; existing Rayon FID policy remains):

| Cell / batch | Baseline median seconds | Candidate median seconds |
| --- | ---: | ---: |
| 03 / Linux AArch64 Snappy O0 / 6 functions | 2.1361 | 2.1315 |
| 27 / Windows x64 Snappy O2 / 5 functions | 0.9540 | 0.9567 |
| 40 / macOS arm64 Ninja O2 / 5 functions | 1.0471 | 1.0477 |

Observed median differences are within 0.3%; no small-batch speed gain is claimed. Windows/macOS medians increase slightly (about 0.29% / 0.06%). Small extra codec/mutex work exists and cannot be separated from scheduling noise by these five observations.

## Output and regression checks

- Fresh generation of all **250 stripped requests across 45 cells** returns 250 function records, with **zero changed non-time records** against the previous clean-main replay. All 45 stripped hashes match the frozen population.
- Compare every emitted function field recursively, excluding exactly `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. Code layers, variables/types, addresses, errors/fallbacks, evidence and counters remain identical. Root build metadata is outside this comparison.
- Full non-time records repeat deterministically in five-pair small tests, three-pair large tests and the separate three-run 22-function Fission comparison lane. The full population is replayed once; two-run full-population determinism is not claimed.
- **Seven focused tests pass:** complete field/enum/stop-reason round-trip (including high bits, signed constants, absent/empty text, Unicode and context/edge metadata), private malformed/truncated payload rejection, cold/hot lifetime, live Arc identity, first insertion, copy-on-write snapshot retention, encoding fallback, incomplete exclusion and concurrent single materialization.
- **1,838 related nextest tests pass, five skipped** across sleigh/static/pcode/decompiler/CLI. Crate checks, release CLI, workspace fmt/diff and owner-boundary checks pass. Remote PR CI is a separate submission gate and is not claimed green at evidence recording time.

No guest binary or recovered source is executed. This is measured resource reduction with bounded output preservation, not new semantic correctness, a score or ranking result. The storage change intentionally preserves the previous results.

## Pinned Kuna comparison

Three alternating fresh-process runs per tool/batch, one requested worker/job, same stripped requests. Kuna **1.808** is pinned; this is not a latest-release claim. All requested addresses return; Kuna code is nonempty with no reported errors; code/variable/map repeat evidence is deterministic within each tool. Different preparation work and cross-tool semantic equivalence are not equated.

| Cell | Fission median seconds | Kuna median seconds | Fission / Kuna |
| --- | ---: | ---: | ---: |
| 03 | 2.1221 | 0.2063 | 10.29x |
| 22 | 32.4294 | 2.8127 | 11.53x |
| 27 | 0.9167 | 0.1107 | 8.28x |
| 40 | 1.0069 | 0.3354 | 3.00x |

**Kuna-level speed is not achieved.** This serial-function lane is distinct from the default-worker paired large comparison and must not be substituted for it. No cross-tool speed-improvement claim is based on changes in these ratios.

## Reproduction and retained artifacts

Reuse the frozen population and [instruction-probe reproduction command](instruction-probe-performance.md#reproduction-and-artifacts). Alternate the two immutable executable paths without changing resources or worker policy. Default-worker large runs use `/usr/bin/time -l` and an external 600-second cap. The full regression replay uses at most two independent producer processes; its concurrent elapsed times are not used for speed claims.

Full drivers (`paired.py`, `large.py`, `vs_kuna.py`, `replay.py`), stdout/stderr, input/reference indexes, source patch/hashes and binaries remain in local `benchmark/artifacts/performance/2026-10-15-decode-storage/`. Their evidence hashes, per-run results and excluded time keys are in the compact JSON. The baseline reference is `2026-10-15-decode/replay/replay-<cell>.json`, authenticated by the reference index. No persisted result cache is supplied to either producer.
