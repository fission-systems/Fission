# Reuse literal results in direct DIE identity matching

Mechanical performance validation, 2026-10-16. **Kuna-level speed is not achieved.** No decompiler quality, semantic/readability score, ranking, recompilation or guest-execution claim.

## Owner and domain contract

Baseline main `1fbad272f9414ba991a37420f051ab0b7701ff0b`, release SHA256 `6198d03abdd9f4cbebc6dfdc7ce3c967326ba25c46655e3799ddaaa53d04fb82`. Candidate release SHA256 `ac9d1d5a229ca76725a2176d2c54bc2a0d7025c28cbfc60948fb47998d43e8ef`. [Compact evidence](die-direct-strings-performance.json) contains exact source/tree/patch hashes, native ranges and all output checks; [pre-implementation proposal](../proposals/2026-10-16-die-direct-strings.md).

`die_compat_identity_with_db` calls `match_signature` directly and repeats primitive evaluation for raw evidence. Its unprepared StringMatch fallback scans the full raw input bytes for every literal occurrence, including misses. Same unchanged DIE owner in the preceding native profile showed 168 ancestor-classified self samples in this path (diagnostic only, not a wall-time share). Three fresh baseline CLI controls completed before implementation, median 10.1709 wall / 80.8519 CPU seconds, with repeated non-duration output.

The existing DIE owner now factors its literal-cache machinery into one helper. Normal `match_binary` passes the same capped slice and all the same rules as before. Direct identity matching prepares only compatible signatures' nonempty literal values over the same **full input domain as its legacy fallback**, filling the existing per-matcher boolean cache. It uses the existing RegexSet and scalar fallback, deduplicates work by literal value and preserves each rule occurrence/count/order. It never caches final detections, confidence or evidence. Section and EP caches remain unprepared in the direct path; preparing them could change current rule results.

Configured budgets and normal capped behavior are unchanged. The direct identity fallback already searched beyond a configured prefix cap; this patch deliberately preserves that legacy behavior and is **not a scan-budget policy correction**. Tests lock down the difference between direct full-input matching and normal capped preparation, including limits 0/2, while preserving all primitive labels/counts, scores, details and provenance. Reusing a matcher for another prepared input clears and recomputes string results.

No new cache state field, dependency, discovery/identity promotion policy, public Rust/CLI/JSON API, address mapping, worker/timeout or persistent program fact map. Retained strings/booleans and transient regex work are bounded by the supplied compatible rule set; no universal peak-memory reduction is claimed.

## Controls

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489), 250 stripped functions / 45 cells: Linux 108, Windows 97, macOS 45. All 45 current input hashes verified. Sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Sources/debug ground truth remain outside producer inputs. [Qualification dataset](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).

Native M4 Pro / 14 logical CPUs / 24 GiB; release LTO/codegen-units=1. Fresh sequential CLI processes, same input/address sets/resources/cwd, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second function limit. Internal runtime/resource caches remain enabled. OS cache and desktop scheduling uncontrolled; no own build/test/profiler overlaps native timing. The fresh full replay uses two processes for output verification only, not speed claims.

## Direct literal-cache change: large-input decompilation

Six requested functions in one large Windows input. Three alternating pairs passed the predeclared >=2% separate-median wall reduction gate.

| Initial controls | Baseline median | Candidate median | Change |
|---|---:|---:|---:|
| Wall seconds | 10.205897 | 8.384415 | -17.85% |
| Process CPU seconds | 80.961033 | 79.279170 | -2.08% |
| Maximum RSS bytes | 2,453,995,520 | 2,434,990,080 | -0.77% |

Five independent confirmation pairs invert initial order, giving eight total pairs/four first starts per tool. Gate: >=2% median paired wall reduction, <=2% CPU regression, explicit RSS.

| Confirmation | Baseline median | Candidate median | Median paired change |
|---|---:|---:|---:|
| Wall seconds | 10.241865 | 8.287436 | -18.42% |
| Process CPU seconds | 81.267206 | 79.475469 | -1.62% |
| Maximum RSS bytes | 2,420,441,088 | 2,423,865,344 | +0.33% |

Separate-median changes (different statistics): seconds -19.08%, cpu_seconds -2.20%, max_rss_bytes +0.14%. All ranges and paired ratios are recorded.

## Additional RE command and combined checkpoint

`list --json --no-db --include-nonuser-functions`, same large input / three alternating pairs: all **42,853 complete function records per run identical**. This is an end-to-end command, not an isolated loader phase or the predeclared acceptance gate.

| List controls | Baseline median | Candidate median | Separate-median change |
|---|---:|---:|---:|
| Wall seconds | 4.561876 | 2.654212 | -41.82% |
| Process CPU seconds | 25.302948 | 23.200428 | -8.31% |
| Maximum RSS bytes | 689,455,104 | 691,896,320 | +0.35% |

