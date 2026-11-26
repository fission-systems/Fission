# Quality metrics and trend artifacts

## FreshBench sweep reports

`freshbench sweep report` gives scores, failures and timing for a run, split
by source size band: GED distance and perfect count, types, recompilation and
call sites. `sweep diff` compares output hashes before scores. Link the run
directory (`runs/<label>/`) when reporting a regression. Cite before/after
full M1 runs with the same corpus id and scorer for quality claims. The
[benchmark guide](BENCHMARK_FRESHBENCH.md) explains how to compare results and
when a `--builds` / `--only` slice is useful. CI's scheduled sweeps are
described in [CI and release gates](CI_RELEASE_GATES.md).

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
