# Decompiler hot paths and work-limit usage

Mechanical performance work plus a measurement of how far the decompiler's deterministic work limits are reached, 2026-11-23. **No semantic, readability, score or ranking improvement is claimed.** Decompiled output is unchanged at the default limits.

## Changes and invariants

Four commits. The speedup, distribution, raised-limit and gate measurements below were taken on base `100878b1593890e578525d372738b5e14778b50d`; the branch was then rebased onto main `1f10b8c789b2db66a70277f8cbe884010968b7d8` and the output-equality and instruction measurements were repeated there ("Re-verified on current main").

1. **`perf(midend)`** removes repeated scans. The register-name predicates in `variable_merge` no longer allocate a lowercased `String` per call inside the O(n²) pair loop, and name-derived facts are computed once per local. The SSA initialization dataflow keeps states as bit vectors over the tracked names instead of cloned `BTreeSet<String>`. `SpaceOpIndex`, built once in `PreviewBuilder::new` and shared by clones, files ops under 16-byte buckets of their varnode ranges, so `collect_output_use_sites_outside_block` and `first_reaching_output_use_after_block_exit` visit only ops that can match; `varnode_matches_key` stays the sole judge. `TraceDag::push_branches` computes `check_retirement` once per branch point per step. Each rewrite keeps the implementation it replaced as the reference in a differential test.
2. **`refactor(midend)`** turns the four work limits (whole-structuring checkpoints, condition-recovery subcalls, SESE region-proof attempts, varnode-lowering steps) from module constants into `ResourceLimits` on `NirRenderOptions`. The defaults are the former constants and the field is `#[serde(default)]`.
3. **`feat(midend)`** records, per limit, the peak its counter reached and how many times it crossed the limit in `NirBuildStats` (`resource_<limit>_peak`, `resource_<limit>_trip_count`). The meter is observation only: one `ResourceUsage` is shared by a builder and all its clones and forks, so discarded attempts are counted, and nothing reads it to decide. The enforcing counters are untouched.
4. **`feat(cli)`** adds `decomp --work-limit NAME=VALUE` and `[decompiler.work_limits]` in `fission.toml` (flag over file over default).

A limit counts a unit of work, never time, so where it trips does not depend on machine speed or load. These commits do not change that; they make the numbers visible and settable.

## Method

Apple M4 Pro, 14 logical CPUs, 24 GiB, macOS 26.5.1, rustc 1.97.0, **`quick-release` profile** (no LTO, 16 codegen units), one function at a time (`FISSION_RUST_DECOMP_WORKERS=1`), `--no-db`, `FISSION_BENCHMARK_NO_CACHE=1`. Baseline CLI is base `100878b15` (SHA256 `ac0a2f41…cf48735`); final CLI is the fourth commit's tree (SHA256 `8438a1af…20c325c6`). Inputs are stripped binaries of the DecBench eval-kit sample-set; none was executed.

The machine was shared: load average was 40-56 during every run. Wall-clock medians taken under that load overlapped between builds and are **not reported**. The comparison is instructions retired (`/usr/bin/time -l`), which load barely moves, and output equality.

Baseline and final were built from separate worktrees, and the two binaries' SHA256 were checked to differ before measuring: two worktrees that share one cargo target directory can hand back each other's binary as up to date.

Outputs are compared on every field except `*_duration_ms`, `*_sec`, `total_time_ms` and the new `resource_*_peak` / `_trip_count` keys.

## Executed instructions

Baseline and final measured back to back, same inputs and address lists. Output is identical on every function.

| Input | Functions | Baseline | Final | Change |
|---|---:|---:|---:|---:|
| bin_074.elf, the four heaviest | 4 | 98.29G | 76.08G | -22.6% |
| bin_074.elf | 383 | 224.85G | 194.03G | -13.7% |
| bin_121.so | 133 | 182.70G | 159.88G | -12.5% |
| bin_164.elf | 17 | 3.97G | 3.47G | -12.8% |
| bin_193.elf | 250 | 193.74G | 175.49G | -9.4% |
| bin_172.dll | 125 | 20.50G | 19.78G | -3.5% |

908 functions in five binaries, plus the four-function subset of one of them. Large functions gain most, small-function inputs least. The usage meters cost about 0.1% of instructions (bin_074, 383 functions: 194.57G without, 194.79G with) and the limit option about 0.03%. This is an instruction count, not a time or throughput claim.

## Re-verified on current main

After rebasing onto main `1f10b8c789b2db66a70277f8cbe884010968b7d8` (11 upstream commits beyond the first rebase target, one overlapping file, merged cleanly) the baseline CLI was rebuilt from that main in its own target directory and the comparison repeated on the same inputs. Output is identical on every function.

