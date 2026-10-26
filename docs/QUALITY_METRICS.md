# Quality metrics and trend artifacts

**Last verified:** 2026-05-02

This note defines **stable filenames and top-level JSON fields** reviewers use when comparing `main` over time. It intentionally stays descriptive—**no nightly automation is required yet** ([`docs/RELEASE.md`](RELEASE.md) optional benchmark attachment).

## Benchmark results

Quality is measured with DecBench; see [`BENCHMARK_DECBENCH.md`](BENCHMARK_DECBENCH.md). Cite the local gate's `--dump` JSON (one row per function: GED, source and output node counts) or full-sweep before/after totals. The former `benchmark/full_benchmark` runner and its `benchmark_summary.json` / `benchmark_compact_summary.json` outputs are retired.

## `NirBuildStats`

Counters cover timing (`build_duration_ms`, `normalize_duration_ms`, …), pcode validation (`invalid_pcode_shape_count`, …), Ghidra-parity stage mirrors (`ghidra_action_*`), BlockGraph collapse admission (`blockgraph_collapse_*`), structuring outcomes (`structuring_*`, `region_linearize_*`, …). **Extend counters only in `nir/ir/build_stats.rs`**—never fork a parallel telemetry struct.

## Loader identity (`BinaryIdentityReport`)

CLI `--identity` JSON includes optional Phase 2 fields: `resources`, `die_compat`, `pe`, `winapi_catalog`. Skipped DIE primitives are aggregated under `die_compat.unsupported_primitives` using stable snake_case keys aligned with [`SignatureRule`](../crates/fission-loader/src/detector/die_engine/rules.rs) variants executed outside the Phase 2 subset (notably `ep_pattern`, `file_pattern`, `overlay_pattern`; supported kinds include `section_name`, `string_match`, `import`, `overlay_present`, `section_entropy`, `overlay_entropy`, `rich_header`, `section_count`, `section_numeric`).

## Deferred automation

Publishing weekly JSON deltas to GitHub Actions summaries remains future work; until then, attach artifact paths in PRs/releases when citing improvements ([`CONTRIBUTING.md`](../CONTRIBUTING.md)).
