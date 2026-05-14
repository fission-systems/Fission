# Source Semantic Benchmark

This benchmark compares Fission output against checked-in original source code.
It does not run Ghidra and does not consume Ghidra output as a baseline.

## Contract

- Source files are the oracle.
- Every source-defined function produces a row.
- Mapping, decompilation, candidate compilation, and behavior failures are rows,
  not skipped cases.
- Dynamic behavior is required for scoring, but unsupported signatures fail
  closed as `unsupported_signature`.

## Smoke Run

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --manifest benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json \
  --fission-bin target/release/fission_cli \
  --timeout-sec 20 \
  --output-dir benchmark/artifacts/source_semantic_benchmark/smoke-latest
```

Use `manifests/source_owned_all.json` for the full checked-in source-owned
corpus.

For AArch64 control-flow/NIR iteration without discovery expanding the corpus,
use the focused sample manifest:

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --manifest benchmark/source_semantic_benchmark/manifests/aarch64_control_flow_small_c.json \
  --fission-bin target/release/fission_cli \
  --timeout-sec 45 \
  --jobs 1 \
  --no-decomp-cache \
  --no-list-cache \
  --include-debug-decomp \
  --output-dir benchmark/artifacts/source_semantic_benchmark/aarch64-control-flow-small-c-latest
```

For faster local iteration, run independent source-function rows in parallel
within each binary entry. The default is half of the detected CPU count; pass
`--jobs 1` for fully serial execution.

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --manifest benchmark/source_semantic_benchmark/manifests/source_owned_all.json \
  --fission-bin target/release/fission_cli \
  --timeout-sec 45 \
  --jobs 4 \
  --output-dir benchmark/artifacts/source_semantic_benchmark/source-owned-jobs4
