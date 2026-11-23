# Quality metrics and trend artifacts

## DecBench sweep reports

The [nightly DecBench sweep](CI_RELEASE_GATES.md#nightly-decbench) reports row
coverage, status, output hashes, GED perfect count and total distance, and
type-match coverage and scores. Link the run and its artifacts when reporting
a regression. Cite cache-disabled before/after full sweeps on matching corpus
and scorer revisions for quality claims. The
[benchmark guide](BENCHMARK_DECBENCH.md) explains how to compare results and
when a local sample-set check is useful.

## Decompiler telemetry

[`NirBuildStats`](https://github.com/fission-systems/Fission/blob/main/crates/fission-midend-core/src/ir/build_stats.rs)
owns decompiler counters for validation, normalization, structuring, and timing.
Use [`nir-stats`](CLI.md#command-model) for a single function and
[`inventory function-facts`](CLI.md#inventory-function-facts) for a batch.
Extend the canonical type when adding counters.

## Loader identity

The optional identity report from `fission_cli info <binary> --identity --json`
includes resource provenance and detector compatibility. When available,
`die_compat.unsupported_primitives` counts skipped rule types by stable
snake-case key. See the [binary metadata reference](CLI.md#info-command).