The **combined checkpoint** directly compares original main `b7c84359322a8e7be6d387d4f6369bd59fee5f36` / binary `9b1abc435f8a4c2240750edad696bc5cf96375e10ba8da2ee1f0bbeb8b460b96` against this final candidate. It includes the accepted MSVC vtable index (PR #339) plus this literal cache. No percentages were added or compounded. Three alternating pairs gave wall medians 11.210499 -> 8.425301 seconds. An additional independent five inverted-order pairs follows:

| Combined confirmation | Original baseline median | Final candidate median | Median paired change |
|---|---:|---:|---:|
| Wall seconds | 11.132579 | 8.303369 | -25.21% |
| Process CPU seconds | 82.690130 | 79.980115 | -3.09% |
| Maximum RSS bytes | 2,451,914,752 | 2,429,272,064 | -0.74% |

These are additional descriptive same-host checkpoint controls, not the literal-cache-only acceptance gate or a whole-corpus claim. Complete six-function non-duration outputs agree across both baselines and final candidate. Separate combined medians differ from median paired ratios; full values are retained.

## Small inputs and fixed Kuna 1.808

Five small-input pairs each, requested render worker=1 (preparation still uses Rayon):

| Cell | Baseline seconds | Candidate seconds | Change |
|---|---:|---:|---:|
| 03 | 1.976189 | 1.977128 | +0.05% |
| 27 | 0.853665 | 0.845584 | -0.95% |
| 40 | 1.002514 | 0.975024 | -2.74% |

Small controls are bounded observations, without independent confirmation for each small input or a general small-input speed claim.

Four frozen batches / 22 requested addresses / three alternating runs per tool and setting at render jobs 1 and 14 (14 lane fixes Rayon=14). Each tool chooses its own faster observed median separately; Fission render=1 does not serialize preparation/FID.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.9715 | 0.2013 | 1.3287 | 0.3326 | 6.60x |
| 22 | 9.5753 | 2.8055 | 8.3641 | 5.6320 | 2.98x |
| 27 | 0.8520 | 0.1107 | 0.6622 | 0.2512 | 5.98x |
| 40 | 0.9930 | 0.3436 | 0.9522 | 0.7133 | 2.77x |

Fission remains **2.77–6.60x slower**; the large batch is **2.98x**. Fixed Kuna executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`. This is not latest Kuna, equal quality, per-function throughput, whole-corpus speed or ranking evidence. Previous-turn ratios are not paired before/after measurements.

## Validation and bounds

- Three targeted tests pass: exact scalar/cached results and complete Detection Debug across binary bytes (invalid UTF-8, UTF-8 literals, NUL/control bytes, metacharacters, case, duplicates, misses); compatible-format selection; unchanged full direct domain vs capped normal domain; untouched direct section/EP cache; recomputation for another input; complete legacy raw-summary/evidence JSON across PE/ELF and caps 0/2/64.
- **2,391 related nextest tests pass / 9 skipped** including loader/analysis DB and sleigh/signatures/static/pcode/decompiler/CLI/emulator. Checks, loader CI-scoped Clippy, release CLI, workspace fmt and owner scan pass. Existing unrelated example warnings remain. A test fixture's initial Option offset type error was corrected before the passing run; no production behavior workaround was added.
- **45 complete `info --identity` JSONs identical**, with no excluded fields: inventory counts and complete identity summaries/detections/raw primitive labels/counts, scores, details, provenance and resource/policy evidence. **Four representative `info --identity --detections` JSONs identical**, including the normal integrated detection path.
- **45 whole function lists / 334,275 records identical**, including nonuser units and every listed name/address/size/classification/provenance/thunk field. Not a complete serialization of every LoadedBinary/ProgramSnapshot field.
- Fresh candidate **250/250 complete function records equal** pinned baseline, excluding recursively only seven durations: `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. Every other existing code/type/variable/PC/map/error/fallback/evidence/counter field is compared; no companion draft's line mapping support was introduced.
- Focused sets repeat across the eight primary large pairs, small pairs, all 24 Fission worker-lane runs, list controls and combined checkpoint pairs. Kuna exact requested sets and nonempty/error-free output are checked; its selected code/variable/map digests repeat. Only candidate full 250/function lists were regenerated this round; baseline full outputs are pinned to the preceding accepted build of the same baseline source. Identity and integrated-detection baselines were freshly captured in this round.
- Four-ISA 1,024-record raw-state/lift/error matrix unchanged: 746 successes / 278 errors / 27 parser traces, uncompressed SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Host backtraces excluded; tests use RUST_BACKTRACE=1.
- 512-input FID hash/resolver-order matrix unchanged: 383 eight-instruction extents / 473 successful hashes, SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`.
- No complete-function execution, recompilation, external semantic bake or score rerun. Output parity and focused tests are not an execution proof or a decompilation-quality gain.

## Reproduction and remaining work

Run `cargo nextest run -p fission-loader -E 'test(direct_string_cache) | test(prepared_identity_strings)'`, then loader/analysis-db and downstream nextest/checks, loader CI-scoped Clippy, `cargo build -p fission-cli --release`, `cargo fmt --all --check` and `bash scripts/check/owner_boundaries.sh`.

Full local evidence: `benchmark/artifacts/performance/2026-10-16-die-string-rules/`: immutable source/tree/patch/binaries, proposal, baseline controls, input hashes, all identity/function/output JSONs, native commands/stderr and validation logs. `controlled.sh` gates primary controls/confirmation before small controls, identity/function parity, fresh replay and Kuna lanes, then adds integrated detection, list and combined checkpoint controls. Driver paths are local; production resource resolution is unchanged.

The remaining measured preparation work includes no-return and FID SLEIGH binding/lifting/state costs. Further changes need fresh ownership profiles and the same complete-output/error contracts; do not skip facts/functions or change decoding semantics to improve latency. No matched Kuna phase profile apportions its total advantage.