```

Generated artifacts:

- `source_semantic_rows.json`
- `source_semantic_summary.json`
- `source_semantic_summary.md`
- `source_semantic_comparison.json` when a prior matching artifact is found
- `behavior/<entry>/<function-address>/oracle.c`, `candidate.c`, and
  `result.json` for non-passing dynamic behavior rows
- `debug_triage/...` captures when `--materialize-debug-triage` is used

If `--output-dir` is omitted, the runner writes to a timestamped artifact
directory under `benchmark/artifacts/source_semantic_benchmark/` instead of
overwriting a `latest` directory. Each run also appends a compact record to
`benchmark/artifacts/source_semantic_benchmark/source_semantic_history.jsonl`
so score, behavior, compile, cache, wall-time, and baseline-delta trends survive
across runs. The current summary includes the latest same-manifest history
record, prefers a same-row-count record for weighted-similarity delta, and
shows both artifact-to-artifact comparison and rolling history context. The
rolling history block reports both the latest same-shape run and the latest run
overall, so a small smoke run does not overwrite the comparable trend for a
larger corpus.
runner also updates
`benchmark/artifacts/source_semantic_benchmark/source_semantic_latest_by_manifest.json`
with the latest artifact path, score, cache file, and comparison outcome per
manifest.

## Metrics

- `function_mapping_rate`: source functions mapped to a Fission function address.
- `decomp_success_rate`: mapped functions with a successful Fission decompile.
- `candidate_compile_rate`: behavior harnesses whose Fission candidate compiled.
- `behavior_pass_rate`: behavior cases that matched the source oracle.
- `host_execution_unavailable_count`: supported behavior rows that could not run
  because the local host failed the compiled-C execution preflight.
- `weighted_semantic_similarity`: `0.65 * behavior + 0.35 * static_similarity`.
- `weighted_semantic_similarity_percent`: the same weighted score expressed as
  a percentage for report display. Per-row `semantic_score_percent` and
  `static_semantic_score_percent` are emitted alongside the raw 0..1 scores.
- `scoring_contract`: machine-readable statement of the scoring denominator
  and component policies. The semantic score denominator is all manifest rows;
  static similarity is a multiset Jaccard over the source/decompiler union, so
  missing source features and extra decompiler features both affect the score.
- `score_denominator_metrics` and `semantic_loss_metrics`: explicit score-sum
  accounting over the full row denominator, including zero/nonzero/perfect row
  counts, lost-score totals, and lost-score attribution by behavior status,
  first failing stage, and zero-credit reason.
- `score_component_metrics`: behavior/static score sums, weighted contribution
  sums, lost contribution sums, and per-component score distributions. This
  makes it clear whether a run is losing credit in dynamic behavior, static
  shape, or both.
- `effective_coverage`: mapped, decompiled, behavior-expected, and
  behavior-executed row counts/rates over the full manifest denominator.
- `behavior_eligibility`: behavior eligibility/execution/pass rates with both
  eligible-row and total-row denominators, so unsupported or absent behavior
  cases cannot silently inflate the pass rate.
- `behavior_denominator_metrics`: row and case denominators for dynamic checks,
  including eligible/executed/pass rates under total, eligible, and executed
  denominators. `behavior_case_metrics.compared_case_count` counts missing or
  extra output lines as failed cases instead of dropping them.
- `zero_credit_breakdown`: reason buckets for rows whose weighted semantic score
  is exactly zero (`unmapped`, `decomp:*`, `behavior:*`, `static_zero`, ...).
- `stage_first_failure_counts`: first non-`ok` debug stage per mapped row when
  debug evidence is available.
- `static_similarity_component_averages` and
  `static_similarity_component_average_percent`: static similarity split into
  control-flow, operator, call, constant, memory, and signature token families.
- `static_similarity_gap_totals`: coverage-aware feature accounting for the
  same static comparison, including source/decompiler feature totals,
  intersection/union totals, missing feature count/rate, extra feature
  count/rate, and top missing/extra features. Missing source features are
  included in the denominator, so absent semantics are penalized.
- `static_similarity_gap_component_totals`: the same missing/extra accounting
  split by static feature family.
- `static_absence_penalty_metrics`: source recall, decompiler precision, union
  Jaccard, missing/extra totals, and rows where source features exist but the
  decompiler emitted no comparable features. This is the top-level proof that
  absence is included rather than ignored.
- `source_decomp_size_metrics`: source-body and decompiler output line/byte
  distributions, decompiler/source size ratios, and the rows with the largest
  decompiler/source line ratio. This helps separate semantic failures from
  excessively expanded or unexpectedly empty output.
- `behavior_case_metrics`: dynamic behavior at case granularity, including
  total/pass/fail case counts, case pass rate, and rows where at least one case
  passed despite an overall mismatch.
- `behavior_support_metrics`: behavior harness case-source counts
  (`explicit` vs `default`), unsupported-signature row counts, and unsupported
  reason buckets, so untested functions are visible instead of blending into
  ordinary behavior failures.
- `behavior_mismatch_metrics`: mismatch-row diagnostics including first failing
  case index buckets, output-length deltas, and mismatch kind counts.
- `behavior_distance_metrics`: case pass-rate distribution plus missing/extra
  candidate output-line totals, so partial dynamic mismatches are visible even
  when the row-level behavior score is still fail-closed.
- `score_distribution`: row counts in `zero`, `low`, `medium`, `high`, and
  `perfect` semantic-score buckets.
- `semantic_score_stats`: min/max/average and p50/p90/p95 score distribution
  over the full manifest denominator, plus nonzero row count/rate.
- `denominator_accounting_metrics`: explicit row accounting for mapped,
  unmapped, decompiled, behavior expected/executed, behavior non-pass, static
  missing-feature, zero, nonzero, and perfect-score rows. This keeps absent or
  unsupported rows visible instead of letting them disappear from the
  denominator.
- `score_by_behavior_status`, `score_by_stage_first_failure`,
  `behavior_status_by_stage_first_failure`, and
  `behavior_status_by_zero_credit_reason`: cross-tabs that connect score loss to
  dynamic behavior status, first failing debug stage, and zero-credit reason.
- `static_gap_row_metrics`: row-level static-gap accounting, including rows
  with missing/extra features, rows with zero source/decompiler feature
  intersection, per-component missing-row counts, and missing/extra feature
  count distributions.
- `source_feature_metrics`: source/decompiler/intersection/union feature-count
  distributions, plus per-component source/decompiler feature-count
  distributions. This exposes whether low similarity is caused by absent output,
  excessive output, or high-complexity source rows.
- `by_arch`, `by_source_return_kind`, and `by_source_param_shape`: quality
  buckets split by inferred binary architecture and source signature shape.
  These are additive to `by_language`, `by_tag`, and `by_entry`.
- `debug_coverage_metrics` and `debug_stage_status_matrix`: debug evidence
  coverage and per-stage status counts for rows where debug decomp evidence was
  requested.
- `pipeline_stage_metrics`: per-stage OK/non-OK/missing counts and OK rates for
  load, decode, raw p-code, NIR build, normalize, structuring, and render.
- `debug_pipeline_numeric_metrics`: distributions for numeric Rust-SLEIGH
  pipeline evidence such as decode attempts and raw p-code block/op/edge counts.
- `nir_build_stats_metrics`: flattened `PreviewBuildStats`/`NirBuildStats`
  numeric totals, nonzero row counts, debt-metric distributions, and highest
  debt rows. This keeps NIR telemetry aligned with the canonical stats payload
  while making benchmark triage possible from the summary alone.
- `nir_debt_correlation_metrics`: rows with nonzero NIR debt metrics,
  behavior-status buckets for those rows, and score distributions for rows with
  and without NIR debt. This is a coarse admission check for whether telemetry
  debt is actually correlated with benchmark quality loss.
- `debug_quality_evidence_nonzero_rows`: row counts for nonzero NIR/debug
  evidence counters, complementing `debug_quality_evidence_totals`.
- `triage_priority_rows`: compact low-score row shortlist with behavior status,
  first failing stage, feature gaps, and artifact paths for follow-up.
- `improvement_axis_metrics`: full-denominator rows grouped by likely owner
  axis (`mapping`, `sleigh_decode_lift`, `nir_build_normalize`,
  `structuring_render`, `behavior_harness`, `dynamic_semantics`,
  `static_semantic_gaps`, and related buckets), including lost-score totals,
  behavior/stage cross-tabs, missing-feature totals, and representative rows.
- `focus_area_metrics`: multi-label roadmap-oriented buckets for
  SLEIGH/runtime lift, NIR builder/dataflow, type/data abstraction,
  structuring/render, mapping/name recovery, behavior harness coverage,
  dynamic semantics, and unclassified quality loss. Row counts can exceed the
  manifest denominator because a row may expose more than one focus area.
- `admission_gate_metrics`: full-denominator funnel counts/rates for mapping,
  decompile, Rust-SLEIGH stages, candidate compile, behavior pass, static
  perfect rows, and fully perfect semantic rows.
- `stage_transition_metrics`: debug-stage transition evidence, including
  furthest OK stage counts and lost-score attribution by first stage blocker.
- `sleigh_lift_health_metrics`: decode/raw-p-code OK rates over mapped rows,
  template-source totals, raw p-code compatibility import totals, invalid p-code
  shape totals, and SLEIGH first-blocker rows. This makes SLEIGH regressions
  visible even when downstream behavior or static scores also fail.
  `--require-sleigh-template-source` also fails if compatibility imports or
  invalid p-code shapes are nonzero.
- `behavior_failure_diagnostics`: behavior failure owner buckets
  (`candidate`, `oracle`, harness unavailable, unsupported, mismatch) plus
  normalized compiler/runtime detail signatures and representative rows.
- `semantic_quality_quadrant_metrics`: row buckets combining dynamic behavior
  state (`dynamic_pass`, `dynamic_mismatch`, unsupported/blocked states) with
  static feature state (`static_perfect`, `static_gap`,
  `static_no_decomp_features`, ...). This separates “behavior correct but shape
  poor” from “shape plausible but behavior wrong.”
- `coverage_blind_spot_metrics`: explicit row counts and representative rows
  for missing evidence surfaces such as unmapped source functions, missing debug
  decomp evidence, unsupported behavior signatures, eligible-but-not-executed
  behavior rows, and source features with no comparable decompiler features.
- `static_gap_density_metrics`: missing/extra feature density distributions and
  feature-gap buckets, so source absence and decompiler excess are visible even
  when raw feature totals differ by function size.
- `complexity_quality_metrics`: source static-feature complexity buckets plus
  hard non-perfect rows, so large functions and dense semantic shapes can be
  separated from small-row failures.
- `stage_cost_correlation_metrics`: decompile wall-time distributions grouped
  by behavior status, first failing debug stage, score bucket, and decompile
  cost bucket, linking quality blockers to runtime cost.
- `harness_cost_metrics`: decompile, behavior compile, behavior run, and
  behavior wall-time totals/averages and p50/p90/p95/max timings, plus behavior
  cache status aggregation.
- `cache_efficiency_metrics`: request counts and hit rates for list, decompile,
  and behavior caches.
- `cost_hot_rows`: slowest rows by decompile wall time and behavior wall time,
  preserving row identity so benchmark runtime improvements can target the
  responsible function instead of only the aggregate timer.

The JSON and Markdown summaries also include mapping, decompile-failure, and
behavior-status buckets plus language/tag/entry breakdowns. `--jobs` changes
only execution scheduling; row order is restored before artifacts are written.
If `orjson` is installed it is used as an optional JSON read/write fast path;
otherwise the standard library `json` module is used.

Fission decompile results are cached in
`benchmark/artifacts/source_semantic_benchmark/.cache/decomp_cache.json` by
default. Cache keys include the input binary path/stat, `fission_cli` path/stat,
function address, whether `--include-debug-decomp` is enabled, and the
debug-evidence contract required by the source-semantic runner, so rebuilding
`fission_cli` or changing debug evidence invalidates old decompile rows
automatically. Use
`--decomp-cache-file <path>` to pin a different cache file or
`--no-decomp-cache` to disable the persistent cache; repeated decompile requests
inside the same process can still reuse the in-memory cache. Each row includes
`decomp_cache_status` (`hit`, `miss`, `refreshed_debug_bundle`, or
`not_requested`) and the summary/Markdown report aggregates those statuses, so
throughput changes are visible separately from semantic-quality changes.

By default, each run looks for the latest previous artifact under
`benchmark/artifacts/source_semantic_benchmark/` with the same manifest name and
the same row-key set, then adds a `comparison` block to the summary. This avoids
calling a smaller smoke run an improvement over a larger corpus run. Use
`--baseline-dir <artifact-dir>` to pin a specific previous run or
`--no-baseline-compare` to disable this. The comparison reports metric deltas,
improved/regressed rows, behavior status transitions, top per-function score
changes, separated top improvements/top regressions, and a `comparison_outcome`
headline that states whether the run improved, regressed, stayed unchanged, or
is mixed versus the baseline. Explicit baseline comparisons with new or missing
rows are marked `mixed`.

For failure triage, `--include-debug-decomp` forwards `--debug-decomp` to
`fission_cli decomp` and stores compact stage status, owner buckets, and selected
quality evidence in each row. This is observation-only and does not affect
scoring, but it makes low-score rows easier to route back to SLEIGH, NIR,
structuring, or type/data owners. The JSON and Markdown summaries aggregate
debug owner buckets and quality-evidence totals when debug evidence is present.
Rows with a mapped function also include a ready-to-run `debug_decomp_command`
and, when `--include-debug-decomp` is used, the runner materializes the same CLI
bundle at `debug_decomp/<entry>/<function-address>.json`. The Markdown summary
lists the lowest-scoring repro commands first. Those rows also include
`disasm --function`, `xrefs --function`, and `inventory function-facts`
commands for the same binary/address so a semantic regression can be routed into
the existing CLI debugging surfaces without rerunning the full benchmark. The
older `inventory preview-candidates` native decomp surface is not materialized
because current `fission_cli` reports it as deprecated after native decomp
removal.

Use `--materialize-debug-triage` to execute the existing CLI debugging surfaces
for the lowest-scoring non-perfect rows during the benchmark run. The runner
saves `fission_cli decomp --debug-decomp-bundle`, `disasm --function`, `xrefs
--function`, and `inventory function-facts` command results under
`debug_triage/`; function facts also get JSONL and summary files. The runner
adds a `debug_triage` block to the JSON and Markdown summaries. Keep this off
for throughput runs; enable it for diagnosis-focused artifact snapshots.

Use `--materialize-regression-debug-triage` when comparing against a previous
artifact and the question is specifically "what got worse?". The runner takes
the comparison's top regressed rows, then materializes the same existing
`fission_cli decomp`, `disasm`, `xrefs`, and `inventory function-facts` captures
for those rows and adds a `regression_debug_triage` block to the JSON and
Markdown summaries. This turns previous-artifact comparison into a ready-to-open
CLI debug bundle without changing the semantic score.

For dynamic behavior failures, each non-passing row also records a
`behavior.artifact_dir` with the exact generated oracle harness, candidate
harness, and compile/run result JSON. These files make timeouts, compile
failures, and mismatches reproducible without re-running the full benchmark.

The static comparison uses language-neutral fingerprints for control-flow,
operators, constants, calls, memory access shape, and signature shape. Dynamic
behavior currently compiles C-like decompiler output with a compatibility
header and runs deterministic cases for supported scalar integral signatures,
plus manifest-owned `behavior_cases` for C `int *` arguments and `void`
side-effect functions. Explicit cases can list observed globals so a function
that communicates through a checked-in global sink is compared by effect, not
only by return value. If the local host cannot execute a freshly compiled C
probe, supported dynamic rows fail closed as `host_execution_unavailable`
instead of being silently skipped.