| Input | Functions | Baseline | Final | Change |
|---|---:|---:|---:|---:|
| bin_074.elf, the four heaviest | 4 | 81.61G | 66.50G | -18.5% |
| bin_074.elf | 383 | 202.67G | 182.89G | -9.8% |
| bin_121.so | 133 | 162.91G | 142.55G | -12.5% |
| bin_164.elf | 17 | 3.61G | 3.24G | -10.2% |
| bin_193.elf | 250 | 175.77G | 159.19G | -9.4% |
| bin_172.dll | 125 | 19.11G | 18.86G | -1.3% |

Upstream main is itself faster than the original base on these inputs (bin_074, four heaviest: 98.29G then, 81.61G now), so the relative gain is smaller than in the first table. The corpus distribution, raised-limit and gate measurements were not repeated on this main.

## Work-limit usage across a corpus sample

45 of the 202 sample-set binaries (every fifth by size), 7,565 functions, 7,524 with stats, 41 fell back. Per binary, the 25 largest functions plus an even stride, at most 300. **The sample over-represents large functions**, so the overall rates below are higher than a whole-corpus rate; the by-size rates are not affected by that.

| Limit | Default | Max peak | p50 | p90 | p99 | ≥50% | Functions tripped |
|---|---:|---:|---:|---:|---:|---:|---:|
| `structuring_total_work` | 200,000 | 204,435 | 40 | 728 | 142,581 | 90 | 62 |
| `condition_recovery_subcalls` | 512 | 513 | 7 | 64 | 513 | 318 | 238 |
| `sese_region_proof_calls` | 20,000 | 20,022 | 10 | 63 | 771 | 2 | 1 |
| `varnode_lowering_work` | 20,000 | 20,340 | 16 | 117 | 908 | 4 | 4 |

243 functions (3.2%) tripped at least one limit, in 31 of the 45 binaries. All 62 that crossed the structuring limit also crossed the condition-recovery limit; none of the 41 fallback functions tripped one.

| Validated p-code ops | Functions | Tripped |
|---|---:|---:|
| < 100 | 3,304 | 0 (0%) |
| 100-300 | 2,426 | 0 (0%) |
| 300-1,000 | 1,352 | 39 (2.9%) |
| 1,000-3,000 | 345 | 134 (38.8%) |
| ≥ 3,000 | 97 | 70 (72.2%) |

On the 233 functions the DecBench gate scores, 22 (9.4%) tripped at the default: 21 only condition recovery, one also the structuring limit.

## Raising the limits

The 243 tripped functions were decompiled again with every limit raised (`condition_recovery_subcalls=4096`, `structuring_total_work=2000000`, `sese_region_proof_calls=200000`, `varnode_lowering_work=200000`).

| Tripped at default | Functions | Output identical when raised |
|---|---:|---:|
| condition recovery only | 176 | 158 |
| structuring total work (and condition recovery) | 62 | 9 |
| varnode lowering | 4 | 0 |
| SESE region proof | 1 | 0 |

- 76 of 243 functions (31%) produce different output; no function changed its fallback status. Where output changed, the line-count delta ranges from -273 to +631 and the `goto` delta from -14 to +163. Whether the changed output is better was not measured here.
- Even raised, 36 functions still crossed the structuring limit (peak 2,004,290) and 111 the condition-recovery limit (peak 4,097), so some functions are not bounded by any of these values. The SESE peak among the raised runs was 53,419, above its default.
- Instructions for the same 243 functions: 3,697.0G at the defaults, 3,974.2G raised (+7.5%).

Peak usage against validated p-code op count, over functions that did not trip, fits `peak ≈ ops^α`: `structuring_total_work` α=1.42 (R²=0.66, 5,257 functions), `condition_recovery_subcalls` 0.96 (0.56), `sese_region_proof_calls` 0.90 (0.67), `varnode_lowering_work` 0.86 (0.67). Size explains about 60% of the variance; the per-op demand of the largest outliers is 50 to 2,000 times the median, so a size-proportional limit would follow the trend but not remove the cut-off for runaway searches.

## DecBench sample-set gate

`decbench_sample_set_gate.py check`, 250 functions decompiled, 233 scored for GED, caches disabled, same environment for the three runs:

| CLI | HIR GED perfect | HIR GED total distance | HIR types perfect |
|---|---:|---:|---:|
| baseline code | 67/233 | 7,529 | 15/223 |
| final, default limits | 67/233 | 7,529 | 15/223 |
| final, limits raised as above | 67/233 | 7,505 | 15/223 |

