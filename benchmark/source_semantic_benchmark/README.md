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
within each binary entry:

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

The static comparison uses language-neutral fingerprints for control-flow,
operators, constants, calls, memory access shape, and signature shape. Dynamic
behavior currently compiles C-like decompiler output with a compatibility
header and runs deterministic cases for supported scalar integral signatures,
plus manifest-owned `behavior_cases` for C `int *` arguments and `void`
side-effect functions. If the local host cannot execute a freshly compiled C
probe, supported dynamic rows fail closed as `host_execution_unavailable`
instead of being silently skipped.
