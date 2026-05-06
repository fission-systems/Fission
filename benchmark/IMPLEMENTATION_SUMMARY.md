# Benchmark Implementation Summary

This file summarizes the current benchmark/reporting ownership after the Windows
corpus migration.

## Canonical Ownership

Canonical benchmark runner and reporting:

- [`benchmark/source_semantic_benchmark/`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark)

Reference/comparison benchmark runner and reporting:

- [`benchmark/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/full_benchmark)

Config ownership:

- [`benchmark/config/benchmark_corpus/`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus)
- [`benchmark/config/automation/`](/Users/sjkim1127/Fission/benchmark/config/automation) (README only; sentinel manifest SSOT is [`crates/fission-automation/config/`](../crates/fission-automation/config))

Artifact ownership:

- [`benchmark/artifacts/source_semantic_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/source_semantic_benchmark)
- [`benchmark/artifacts/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)
- [`benchmark/artifacts/automation/`](/Users/sjkim1127/Fission/benchmark/artifacts/automation)

## Current Validation Surface

Canonical benchmark entrypoint:

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py ...
```

Current validation policy:

- checked-in source-owned corpus entries under `benchmark/binary`
- source-vs-Fission semantic rows are the canonical release-quality validation surface
- Ghidra benchmark output is a reference/comparison lane, not the primary oracle
- advisory-first rollout
- source semantic rows and summary JSON are the preferred AI-facing artifacts

## Artifact Contracts

Full benchmark output naming:

- `benchmark/artifacts/source_semantic_benchmark/<suite>-latest`
- `benchmark/artifacts/full_benchmark/<target>-<profile>-latest`
- `benchmark/artifacts/full_benchmark/<target>-<profile>-baseline`
- `benchmark/artifacts/full_benchmark/<target>-<profile>-<YYYYmmdd-HHMMSS>`

Automation output naming:

- `benchmark/artifacts/automation/<lane>-<run-profile>-<unix_run_id>`
- `benchmark/artifacts/automation/latest/<lane>/`

The canonical source semantic outputs are:

- `source_semantic_rows.json`
- `source_semantic_summary.json`
- `source_semantic_summary.md`

## Corpus Suites

Checked-in manifests:

- [`benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json)
- [`benchmark/source_semantic_benchmark/manifests/source_owned_all.json`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/manifests/source_owned_all.json)
- [`benchmark/config/benchmark_corpus/smoke_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/smoke_corpus.json)
- [`benchmark/config/benchmark_corpus/release_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/release_corpus.json)
- [`benchmark/config/benchmark_corpus/parity_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/parity_corpus.json)

Top-level manifest metadata is explicit:

- `name`
- `suite_tier`
- `gate_mode`
- `dynamic_watchlist_limit`
- optional `notes`

Source semantic per-entry schema remains compact:

- `id`
- `binary_path`
- `source_path`
- `language`
- `tags`
- `weight`

## Reporting Model

The benchmark summary now separates:

- semantic owner drift
- shape-drift proxies
- x86/x64 split reporting
- watchlist selection reasons
- promotion blockers

First-pass review should use:

- `source_semantic_summary.json`

Deep debugging should use:

- row JSON and Markdown artifacts

## CI Alignment

Fast gate:

- [`/.github/workflows/ci.yml`](/Users/sjkim1127/Fission/.github/workflows/ci.yml)
- build / lint / test only

Heavy validation:

- [`/.github/workflows/ci-heavy.yml`](/Users/sjkim1127/Fission/.github/workflows/ci-heavy.yml)
- corpus validation
- `nir-check`
- canonical source semantic benchmark
- advisory benchmark reporting

## Non-Canonical Perf Helpers

Criterion / `scripts/benchmark/*` still exist, but they are no longer the
canonical quality surface. They should be treated as microbenchmark and
performance-history helpers only.
