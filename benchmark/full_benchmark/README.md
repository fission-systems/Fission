# Batch Benchmark

This is the canonical operator-facing benchmark guide for decompilation quality
and Ghidra-parity work. Criterion/perf helpers under `scripts/benchmark/` are
non-canonical and should be treated as microbenchmark tooling only.

Canonical benchmark script root:

- [`benchmark/full_benchmark`](/Users/sjkim1127/Fission/benchmark/full_benchmark)

Default artifact root:

- [`benchmark/artifacts/full_benchmark`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)

Fission keeps two benchmark entrypoints:

- `compare_legacy_preview.py`: historical fixed-seed compatibility benchmark
- `full_decomp_benchmark.py`: whole-binary 2-way benchmark

`full_decomp_benchmark.py` compares:

- `pyghidra`: Python-host baseline
- `fission`: Rust decompiler pipeline

The runner now supports both:

- single-binary benchmarking
- corpus benchmarking via `--corpus-manifest`

## Requirements

- `pyghidra`
- optional `psutil` for RSS / CPU metrics
- `GHIDRA_INSTALL_DIR` configured, or `vendor/ghidra/ghidra-Ghidra_12.0.4_build`
- a `fission_cli` binary
- benchmark-local Python dependencies:
  - `python3 -m pip install -r benchmark/full_benchmark/requirements-benchmark.txt`

## Example Usage

```bash
# Historical fixed-seed comparison
python3 benchmark/full_benchmark/compare_legacy_preview.py \
  samples/windows/x64/putty.exe \
  --addresses 0x140006380 \
  --with-ghidra \
  --repeat 3 \
  --fission-bin target/release/fission_cli \
  --output-dir benchmark/artifacts/full_benchmark/compare_legacy_preview/putty-fixed

# Whole-binary benchmark
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build \
  --output-dir benchmark/artifacts/full_benchmark/putty-balanced-latest

# Faster validation: first N canonical seed functions
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/test_control_flow_x64_O0.exe \
  --limit 30 \
  --timeout 300

# Smoke corpus benchmark
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  --corpus-manifest benchmark/config/benchmark_corpus/smoke_corpus.json \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build \
  --output-dir benchmark/artifacts/full_benchmark/fission-smoke-windows-samples-balanced-latest

# Parity corpus benchmark for Ghidra-reference work
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  --corpus-manifest benchmark/config/benchmark_corpus/parity_corpus.json \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build \
  --output-dir benchmark/artifacts/full_benchmark/fission-ghidra-parity-windows-workbench-balanced-latest \
  --baseline-dir benchmark/artifacts/full_benchmark/fission-ghidra-parity-windows-workbench-balanced-baseline

# Release corpus benchmark against a previously accepted corpus baseline
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  --corpus-manifest benchmark/config/benchmark_corpus/release_corpus.json \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build \
  --output-dir benchmark/artifacts/full_benchmark/fission-release-windows-samples-balanced-latest \
  --baseline-dir benchmark/artifacts/full_benchmark/fission-release-windows-samples-balanced-baseline
```

## Output Naming

Default output directories are split by workflow:

- full benchmark:
  - `benchmark/artifacts/full_benchmark/<binary-or-suite>-<profile>-latest`
  - `benchmark/artifacts/full_benchmark/<binary-or-suite>-<profile>-<YYYYmmdd-HHMMSS>` when `--timestamped-output`
- automation:
  - `benchmark/artifacts/automation/<lane>-<run-profile>-<unix_run_id>`
  - `benchmark/artifacts/automation/latest/<lane>/` for the synced latest view

## Generated Artifacts

- function-level:
  - `*_legacy_vs_preview.json`
  - `*_legacy_vs_preview.md`
- single-binary whole-decomp:
  - `fission_full.json`
  - `ghidra_full.json`
  - `benchmark_summary.json`
  - `benchmark_summary.md`
  - `benchmark_compact_summary.json`
  - `fission_stdout.log`, `fission_stderr.log`
- corpus whole-decomp:
  - top-level `benchmark_summary.json` / `.md`: corpus-global assessment
  - top-level `benchmark_compact_summary.json`: AI-facing compact sidecar
  - per-binary subdirectories:
    - `<binary-id>/fission_full.json`
    - `<binary-id>/ghidra_full.json`
    - `<binary-id>/benchmark_summary.json`
    - `<binary-id>/benchmark_summary.md`

## Regression Validation (limit 2 / 20)

Use the helper below to run `full_decomp_benchmark.py` twice for `--limit 2` and `--limit 20`,
then validate:

- required artifacts exist
- JSON schema keys are present
- per-function address ordering is deterministic
- run-to-run function address lists and key-shape are stable

```bash
python3 benchmark/full_benchmark/validate_limit_regression.py \
  samples/windows/x64/test_control_flow_x64_O0.exe \
  --fission-bin target/debug/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build
```

## Corpus Manifest

The corpus manifest is a JSON file with an `entries` array. Each entry keeps the benchmark contract minimal:

- `id`
- `binary_path`
- `ghidra_project_key`
- `tags`
- `seed_limit`
- `role`

Optional:

- `row_fidelity_targets`: fixed row watchlist for that binary
- `weight`: override the default corpus weight (`primary_canary=2`, others `=1`)

Checked-in defaults:

