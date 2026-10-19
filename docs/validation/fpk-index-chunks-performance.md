# Reuse decoded FPK hash-index chunks

Mechanical performance validation, 2026-10-16. **Kuna-level speed is not achieved.** No decompiler quality, score, ranking, recompilation or guest-execution claim.

## Owner and invariant

Baseline main `6d6d8872a67a3beae2b1eb811d6240a5b6ac80be`, release SHA256 `cad83688200d2aeca0f5b5b559d4df5ebf0b1c2a3cf218e7c83aa14b3fce20a4`. Candidate release SHA256 `9b1abc435f8a4c2240750edad696bc5cf96375e10ba8da2ee1f0bbeb8b460b96`. [Compact evidence](fpk-index-chunks-performance.json) contains exact measured source hashes, native ranges and comparisons; [pre-implementation proposal](../proposals/2026-10-16-fpk-index-chunks.md).

`FpkReader::records_by_key` decoded an identical zstd hash-index chunk for every query, including misses. Existing payload-block and positive-candidate caches do not cover these index bytes. The reader now lazily retains successful length-checked chunks in reader-owned OnceLock slots. Warm callers borrow the same bytes without a global mutex or Arc increment. The slot count is bounded by the mapped archive directory; retained decoded memory grows with newly touched chunks, not with distinct query keys, and lives until reader destruction. Cold concurrent callers can decode the same chunk more than once before one publishes it. Failed decoding/length validation stays uncached and retries.

Lookup traversal, duplicate-key boundaries/order, record/payload validation, errors, matching scores/flags, file formats, resources, workers and instruction/time limits are unchanged. No new dependency, program-fact map, CLI/JSON/adapter change or oracle input.

## Observation and controls

Temporary optimized cfg(test) FactStore diagnostics on a real large input put matching at median 2.1503 wall seconds / 28.12 process CPU seconds over three fresh runs. The full xref relation scan took approximately 0.14 seconds; it was not the dominant phase. These phase diagnostics motivated inspecting the FPK lookup owner; they do not attribute every matching sample to decompression. All temporary instrumentation is excluded from the shipped source and product binaries.

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions / 45 cells (Linux 108, Windows 97, macOS 45), all 45 current stripped input hashes verified. Sample manifest SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Sources and debug ground truth stay outside producer inputs. [Qualification dataset](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).

Native M4 Pro, 14 logical CPUs, 24 GiB; release LTO/codegen-units=1. Fresh sequential CLI processes, identical requested addresses/resources/cwd, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second function limit. Internal resource/runtime caches remain enabled. OS caches/scheduling uncontrolled; no own build, test or profiler overlapped timing. The full replay used two processes only for output validation, not latency claims.

## Native baseline/candidate

Six requested functions in a large Windows input. Three alternating pairs passed the predeclared >=2% separate-median latency gate:

| Initial controls | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 11.680213 | 10.335352 | -11.51% |
| Process CPU seconds | 100.895432 | 81.626528 | -19.10% |
| Maximum RSS bytes | 2,429,091,840 | 2,432,925,696 | +0.16% |

Five independent confirmation pairs inverted the initial order; eight total pairs give four first starts/tool. The confirmation gate required >=2% median paired wall reduction and no >2% CPU regression.

| Confirmation | Baseline median | Candidate median | Median paired change |
|---|---:|---:|---:|
| Wall seconds | 11.732475 | 10.321781 | -11.88% |
| Process CPU seconds | 101.155339 | 81.463894 | -19.56% |
| Maximum RSS bytes | 2,402,467,840 | 2,446,065,664 | +1.19% |

Separate medians and median paired ratios are different statistics; separate-median changes were -12.02% wall, -19.47% CPU and +1.81% RSS. The memory increase is an explicit cost of retaining decoded index chunks. This does not establish peak memory behavior on all archives.

Small inputs, five pairs each (requested rendering worker=1; preparation still uses Rayon): Linux 1.849159 -> 1.842974 seconds (-0.33%), Windows 0.818615 -> 0.812845 (-0.70%), macOS 0.924835 -> 0.932199 (+0.80%). No useful small-input latency improvement is claimed.

## Fixed Kuna 1.808 comparison

Four frozen batches / 22 requested addresses; three alternating runs/tool/batch/setting, rendering jobs=1 and 14 (14 lane fixes Rayon=14). Each tool chooses its own faster observed median separately. Fission render=1 does not serialize preparation/FID.

