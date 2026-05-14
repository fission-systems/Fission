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
- `effective_coverage`: mapped, decompiled, behavior-expected, and
  behavior-executed row counts/rates over the full manifest denominator.
- `behavior_eligibility`: behavior eligibility/execution/pass rates with both
  eligible-row and total-row denominators, so unsupported or absent behavior
  cases cannot silently inflate the pass rate.
- `zero_credit_breakdown`: reason buckets for rows whose weighted semantic score
  is exactly zero (`unmapped`, `decomp:*`, `behavior:*`, `static_zero`, ...).
- `stage_first_failure_counts`: first non-`ok` debug stage per mapped row when
  debug evidence is available.
- `static_similarity_component_averages` and
  `static_similarity_component_average_percent`: static similarity split into
  control-flow, operator, call, constant, memory, and signature token families.
- `harness_cost_metrics`: decompile, behavior compile, behavior run, and
  behavior wall-time totals/averages, plus behavior cache status aggregation.

The JSON and Markdown summaries also include mapping, decompile-failure, and
behavior-status buckets plus language/tag/entry breakdowns. `--jobs` changes
only execution scheduling; row order is restored before artifacts are written.
If `orjson` is installed it is used as an optional JSON read/write fast path;
otherwise the standard library `json` module is used.

Fission decompile results are cached in
`benchmark/artifacts/source_semantic_benchmark/.cache/decomp_cache.json` by
default. Cache keys include the input binary path/stat, `fission_cli` path/stat,
function address, and whether `--include-debug-decomp` is enabled, so rebuilding
`fission_cli` invalidates old decompile rows automatically. Use
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
