# Native FID binding reuse: performance and preservation

Mechanical performance validation, 2026-10-17. **Kuna-level speed is not achieved:** the four measured batches remain 2.67–6.89x slower than fixed Kuna 1.808. No decompilation quality, score, ranking, recompilation or guest-execution claim.

## Frozen source and compiler controls

Baseline main `3082064e747cd73dda2dd98777a5eb25e6c1c9d5`, source-only candidate tree `cc083352c0ea8bbae68b6adde67970c9a07fa888`, patch SHA256 `6041f2f88e3550f96ef0bf3988462b31ce93681ccdfeacdb6fea414fa20baa5e`. Baseline CLI SHA256 `70d635cedbadf35832151425616ae3ac24c8703ac3c3b85004b7a356a73f6dd8`; candidate `7a3eb9c923537a3280fb80407822fa1a35188b1c8bb71e25279249347ef3ae61`. Both release builds use Rust 1.97.0, commit `2d8144b7880597b6e6d3dfd63a9a9efae3f533d3`, at the same isolated performance checkout, release opt-level=3/LTO=true/codegen-units=1. The baseline build temporarily restored all nine owned files to main, verified the complete worktree clean, built, then restored the exact candidate source. [Compact evidence](fid-independent-bindings-performance.json) retains hashes, native ranges and ratios; [proposal and owner proof](../proposals/2026-10-17-fid-independent-bindings.md).

The historical `ac9d1d5a` baseline was Rust 1.96.1. Its earlier controls are excluded from this speed comparison. A failed historical-byte-reproduction assertion exposed the compiler difference; the clean rebuilt main became the matched 1.97 baseline before acceptance measurements. The nine previously rejected experiments were themselves compiler-matched on 1.96.1 and remain rejected. No repository toolchain policy or dependency changed.

## Owner and reuse contract

The diagnostic reached 1,921,487 distinct-address FID binding requests, with 219,980 distinct exact byte sequences. A complete native address-use witness found 1,561,875 successful PC-independent requests from 42,436 byte sequences, 359,612 PC-sensitive requests and zero bind failures. This instrumentation explains duplicate work; its times are not performance evidence.

FID normally binds isolated instruction bytes/default context again. The compiled-table owner now retains complete immutable bindings only when the entire native selection/bind attempt, including discarded candidate paths, never consumed PC. Every observed checked PC-plus-cursor addition must succeed at both source and replacement addresses. Actual InstStart/InstNext reads and checked subtable-address additions are observed at their canonical getters. The proof is owner-dependent: future address reads must extend the witness and its tests.

Immutable frontend Arc scope supplies identity; Weak allocation identity prevents pointer reuse, COW changes the owner, scopes restore on unwind and remain thread-local. Callers without ownership proof, PC-sensitive paths and failures retain ordinary native binding. Exact byte keys up to 64 bytes; direct-map 256 slots/one frontend per worker; at most 128 state nodes, 64 KiB owned-payload estimate per entry and 2 MiB total estimate per worker. Owner change/budget overflow clears entries. Shared compiler metadata, fixed slots and allocator overhead are outside the estimate; actual RSS is reported below.

Current-PC p-code completion/descriptions, mask computation, register resolver callbacks/order, digest mixing and failures stay on their original path. No cached p-code, resolver result, digest or final match. Per-instruction bind cache is cleared even on hits, and trace modes retain native replay. No unsafe code, new dependency, ISA-specific cache rule, work/timeout/worker change, serialized schema change or program-fact map.

## Native performance

[Frozen qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489), 250 stripped functions / 45 cells: Linux 108, Windows 97, macOS 45. All 45 input hashes rechecked; sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. [Qualification dataset](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus). Sources/debug ground truth are outside producer inputs.

Native M4 Pro, 14 logical CPUs / 24 GiB. Fresh sequential CLI processes, same input/address sets/resources/cwd, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second function limit. Internal bounded runtime/resource caches remain enabled. No own build/test/profiler overlaps timing. OS cache and desktop scheduling remain uncontrolled. Full replay permits two processes for output verification only.

Six requests in a large Windows input: initial three alternating pairs passed the predeclared >=2% separate-median wall reduction gate.

| Measure | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 8.236454 | 7.453923 | -9.50% |
| Process CPU seconds | 79.967973 | 70.401855 | -11.96% |
| Maximum RSS bytes | 2,401,140,736 | 2,482,814,976 | +3.40% |

Five independent confirmation pairs invert initial order, for eight pairs total/four first starts each. Predeclared gate: >=2% median paired wall reduction and <=2% CPU regression, explicit RSS. This table reports **median paired changes**, which differ from ratios of the separate medians.

| Measure | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 8.292457 | 7.506717 | -9.44% |
| Process CPU seconds | 79.949625 | 70.259473 | -11.85% |
| Maximum RSS bytes | 2,426,126,336 | 2,473,377,792 | +1.95% |

