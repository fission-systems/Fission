# Benchmark Implementation Summary

This file summarizes the current benchmark/reporting ownership after the Windows
corpus migration.

## Canonical Ownership

Benchmark runner and reporting:

- [`benchmark/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/full_benchmark)

Config ownership:

- [`benchmark/config/benchmark_corpus/`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus)
- [`benchmark/config/automation/`](/Users/sjkim1127/Fission/benchmark/config/automation)

Artifact ownership:

- [`benchmark/artifacts/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)
- [`benchmark/artifacts/automation/`](/Users/sjkim1127/Fission/benchmark/artifacts/automation)

## Current Validation Surface

Canonical benchmark entrypoint:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py ...
```

Current validation policy:

- Windows-only corpus entries under `samples/windows/x86` and `samples/windows/x64`
- corpus suites are the canonical parity/release validation surface
- advisory-first rollout
- compact summary JSON is the preferred AI-facing artifact

## Artifact Contracts

Full benchmark output naming:

- `benchmark/artifacts/full_benchmark/<target>-<profile>-latest`
- `benchmark/artifacts/full_benchmark/<target>-<profile>-baseline`
- `benchmark/artifacts/full_benchmark/<target>-<profile>-<YYYYmmdd-HHMMSS>`

Automation output naming:

- `benchmark/artifacts/automation/<lane>-<run-profile>-<unix_run_id>`
- `benchmark/artifacts/automation/latest/<lane>/`

The canonical corpus outputs are:

- `benchmark_summary.json`
- `benchmark_summary.md`
- `benchmark_compact_summary.json`
- optional `benchmark_delta_vs_previous.json/.md`
- optional `benchmark_regression_gate.json/.md`

## Corpus Suites

Checked-in manifests:

- [`benchmark/config/benchmark_corpus/smoke_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/smoke_corpus.json)
- [`benchmark/config/benchmark_corpus/release_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/release_corpus.json)
- [`benchmark/config/benchmark_corpus/parity_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/parity_corpus.json)

Top-level manifest metadata is explicit:

- `name`
- `suite_tier`
- `gate_mode`
- `dynamic_watchlist_limit`
- optional `notes`

Per-entry schema remains compact:

- `id`
- `binary_path`
- `ghidra_project_key`
- `tags`
- `seed_limit`
- `role`
- `weight`
- optional `row_fidelity_targets`

## Reporting Model

The benchmark summary now separates:

- semantic owner drift
- shape-drift proxies
- x86/x64 split reporting
- watchlist selection reasons
- promotion blockers

First-pass review should use:

- compact summary JSON

Deep debugging should use:

- verbose JSON/Markdown artifacts

## CI Alignment

Fast gate:

- [`/.github/workflows/ci.yml`](/Users/sjkim1127/Fission/.github/workflows/ci.yml)
- build / lint / test only

Heavy validation:

- [`/.github/workflows/ci-heavy.yml`](/Users/sjkim1127/Fission/.github/workflows/ci-heavy.yml)
- corpus validation
- `nir-check`
- canonical full benchmark
- advisory benchmark reporting

## Non-Canonical Perf Helpers

Criterion / `scripts/benchmark/*` still exist, but they are no longer the
canonical quality surface. They should be treated as microbenchmark and
performance-history helpers only.