- Baseline vs final at the defaults: **0 of 233 rows differ** in GED or output node count.
- Default vs raised: **1 of 233 rows differs**, `bin_029.elf` `readproc` (source graph 104 nodes): output nodes 71 → 77, GED 127 → 103. It is the one scored row that crossed the structuring limit.

`bin_029` is not among the 45 sampled binaries, so this row is not one of the 76 functions above, and none of those 76 is a scored gate row: their changed outputs were not scored. One row is below this project's bar for acting on a shape (three functions across two programs), and structure distance prices node count and degree only, so the lower total distance is recorded as measured and **not** as a quality gain.

The gate exits 1 for all three runs, including the baseline code: the stored floor is 68 / 16 perfect and this environment scores the unmodified base 67 / 15. Two environment gaps explain part of that and apply equally to every run: the published source CFGs for `nuttx` are absent, and the `decbench` and `rust_joern` editable installs point into a `vendor/` that is a self-referential symlink here. The gate was run from a scratch copy of the script with `PYTHONPATH` set to `scorer-vendor-snapshot` and `~/rust-joern`, and the kit linked in. The comparison between the three runs is valid; the floor comparison is not.

## Validation

- Each of the four code commits was rebuilt and tested on its own after the rebase onto main `775139eb5`: `cargo fmt --all --check`, `cargo check --workspace --tests` and `cargo nextest` over `fission-core`, `fission-pcode`, `fission-midend-*`, `fission-cspec`, `fission-decompiler` and `fission-cli` (2,841 / 2,849 / 2,854 / 2,867 tests). The final head on `1f10b8c78` passes fmt, the workspace check, CI-line clippy over the touched crates and `scripts/check/owner_boundaries.sh`; of 2,881 tests, 2,878 pass and the three `debug_session_cli` tests below fail.
- `fission-cli` `debug_session_cli` has three interactive-emulator tests that fail with a 5 s timeout, zero to three of them per run. They failed the same way on the unmodified original base (`100878b15`); they were not re-run on the newer main.
- `scripts/test/check_resource_path_literals.sh` reports three hits in `crates/fission-signatures`, files this change does not touch.
- New tests: differential tests for the register-name predicates, the outside-block use scan (including bucket-boundary, oversized and zero-size varnodes) and the first-reaching-use scan; the `ResourceLimits` defaults, serde compatibility, override and trip paths; the meter's peaks, trips and merge rule; `--work-limit` and `[decompiler.work_limits]` parsing, precedence and rejection of bad input.
- Rebasing exposed a semantic collision that merged without a textual conflict. Upstream `b6d13fb36` makes an output-less `Call`/`CallInd` end a return-register definition, which no varnode match can see, so the `SpaceOpIndex` premise (an op outside the candidates cannot redefine) no longer held for `first_reaching_output_use_after_block_exit`. For a return register the scan now also visits every output-less call. The differential fixture gained a return-register definition whose only read sits behind a call; with the fix disabled both that fixture and the dedicated test fail, with it enabled they pass.
- Rejected during the work: a dense-bitmatrix `DomTree::dominates`, because `dominates` was 0.1% of the profile and `push_branches` did not move. It is not in the branch.

## Limits of this evidence

- `quick-release`, instructions retired, one function at a time on a loaded machine; no wall-clock, release-profile or parallel-throughput claim.
- 908 functions in five binaries for output equality; 45 of 202 binaries for the distribution. One PE (a DLL); no Mach-O.
- The gate run is one environment with the floor mismatch above; no full-dataset sweep and no `corpus/scale` run. The 76 changed outputs from the corpus sample were not scored.
- The distribution, raised-limit and gate measurements are from `100878b15`, 146 commits behind main when taken. Only output equality and instruction counts were repeated on the rebased branch, and main keeps moving: re-verify against the base the pull request merges into.

## Reproduction

```sh
cargo build --profile quick-release -p fission-cli
# instructions: FISSION_RUST_DECOMP_WORKERS=1 /usr/bin/time -l fission_cli --resource-root . decomp BIN \
#   --addresses-file ADDRS --json --no-db --no-header --no-warnings --timeout-ms 120000
fission_cli decomp BIN --addr 0xa746 --json --work-limit sese_region_proof_calls=1   # trips show in preview_build_stats
```

Local raw artifacts, not committed: `benchmark/artifacts/performance/2026-11-23-hotpaths/` (`corpus_run.py`, `raise_exp.py`, `paired.py`, `cmp.py`, the per-function corpus rows, the raised-limit results, the three gate dumps and wrappers). Compact evidence: [`decompiler-hotpath-work-limits-performance.json`](decompiler-hotpath-work-limits-performance.json).
