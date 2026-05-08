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

If `--output-dir` is omitted, the runner writes to a timestamped artifact
directory under `benchmark/artifacts/source_semantic_benchmark/` instead of
overwriting a `latest` directory. Each run also appends a compact record to
`benchmark/artifacts/source_semantic_benchmark/source_semantic_history.jsonl`
so score, behavior, compile, cache, wall-time, and baseline-delta trends survive
across runs.

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
inside the same process can still reuse the in-memory cache.

By default, each run looks for the latest previous artifact under
`benchmark/artifacts/source_semantic_benchmark/` with the same manifest name and
adds a `comparison` block to the summary. Use `--baseline-dir <artifact-dir>` to
pin a specific previous run or `--no-baseline-compare` to disable this. The
comparison reports metric deltas, improved/regressed rows, behavior status
transitions, and top per-function score changes.

For failure triage, `--include-debug-decomp` forwards `--debug-decomp` to
`fission_cli decomp` and stores compact stage status, owner buckets, and selected
quality evidence in each row. This is observation-only and does not affect
scoring, but it makes low-score rows easier to route back to SLEIGH, NIR,
structuring, or type/data owners. The JSON and Markdown summaries aggregate
debug owner buckets and quality-evidence totals when debug evidence is present.
Rows with a mapped function also include a ready-to-run `debug_decomp_command`
and, when `--include-debug-decomp` is used, the runner materializes the same CLI
bundle at `debug_decomp/<entry>/<function-address>.json`. The Markdown summary
lists the lowest-scoring repro commands first.

For dynamic behavior failures, each non-passing row also records a
`behavior.artifact_dir` with the exact generated oracle harness, candidate
harness, and compile/run result JSON. These files make timeouts, compile
failures, and mismatches reproducible without re-running the full benchmark.

The static comparison uses language-neutral fingerprints for control-flow,
operators, constants, calls, memory access shape, and signature shape. Dynamic
behavior currently compiles C-like decompiler output with a compatibility
header and runs deterministic cases for supported scalar integral signatures,
plus manifest-owned `behavior_cases` for C `int *` arguments and `void`
side-effect functions. If the local host cannot execute a freshly compiled C
probe, supported dynamic rows fail closed as `host_execution_unavailable`
instead of being silently skipped.