- [`benchmark/config/benchmark_corpus/smoke_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/smoke_corpus.json)
- [`benchmark/config/benchmark_corpus/release_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/release_corpus.json)
- [`benchmark/config/benchmark_corpus/parity_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/parity_corpus.json)

Current checked-in suites are intentionally constrained to [`samples/windows`](/Users/sjkim1127/Fission/samples/windows) so Ghidra-parity work stays on Windows x86/x64 binaries only.

Top-level manifest metadata:

- `name`
- `suite_tier`: `smoke | release | parity`
- `gate_mode`: `advisory | blocking`
- `dynamic_watchlist_limit`
- `notes`

## Quality Metrics

- fixed-seed:
  - `goto_count`
  - `top_level_label_count`
  - `must_emit_label_count`
  - `empty_if_count`
  - `constant_if_count`
  - `residue_score`
  - preview routing / fallback / structuring counters (historical script)
- whole-binary:
  - address-based function matching
  - success rate and matching coverage
  - per-function raw / normalized similarity
  - aggregate normalized similarity across the concatenated corpus
  - proof/fidelity counters from canonical `NirBuildStats`
  - row-fidelity gate
  - degraded watchlist
- corpus whole-decomp:
  - weighted aggregate normalized similarity
  - x86/x64 split weighted aggregate normalized similarity
  - per-binary row-fidelity gate
  - cross-binary degraded watchlist
  - owner-aware counters from `preview_build_stats`
  - shape-drift proxy totals
  - failure-family distribution
  - per-binary hot-pass summaries

## Owner-Aware Metrics

Corpus and per-binary summaries now surface stable owner-facing counters already present in
`summary.engines.fission` / `preview_build_stats`.

Current owner metrics:

- `alias_unsafe`
- `missing_merge`
- `representative_root`
- `temp_only_lifecycle`
- `dead_temp`
- `representative_downgrade`
- `downgrade_no_aliassafe_source`
- `downgrade_join_conflict`
- `materialization_stabilized`

These appear in:

- per-binary corpus JSON as `owner_metrics`
- top-level corpus JSON as `owner_metric_totals`
- top-level corpus JSON as `owner_metric_totals_per_binary`

## Shape-Drift Proxies

The benchmark also carries presentation-oriented proxy metrics so semantic owner drift and
surface-shape drift are easier to separate.

Current proxy set:

- `goto_total`
- `top_level_label_total`
- `generic_local_name_sum`
- `generic_param_name_sum`
- `unknown_type_var_total`
- `ptr_offset_total`
- `index_expr_total`
- `heuristic_avg_line_length_mean`
- `heuristic_max_brace_nesting_mean`
- `synthetic_helper_call_total`

`synthetic_helper_call_total` is derived conservatively from `__fission_*(` call sites only.
These metrics are not semantic truth; they are there to highlight surfacing drift.

## Speed Metrics

- function-level:
  - `min/avg/median/p95` wall-clock timing per engine
- whole-binary:
  - `init_sec`
  - `total_decomp_sec`
  - `total_postprocess_sec`
  - `wall_clock_sec`
- optional resources:
  - `max_rss_mb`
  - `avg_rss_mb`
  - `avg_cpu_pct`
  - `max_cpu_pct`

## Release Model

The release owner is no longer `putty.exe` alone.

- `putty` remains the primary canary and keeps a larger default weight
- release requires corpus-global non-regression
- any binary-specific improvement that breaks another corpus member is rejected
- fixed canaries and dynamic degraded-watchlist rows are both reported
- x86 and x64 weighted averages are reported separately so one arch does not hide the other

## Suite Purposes

- `smoke`: fast local validation across a small Windows x86/x64 suite
- `parity`: Ghidra-reference workbench for owner-focused parity experiments on Windows x86/x64 samples
- `release`: broader advisory Windows x86/x64 corpus for promotion candidates

## Watchlists

Row fidelity is no longer intended to be `putty`-only, but the checked-in suites are still Windows-only.

- manifest `row_fidelity_targets` are treated as bootstrap hints
- baseline degraded rows are preferred when available
- lowest-similarity successful rows backfill the remaining watchlist slots
- per-binary summaries record:
  - `watchlist_source`
  - `dynamic_watchlist_rows`
  - `bootstrap_row_targets`
  - `watchlist_diagnostics.selected_because_counts`

Dynamic row selection reasons are explicit and stable:

- `bootstrap_explicit`
- `baseline_degraded`
- `baseline_low_similarity`
- `dynamic_low_similarity`

## Advisory Rollout

Corpus suites currently default to `gate_mode=advisory`.

- regressions are still computed and written to artifacts
- corpus summaries now distinguish benchmark status from promotion eligibility
- `release_promotion_allowed=false` is expected until a suite is intentionally promoted
- advisory suites record owner/shape/arch promotion blockers but still remain non-blocking

Recommended workflow:

1. local unit / invariant tests
2. smoke corpus benchmark
3. parity corpus benchmark for Windows reference-guided work
4. release corpus benchmark only for Windows promotion candidates

## Investigating Timeout Causes

If `--limit 20` results in a 900-second timeout:

```bash
# Identify the culprit function by testing each one individually
python benchmark/full_benchmark/find_timeout_culprit.py samples/windows/x64/putty.exe --limit 20 --timeout 120 --verbose
```

For the full procedure, see [`docs/debug/TIMEOUT_DEBUG_GUIDE.md`](../../../docs/debug/TIMEOUT_DEBUG_GUIDE.md).

## Current Validation Snapshot

- `test_control_flow_x64_O0.exe --limit 30`
  - Fission: `init 0.183s`, `decomp 4.470s`, `post 0.027s`, success `25/30`
  - Ghidra: `init 1.412s`, `decomp 0.170s`, success `30/30`
  - Fission synthetic failure `3`, explicit error `2`
- `putty.exe --limit 100`
  - Fission: `init 0.260s`, `decomp 157.037s`, success `50/100`
  - Ghidra: `init 1.767s`, `decomp 3.140s`, success `100/100`
  - the main bottleneck is currently closer to per-function decompilation than preparation
