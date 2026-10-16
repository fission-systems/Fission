# Index selected MSVC vtable references once per section

Mechanical performance validation, 2026-10-16. **Kuna-level speed is not achieved.** No semantic/readability, score, ranking, guest-execution or recompilation claim.

## Source and owner

Baseline main `b7c84359322a8e7be6d387d4f6369bd59fee5f36`, release SHA256 `9b1abc435f8a4c2240750edad696bc5cf96375e10ba8da2ee1f0bbeb8b460b96`. Candidate release SHA256 `6198d03abdd9f4cbebc6dfdc7ce3c967326ba25c46655e3799ddaaa53d04fb82`. [Compact evidence](msvc-vtable-index-performance.json) records source/tree/patch hashes, native ranges and complete-output checks; [pre-implementation proposal](../proposals/2026-10-16-msvc-vtable-index.md).

`CppAnalyzer::discover_vtable_functions` scanned every eligible `.rdata`/`.data` aligned pointer slot once per already selected Complete Object Locator (COL). Same-main symbolized profiling showed that nested scan on a real large input (73 ancestor-classified self samples, diagnostic only). Before implementation, three baseline CLI controls repeated complete output and gave median 10.8638 wall / 80.5413 CPU seconds. Host scheduling differed from the preceding turn; the new paired controls below are authoritative.

The existing loader owner now builds a temporary inverse lookup restricted to the COLs returned by unchanged `analyze_msvc_classes`. Each eligible section is scanned once; starts are retained in the original section/slot order and replayed in the original class order. Duplicate COL inputs replay their same starts. The unchanged payload sweep keeps executable-section precedence, zero/unmapped/nonexecutable stops, function names/provenance, sorting and deduplication. Empty class sets skip scanning.

This is an operation-local query index, not persistent program metadata. No discovery policy, RTTI parsing, pointer width/alignment, byte-view availability, identity policy, CLI/JSON/adapter, worker/limit, public Rust API or dependency change. Temporary memory is proportional to selected COLs and their matching aligned slots; pathological inputs with many references may retain more starts. No universal peak-memory improvement is claimed.

## Controls

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions / 45 cells, Linux 108 / Windows 97 / macOS 45. All 45 current stripped input hashes verified. Sample manifest SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Source/debug oracles are outside producer inputs. [Qualification dataset](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).

Native M4 Pro, 14 logical CPUs, 24 GiB, release LTO/codegen-units=1. Fresh sequential CLI processes, same inputs/requested addresses/resources/cwd, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second function limit. Internal runtime/resource caches remain enabled. OS caches/desktop scheduling uncontrolled (an ordinary renderer was active); no own build/test/profiler overlapped native timing. Fresh full replay uses two processes for output validation only, not speed claims.

## Large-input decompilation

Six requested functions in one large Windows input, three alternating pairs. Predeclared first gate: >=2% separate-median latency reduction.

| Initial controls | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 10.983990 | 10.126424 | -7.81% |
| Process CPU seconds | 80.528182 | 80.123700 | -0.50% |
| Maximum RSS bytes | 2,456,059,904 | 2,431,664,128 | -0.99% |

Five independent confirmation pairs invert initial order; eight total pairs give four first starts/tool. Gate: >=2% median paired wall reduction and <=2% CPU regression, with explicit memory reporting.

| Confirmation | Baseline median | Candidate median | Median paired change |
|---|---:|---:|---:|
| Wall seconds | 10.894103 | 10.096893 | -7.05% |
| Process CPU seconds | 80.608326 | 79.776773 | -0.67% |
| Maximum RSS bytes | 2,432,860,160 | 2,450,767,872 | +0.28% |

Separate medians and median paired ratios differ. Separate-median changes: seconds -7.32%, cpu_seconds -1.03%, max_rss_bytes +0.74%. All ranges and paired ratios are in the compact evidence.

## Whole-function listing controls

The additional `list --json --no-db --include-nonuser-functions` experiment uses the same large input and three alternating pairs. It is an end-to-end RE command, not an isolated loader phase or a predeclared acceptance gate. All **42,853 complete function records per run are identical**.

| List controls | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 5.351428 | 4.469044 | -16.49% |
| Process CPU seconds | 26.013279 | 25.046537 | -3.72% |
| Maximum RSS bytes | 689,602,560 | 687,161,344 | -0.35% |

## Small controls and fixed Kuna 1.808

Five small-input pairs each, requested render worker=1 (preparation still uses Rayon):

