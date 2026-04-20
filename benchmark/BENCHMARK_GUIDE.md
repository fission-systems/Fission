# Benchmark Guide

This document is the top-level benchmark guide for the repository.

Current policy is intentionally split:

- Canonical decompilation-quality validation: [`benchmark/full_benchmark/README.md`](/Users/sjkim1127/Fission/benchmark/full_benchmark/README.md)
- Legacy microbenchmark / perf helpers: `cargo bench`, `scripts/benchmark/*`

The canonical validation surface for Ghidra-parity and release-quality work is:

- `python3 benchmark/full_benchmark/full_decomp_benchmark.py ...`
- manifests under [`benchmark/config/benchmark_corpus/`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus)
- artifacts under [`benchmark/artifacts/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)

## Canonical Workflow

Use the whole-binary corpus benchmark for decompilation quality work.

Primary operator guide:

- [`benchmark/full_benchmark/README.md`](/Users/sjkim1127/Fission/benchmark/full_benchmark/README.md)

Current scope:

- Windows-only corpus entries under [`samples/windows/`](/Users/sjkim1127/Fission/samples/windows)
- explicit `smoke`, `release`, and `parity` suites
- advisory-first corpus gating
- compact summary JSON as the preferred AI-facing artifact

Recommended loop:

1. targeted trace or targeted crate validation
2. smoke corpus benchmark
3. parity corpus benchmark for Ghidra-reference work
4. release corpus benchmark only for promotion candidates

## Canonical Paths

Config roots:

- [`benchmark/config/benchmark_corpus/smoke_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/smoke_corpus.json)
- [`benchmark/config/benchmark_corpus/release_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/release_corpus.json)
- [`benchmark/config/benchmark_corpus/parity_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/parity_corpus.json)
- [`benchmark/config/automation/sentinel_sets.toml`](/Users/sjkim1127/Fission/benchmark/config/automation/sentinel_sets.toml)

Artifact roots:

- [`benchmark/artifacts/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)
- [`benchmark/artifacts/automation/`](/Users/sjkim1127/Fission/benchmark/artifacts/automation)

Naming contract:

- full benchmark latest: `benchmark/artifacts/full_benchmark/<target>-<profile>-latest`
- full benchmark baseline: `benchmark/artifacts/full_benchmark/<target>-<profile>-baseline`
- full benchmark timestamped run: `benchmark/artifacts/full_benchmark/<target>-<profile>-<YYYYmmdd-HHMMSS>`
- automation run: `benchmark/artifacts/automation/<lane>-<run-profile>-<unix_run_id>`
- automation latest: `benchmark/artifacts/automation/latest/<lane>/`

## Corpus Semantics

The checked-in suites are intentionally Windows-only for this phase.

- `smoke`: fast local validation across a small x86/x64 suite
- `parity`: Ghidra-reference workbench for parity owners
- `release`: broader advisory suite for promotion candidates

`putty` remains the primary canary, but it is not the only release narrative.
Cross-binary degraded rows, x86/x64 split reporting, owner metrics, and shape-drift
proxies are part of the canonical corpus summary.

## Advisory vs Promotion

Current rollout stays advisory-first.

- `gate_mode=advisory` still computes regressions and promotion blockers
- advisory suites do not fail solely because a corpus regression exists
- `release_promotion_allowed=false` is expected until a suite is intentionally promoted

When a same-axis baseline is present, summaries should expose:

- `benchmark status`
- `gate mode`
- `release promotion eligibility`

## Compact Summary

The preferred machine-readable artifact is:

- `benchmark_compact_summary.json`

Use it for first-pass review, AI tooling, and advisory summarization. Keep the
verbose JSON/Markdown artifacts for deep debugging only.

## Legacy Microbenchmarks

Criterion / perf-benchmark helpers remain in the repository, but they are no
longer the canonical decompilation-quality surface.

Still valid for targeted throughput/perf work:

- `cargo bench ...`
- `scripts/benchmark/analyze_benchmark.py`
- `scripts/benchmark/update_history.py`
- `scripts/benchmark/setup.sh`

Use those only when the question is microbenchmark throughput or performance
history. Do not treat them as the release-quality oracle for Ghidra-parity work.