| Cell | Fission jobs 1 | Kuna jobs 1 | Fission jobs 14 | Kuna jobs 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.8392 | 0.1834 | 1.2579 | 0.3237 | 6.86x |
| 22 | 11.4333 | 2.6818 | 10.2910 | 5.3986 | 3.84x |
| 27 | 0.8124 | 0.1058 | 0.6401 | 0.2370 | 6.05x |
| 40 | 0.9299 | 0.3195 | 0.8803 | 0.6731 | 2.75x |

Fission remains **2.75–6.86x slower**. These are batch process latencies, not per-function throughput, matched semantic quality, latest Kuna or a ranking. Previous-run ratios are not paired before/after evidence. Fixed Kuna executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`.

## Validation

- 17 targeted FPK nextest tests pass: lazy chunk retention, distinct absent keys, concurrent cold lookups, stable retained storage, bounded slots, duplicate keys across chunk boundaries, exact records/order, corrupt compressed bytes, length mismatch and existing invalid payload-locator errors.
- 2,211 related nextest tests pass / 9 skipped across sleigh/signatures/static/pcode/decompiler/CLI/emulator. Crate checks, signatures Clippy, release CLI, workspace fmt and owner scan pass. Existing unrelated example warnings remain.
- The four-ISA 1,024-record complete raw-state/lift/error matrix is identical: 746 successes, 278 errors, 27 parser traces; uncompressed SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Host backtraces are excluded from error comparisons; tests use RUST_BACKTRACE=1.
- FID hash/resolver-order matrix identical: 512 inputs, 383 eight-instruction extents, 473 successful hashes, SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`.
- Fresh candidate **250/250 complete function records equal** pinned current-main outputs, excluding recursively only `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. Every other existing code/type/variable/PC/map/error/fallback/evidence/counter field is compared. No other draft's mapping support was introduced.
- Focused output sets repeat across eight large pairs, five small pairs each and all 24 Fission worker-lane runs; all match the fresh full replay. Kuna requested-address sets and nonempty/error-free outputs are checked; its selected code/variable/map digests repeat. Only the candidate full population was regenerated in this round; pinned baseline output is the preceding accepted build of the same main source. No execution, recompilation or quality score run is claimed.
- Three preceding prototypes (raw-decode seed buffer, emitter space-table borrowing, persistent probe chunks) failed useful/reproducible latency gates and were fully restored. Their patches/binaries/rejection evidence remain local; none is included here.


### Post-patch phase diagnostic

| Optimized cfg(test) phase | Baseline median wall / CPU seconds | Candidate median wall / CPU seconds |
|---|---:|---:|
| binary_loaded:functions=38734 | 3.1739 / 3.17 | 3.1916 / 3.19 |
| no_return_ready:seeds=38186 | 3.4281 / 42.28 | 3.4419 / 43.25 |
| function_hashes_ready:38023 | 2.6838 / 36.00 | 2.7996 / 38.18 |
| matches_ready:4812 | 2.1503 / 28.12 | 0.1672 / 1.08 |

Same temporary instrumentation, three fresh runs/source, optimized test profile. Observation overhead and scheduling differ from product CLI controls. All candidate runs retain 38,023 function hashes, 4,812 matches and 38,737 names; these counts alone are not full FactStore content equality. Matching no longer dominates this diagnostic; loader, no-return and function hashing remain substantial. All temporary source has been restored byte-for-byte.

## Reproduction and remaining work

Run targeted `cargo nextest run -p fission-signatures -E 'test(fpk::tests)'`, then the related package suite and checks, signatures CI-scoped Clippy, `cargo build -p fission-cli --release`, `cargo fmt --all --check` and `bash scripts/check/owner_boundaries.sh`.

Full local artifacts are under `benchmark/artifacts/performance/2026-10-16-fpk-index-chunks/`: immutable binaries, source snapshot/tree/patch, proposal, input hashes, all native command arrays/JSON/stderr, drivers and check logs. `controlled.sh` freezes the release binary and gates three large pairs, five confirmation pairs, small pairs, replay and Kuna lanes. Driver paths are local; production resource resolution is unchanged. Temporary phase instrumentation and its source/binary hashes are retained separately under `2026-10-16-preparation-phases/` and `phase-after/`.

The remaining gap needs measured work on preparation and binding rather than skipping functions or facts. The baseline phase diagnostics showed loader serial work, no-return preparation and full FID function hashing to be substantial. No matched Kuna phase timing establishes how much of its total advantage belongs to each subsystem.
