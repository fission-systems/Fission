# Quality metrics and trend artifacts

**Last verified:** 2026-05-02

This note defines **stable filenames and top-level JSON fields** reviewers use when comparing `main` over time. It intentionally stays descriptive—**no nightly automation is required yet** ([`docs/RELEASE.md`](RELEASE.md) optional benchmark attachment).

## Benchmark runner outputs

Runs emitted under `benchmark/artifacts/full_benchmark/<run-name>/`.

| File | Role | Stable identifiers |
|------|------|---------------------|
| `benchmark_summary.json` | Full verbose rollup used by regression tooling | Referenced by corpus manifests / runner docs ([`benchmark/BENCHMARK_GUIDE.md`](../benchmark/BENCHMARK_GUIDE.md)) |
| `benchmark_compact_summary.json` | Operator-facing condensed summary (`compact_summary.py`) | Top-level discriminator `summary_kind`: `compact_corpus_benchmark` **or** `compact_single_benchmark`; `compact_version` integer |

### Compact corpus (`compact_corpus_benchmark`)

Authoritative schema documentation lives beside the harness (`benchmark/full_benchmark/grand_finale_support/artifact_models.py`). Trend-friendly scalars include:

- Gate posture: `weighted_avg_normalized_similarity`, `release_promotion_allowed`, `promotion_blockers`
- Architecture buckets: `x86_summary`, `x64_summary` (`binary_count`, `weighted_avg_normalized_similarity`, `failed_binary_ids`, …)
- Rolled metrics: `owner_metric_totals`, `shape_drift_totals`, `normalize_pass_metric_totals`, `ghidra_action_metric_totals`, `mir_metric_totals`, `blockgraph_region_metric_totals`, `alias_interleave_metric_totals`, `cpu_metric_totals`
- Row sampling: `top_degraded_rows`, `per_binary_rows[]` (`direct_success`, `avg_normalized_similarity`, nested metric maps matching single-binary compact rows)

Subset constants enumerating promoted keys appear in [`benchmark/full_benchmark/grand_finale_support/compact_summary.py`](../benchmark/full_benchmark/grand_finale_support/compact_summary.py) (`SELECTED_*` tuples).

### Compact single benchmark (`compact_single_benchmark`)

Shares nested metric dictionaries with corpus rows (`owner_metrics`, `shape_drift_metrics`, `normalize_pass_metrics`, …) and exposes comparison helpers (`both_success_rate_pct`, `top_regressions`, `baseline_blockers`).

## Automation lane outputs (`fission-automation`)

Artifacts upload under `benchmark/artifacts/automation/` (CI: [`reusable-nir-check.yml`](../.github/workflows/reusable-nir-check.yml)).

| File | Role |
|------|------|
| `summary.json` | Aggregate lane snapshot (`AutomationSummary`) |
| `summary.md` | Human-readable sibling |
| `decision_insights.json` | Go/stop reasoning surfaces |
| `diagnosis.json` | Bucketed diagnostics |

### `AutomationSummary` (JSON)

Defined in Rust (`crates/fission-automation/src/report/snapshot.rs`). Trend-minded fields:

| Field | Meaning |
|-------|---------|
| `generated_at`, `lane`, `run_id`, `run_profile` | Run identity |
| `target_count`, `*_elapsed_ms`, `total_elapsed_ms` | Throughput / perf proxies |
| `binaries[]` | Per-binary snapshots (`binary`, success counters, recovery maps) |
| `aggregate` | Rollup mirrors binary keys + `diagnosis_bucket_counts`, `nir_block_signature_counts` |

Each `BinarySnapshot` / `AggregateSnapshot` embeds `nir_build_stats_totals`, which **must** match the canonical [`NirBuildStats`](../crates/fission-pcode/src/nir/types.rs) JSON shape (guard tests live in `snapshot.rs`).

### Embedded `NirBuildStats`

Counters cover timing (`build_duration_ms`, `normalize_duration_ms`, …), pcode validation (`invalid_pcode_shape_count`, …), Ghidra-parity stage mirrors (`ghidra_action_*`), MIR projection counts (`mir_*`), structuring outcomes (`structuring_*`, `region_linearize_*`, …). **Extend counters only in `types.rs`**, then thread through automation aggregates—never fork a parallel telemetry struct.

## Loader identity (`BinaryIdentityReport`)

CLI `--identity` JSON includes optional Phase 2 fields: `resources`, `die_compat`, `pe`, `winapi_catalog`. Skipped DIE primitives are aggregated under `die_compat.unsupported_primitives` using stable snake_case keys aligned with [`SignatureRule`](../../crates/fission-loader/src/detector/die_engine/rules.rs) variants executed outside the Phase 2 subset (notably `ep_pattern`, `file_pattern`, `overlay_pattern`; supported kinds include `section_name`, `string_match`, `import`, `overlay_present`, `section_entropy`, `overlay_entropy`, `rich_header`, `section_count`, `section_numeric`).

## Deferred automation

Publishing weekly JSON deltas to GitHub Actions summaries remains future work; until then, attach artifact paths in PRs/releases when citing improvements ([`CONTRIBUTING.md`](../CONTRIBUTING.md)).
