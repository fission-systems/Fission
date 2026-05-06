# Benchmark Guide

This document is the top-level benchmark guide for the repository.

Current policy is intentionally split:

- Canonical decompilation-quality validation: [`benchmark/source_semantic_benchmark/README.md`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/README.md)
- Ghidra reference/comparison validation: [`benchmark/full_benchmark/README.md`](/Users/sjkim1127/Fission/benchmark/full_benchmark/README.md)
- Legacy microbenchmark / perf helpers: `cargo bench`, `scripts/benchmark/*`

The canonical validation surface for release-quality decompiler work is:

- `python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py ...`
- manifests under [`benchmark/source_semantic_benchmark/manifests/`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/manifests)
- artifacts under [`benchmark/artifacts/source_semantic_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/source_semantic_benchmark)

## Canonical Workflow

Use the source semantic benchmark for decompilation quality work. It compares
Fission output against checked-in original source code and does not use Ghidra
as the quality oracle.

Primary operator guide:

- [`benchmark/source_semantic_benchmark/README.md`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/README.md)

Current scope:

- Primary corpus binaries and source files live under [`benchmark/binary/`](/Users/sjkim1127/Fission/benchmark/binary).
- explicit source-owned smoke and full source-owned suites
- advisory-first corpus gating
- row JSON and summary JSON/Markdown as the preferred review artifacts

Recommended loop:

1. targeted trace or targeted crate validation
2. source semantic smoke benchmark
3. source-owned corpus benchmark
4. Ghidra reference/comparison benchmark only when investigating parity against local Ghidra code

## Real-World Sample Automation

CLI-first sample automation lives under [`scripts/corpus/`](/Users/sjkim1127/Fission/scripts/corpus)
and [`scripts/benchmark/`](/Users/sjkim1127/Fission/scripts/benchmark). The pipeline is intentionally
split so it can be run locally or in CI without a browser session:

```bash
# 1. Collect/hash local samples and emit a full-benchmark-compatible manifest.
python3 scripts/corpus/hash_and_manifest.py \
  --input benchmark/binary/x86-64/window/small/binary \
  --copy-to benchmark/binary/realworld/local \
  --repo-relative \
  --output benchmark/config/benchmark_corpus/realworld_local.json

# Optional: download URLs listed one per line before hashing.
python3 scripts/corpus/hash_and_manifest.py \
  --url-list /path/to/urls.txt \
  --copy-to benchmark/binary/realworld/downloaded \
  --repo-relative \
  --output benchmark/config/benchmark_corpus/realworld_downloaded.json

# Optional: collect GitHub release asset URLs and, with --download, store them
# under an ignored local corpus directory before writing a manifest.
python3 scripts/corpus/collect_github_release_samples.py \
  --source-config benchmark/config/benchmark_corpus/github_release_sources.example.json \
  --download \
  --store benchmark/binary/realworld/github \
  --output benchmark/config/benchmark_corpus/github_release_samples.json

# 2. Run loader-only smoke over the manifest.
python3 scripts/benchmark/run_loader_smoke.py \
  --manifest benchmark/config/benchmark_corpus/realworld_local.json \
  --fission-bin target/release/fission_cli \
  --output-dir benchmark/artifacts/realworld_suite/loader_smoke_latest

# 3. Orchestrate loader smoke plus optional raw p-code and full benchmark lanes.
python3 scripts/benchmark/run_realworld_suite.py \
  --build-cli \
  --loader-manifest benchmark/config/benchmark_corpus/realworld_local.json \
  --raw-pcode-manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --full-corpus-manifest benchmark/config/benchmark_corpus/realworld_local.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-release \
  --output-dir benchmark/artifacts/realworld_suite/latest

# 4. Diff any two JSON reports from loader/raw/full/suite runs.
python3 scripts/benchmark/compare_reports.py \
  --baseline benchmark/artifacts/realworld_suite/baseline/loader_smoke_report.json \
  --current benchmark/artifacts/realworld_suite/latest/loader_smoke/loader_smoke_report.json \
  --output-dir benchmark/artifacts/realworld_suite/latest/diff
```

Downloaded samples are not automatically trusted. The manifest records SHA-256,
size, source provenance, and a magic-byte format guess; loader semantics still
come from Fission parsers and typed loader failures, not benchmark-side repair.
Downloaded GitHub release binaries under `benchmark/binary/realworld/` are local
corpus artifacts and must not be staged or pushed.

## Canonical Paths

Config roots:

- [`benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json)
- [`benchmark/source_semantic_benchmark/manifests/source_owned_all.json`](/Users/sjkim1127/Fission/benchmark/source_semantic_benchmark/manifests/source_owned_all.json)
- [`benchmark/config/benchmark_corpus/smoke_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/smoke_corpus.json)
- [`benchmark/config/benchmark_corpus/release_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/release_corpus.json)
- [`benchmark/config/benchmark_corpus/parity_corpus.json`](/Users/sjkim1127/Fission/benchmark/config/benchmark_corpus/parity_corpus.json)
- [`crates/fission-automation/config/sentinel_sets.toml`](../crates/fission-automation/config/sentinel_sets.toml) (default `nir-check` manifest)
- [`benchmark/config/automation/README.md`](config/automation/README.md) (pointer / layout note)

Artifact roots:

- [`benchmark/artifacts/source_semantic_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/source_semantic_benchmark)
- [`benchmark/artifacts/full_benchmark/`](/Users/sjkim1127/Fission/benchmark/artifacts/full_benchmark)
- [`benchmark/artifacts/automation/`](/Users/sjkim1127/Fission/benchmark/artifacts/automation)

Naming contract:

- source semantic latest: `benchmark/artifacts/source_semantic_benchmark/<suite>-latest`
- full benchmark latest: `benchmark/artifacts/full_benchmark/<target>-<profile>-latest`
- full benchmark baseline: `benchmark/artifacts/full_benchmark/<target>-<profile>-baseline`
- full benchmark timestamped run: `benchmark/artifacts/full_benchmark/<target>-<profile>-<YYYYmmdd-HHMMSS>`
- automation run: `benchmark/artifacts/automation/<lane>-<run-profile>-<unix_run_id>`
- automation latest: `benchmark/artifacts/automation/latest/<lane>/`

## Corpus Semantics

The canonical source semantic suites are source-owned. Every source-defined
function produces a row; mapping, decompilation, candidate compilation, and
behavior failures are recorded as failures rather than skipped cases.

- `smoke`: deterministic source-owned C fixture validation
- `source-owned-all`: auto-discovered checked-in source/binary pairs
- Ghidra parity/release manifests remain reference/comparison inputs, not the primary oracle

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

The preferred machine-readable artifacts are:

- `source_semantic_rows.json`
- `source_semantic_summary.json`

Use them for first-pass review, AI tooling, and advisory summarization. Keep the
Markdown artifact for operator-facing triage.

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
