## Summary

<!-- What does this PR change and why? Link issues (e.g. `Closes #123`). -->

## Change Type

- [ ] Decompiler semantics / IR / structuring / types
- [ ] SLEIGH lifting
- [ ] Loader / binary format support
- [ ] Static facts / native preparation
- [ ] Decompilation orchestration / Rust-Sleigh
- [ ] Verification (`fission-dir`) / solver
- [ ] Emulator / dynamic analysis / TTD
- [ ] CLI / JSON contract
- [ ] Benchmark / measurement scripts
- [ ] Documentation only
- [ ] Build / CI / release infrastructure

## Semantic Ownership

<!-- Check every area you materially changed; reviewers use this for routing. -->

- [ ] **NIR / structuring / pcode** (`crates/fission-pcode/`, `crates/fission-midend-*`; `AGENTS.md` "NIR structuring")
- [ ] **SLEIGH** (`crates/fission-sleigh/`, `crates/fission-cspec/`)
- [ ] **Static facts / native prep** (`crates/fission-static/`)
- [ ] **Decompilation orchestration / Rust-Sleigh** (`crates/fission-decompiler/`)
- [ ] **Loader / binary parsing / metadata** (`crates/fission-loader/`, `crates/fission-analysis-db/`, `crates/fission-signatures/`)
- [ ] **Verification / solver** (`crates/fission-dir/`, `crates/fission-solver/`)
- [ ] **Emulator / dynamic / TTD** (`crates/fission-emulator/`, `crates/fission-dynamic/`, `crates/fission-ttd/`)
- [ ] **CLI / JSON contract** (`crates/fission-cli/`)
- [ ] **Benchmark scripts** (`benchmark/config/`, `scripts/quality/decbench_*`; see [`docs/BENCHMARK_DECBENCH.md`](../docs/BENCHMARK_DECBENCH.md))
- [ ] **Docs / process only** (no semantic code paths)
- [ ] **CI / release plumbing** (`.github/`, `docs/RELEASE.md`)

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity` (CI's exact line)
- [ ] `cargo nextest run -p <crates touched>`
- [ ] Each new test fails with the fix disabled (positive control)
- [ ] DecBench measurement, if decompiler output can change (see below)
- [ ] Not run — reason:

<details>
<summary>Commands / key output</summary>

```bash

```

</details>

## Benchmark Impact

<!--
Required when decompiler output can change. Quality claims cite the DecBench
full-dataset sweep (`~/fission-benchmark/decbench-full/sweep.py`); the
250-function sample-set gate is the quick pre-check. Run with caches disabled
(`FISSION_BENCHMARK_NO_CACHE=1`) and compare output hashes before scores
(`sweep.py diff`). Report total distance next to the perfect count: the count
alone has ranked a regression as a win. byte_match is not run locally, so
Union here is GED or type. Use `n/a` for changes that cannot change output.
-->

```text
sample-set gate:    GED perfect <before> -> <after>, total distance <before> -> <after>

full-dataset sweep, per config (<config>):
  GED perfect:        <before> -> <after>
  GED total distance: <before> -> <after>
  Types perfect/mean: <before> -> <after>
  Union (GED, type):  <before> -> <after>
  failures:           <before> -> <after>
  sweep.py diff:      <functions whose output changed>
```

## Telemetry and Reporting

- [ ] Changes to decompiler semantics or inventory output: **confirmed** [`NirBuildStats`](../crates/fission-midend-core/src/ir/build_stats.rs) consumers stay aligned, or this PR updates them together with tests.
- [ ] No parallel ad-hoc telemetry JSON outside the established contracts ([`docs/QUALITY_METRICS.md`](../docs/QUALITY_METRICS.md)).

## ADR / Design Traceability

- [ ] Architectural trade-offs are covered by an existing [`docs/adr/`](../docs/adr/) decision, **or** this PR adds/updates an ADR.
- ADR link(s): <!-- e.g. `docs/adr/0001-cli-first-external-surface.md` -->

## Risk Notes

> [!IMPORTANT]
> If this changes semantic recovery, structuring, loader classification, or benchmark gates, include the exact reproduction command and artifact path.

## Checklist

- [ ] Documentation updated or not needed
- [ ] New/changed behavior is covered by tests, benchmark rows, or an explicit reason
- [ ] Generated artifacts, local corpora, and downloaded samples are not committed