| Cell | Baseline seconds | Candidate seconds | Change |
|---|---:|---:|---:|
| 03 | 1.934748 | 1.917725 | -0.88% |
| 27 | 0.847490 | 0.836922 | -1.25% |
| 40 | 0.980715 | 0.969500 | -1.14% |

All changes are below 1.3%; no useful small-input gain is claimed. The new lookup is only used by MSVC discovery, so the non-PE controls do not demonstrate a direct optimization effect.

Four fixed Kuna 1.808 batches / 22 addresses, three alternating runs/tool/batch/setting at render jobs=1 and 14 (14 lane fixes Rayon=14). Each tool chooses its own faster observed median separately; Fission render=1 does not serialize native preparation/FID.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.9492 | 0.1954 | 1.3108 | 0.3338 | 6.71x |
| 22 | 11.3210 | 2.7450 | 10.1156 | 5.4820 | 3.69x |
| 27 | 0.8420 | 0.1103 | 0.6620 | 0.2470 | 6.00x |
| 40 | 0.9900 | 0.3329 | 0.9319 | 0.6929 | 2.80x |

Fission remains **2.80–6.71x slower**. These are process batch latencies, not per-function throughput, latest Kuna, equal semantic quality, whole-corpus speed or ranking evidence. Prior-turn ratios are not paired before/after evidence. Kuna binary SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`.

## Validation

- Four targeted C++ analyzer nextest tests pass: two existing real Itanium fixtures plus two new generated 32/64-bit controls. The test-only old nested scan agrees on complete per-class starts/order, duplicate COLs, high pointer bits, aligned/unaligned candidates, ignored/partial/unavailable sections and empty classes. The end-to-end RTTI fixture covers zero/unmapped/nonexecutable/section-end payload stops and deduplicated function metadata.
- **2,388 related nextest tests pass / 9 skipped**, including loader and analysis DB owners plus sleigh/signatures/static/pcode/decompiler/CLI/emulator. Crate checks, loader CI-scoped Clippy, release CLI, workspace fmt and owner scan pass. Existing unrelated example warnings remain.
- **45 full function lists / 334,275 records identical**, including validated nonuser units and every listed address/name/size/classification/provenance/thunk field. This is not a complete serialization of every LoadedBinary or ProgramSnapshot field; the changed owner produces function discoveries only.
- Fresh candidate **250/250 complete function records equal** pinned baseline outputs, excluding recursively only seven durations: `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. Every other existing code/type/variable/PC/map/error/fallback/evidence/counter field is compared. No companion draft mapping support was introduced.
- Eight large pairs, five small pairs each and all 24 Fission worker-lane runs repeat complete non-duration output. Kuna address sets and nonempty/error-free outputs are checked; its selected code/variable/map digests repeat. Only the candidate full 250 population was regenerated this round; pinned baseline outputs come from the preceding accepted build of the same baseline source.
- Four-ISA 1,024-record raw-state/lift/error matrix unchanged: 746 successes / 278 errors / 27 parser traces; uncompressed SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Host backtraces are excluded; tests use RUST_BACKTRACE=1.
- 512-input FID matrix unchanged: 383 eight-instruction extents / 473 successful hashes, exact resolver ordering; SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`.
- No complete-function execution, recompilation, external quality bake or score rerun. This validates performance and output preservation, not improved decompilation quality.

## Reproduction and remaining work

Run `cargo nextest run -p fission-loader -E 'test(analyzers::cpp::tests)'`, then loader/analysis-db and the downstream package suite, crate checks, loader CI-scoped Clippy, `cargo build -p fission-cli --release`, `cargo fmt --all --check` and `bash scripts/check/owner_boundaries.sh`.

Full local evidence: `benchmark/artifacts/performance/2026-10-16-vtable-index/`: immutable binaries/source/tree/patch, pre-implementation proposal, baseline controls, input hashes, full function lists, all native commands/JSON/stderr and validation logs. `controlled.sh` gates large controls/confirmation before small controls, all-function metadata equality, fresh replay and Kuna lanes. `list-pairs.py` supplies the additional RE command experiment. Driver paths are local; production resource resolution is unchanged.

Next observed loader owner is repeated DIE literal-string scanning in identity rule evaluation. Warming its existing complete matcher caches indiscriminately can change section matches and scan-budget behavior, so any subsequent performance change must prove exact rule/evidence/domain preservation. SLEIGH/no-return/FID preparation remains substantial; no matched Kuna phase profile apportions the total gap.