Separate-median confirmation changes: seconds -9.48%, cpu_seconds -12.12%, max_rss_bytes +1.95%. RSS increases; the retained-payload cap is not a process-memory reduction guarantee. Complete six-function records agree across all 16 runs after the duration exclusions below.

Small inputs, five alternating pairs each, render worker 1 (preparation still uses Rayon):

| Cell | Baseline seconds | Candidate seconds | Separate-median change |
|---|---:|---:|---:|
| 03 | 1.915251 | 1.898388 | -0.88% |
| 27 | 0.836845 | 0.834566 | -0.27% |
| 40 | 0.962624 | 0.956785 | -0.61% |

These bounded small controls do not establish a general small-input gain.

## Fixed Kuna comparison

Four frozen batches / 22 requests, three alternating runs per tool/setting. Each tool chooses its own faster observed median among render/jobs 1 and 14; the 14 lane fixes Rayon 14. Fission render 1 does not serialize native preparation/FID. Both exact address sets and nonempty/error-free outputs checked. Fixed Kuna executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`.

| Cell | Fission jobs 1 | Kuna jobs 1 | Fission jobs 14 | Kuna jobs 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.8985 | 0.1932 | 1.3315 | 0.3305 | 6.89x |
| 22 | 8.6468 | 2.7932 | 7.7089 | 5.6194 | 2.76x |
| 27 | 0.8353 | 0.1110 | 0.6602 | 0.2495 | 5.95x |
| 40 | 0.9937 | 0.3437 | 0.9193 | 0.7232 | 2.67x |

This is not latest Kuna, equal quality, per-function throughput, whole-corpus speed or ranking evidence. Earlier-turn ratios are not a paired before/after comparison.

## Output preservation and tests

- 10 targeted nextest tests and **2,401 related tests passed / 9 skipped**. Related crates: loader, analysis-db, sleigh, signatures, static, pcode, decompiler, CLI, emulator. Checks, CI-scoped Sleigh Clippy, release CLI, workspace fmt and owner scan pass. Existing unrelated example warnings remain.
- Positive shared/native complete-state comparisons across low/high PCs; PC-sensitive/malformed/missing-byte fallback; discarded checked-overflow; bind-cache clearing; COW/owner replacement, nested scopes/unwind/thread isolation, reentrant resolver; exact collision/eviction/retention behavior. Four-language state differential covers x86/x64/AArch64/Thumb.
- Canonical 1,024-record complete raw-state/lift/error matrix unchanged: 746 successes / 278 errors / 27 parser traces; uncompressed SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Host backtraces excluded. 512-input FID hash/resolver-order matrix unchanged: 383 eight-instruction extents / 473 hashes; SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`. The hash matrix explicitly enters immutable cache scope.
- 45 complete fresh baseline/candidate `info --identity` JSONs and 4 integrated `--detections` controls identical, with no field exclusions.
- 45 fresh candidate whole-function lists / 334,275 records identical to the preceding accepted pinned baseline, including nonuser units and every listed name, address, size, classification, provenance/thunk field. This is not a serialization of every internal LoadedBinary/ProgramSnapshot field.
- Fresh candidate **250/250 complete function records unchanged** from the pinned baseline. Recursively exclude only seven durations: `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. All other existing code/type/variable/address/map/error/fallback/evidence/counter fields are compared. Full baseline decomp/list outputs remain pinned to the same main source built earlier; only focused pairs and identity/detection baselines are regenerated with the new compiler. Cross-compiler full parity passed, but only matched 1.97 binaries supply speed deltas.
- Focused sets repeat in large/small pairs and all 24 Fission Kuna-lane runs; complete non-duration records match the fresh replay. Kuna selected code/variable/map digests repeat per setting. The full 250 candidate replay is generated once; no full-corpus determinism or execution claim.

## Reproduction and limits

Pin both comparison builds to the same compiler. Run `cargo nextest run -p fission-sleigh -E 'test(fid_binding)'`, then related crate nextest/checks, `cargo clippy -p fission-sleigh --locked -- -D warnings -A clippy::style -A clippy::complexity`, release CLI, workspace fmt and `bash scripts/check/owner_boundaries.sh`. Native replay scripts, full commands/stdout/stderr, input hashes, diagnostic source, immutable source/binaries and native ranges are retained under `benchmark/artifacts/performance/2026-10-17-fid-independent-bindings/`. `controlled.sh` verifies frozen artifacts and gates 3+5 pairs; `accepted-followup.sh` runs the remaining output and Kuna controls. Local paths are measurement configuration, not production resource resolution.

No complete guest-function execution, recompilation, external semantic bake or score rerun. Output parity and focused tests do not prove execution parity. The dynamic witness is maintained alongside native PC-read owners. Cache capacity/budget can decline reuse without skipping analysis. Remaining full-binary preparation and p-code work require fresh profiling; no matched Kuna phase profile attributes its entire advantage.
