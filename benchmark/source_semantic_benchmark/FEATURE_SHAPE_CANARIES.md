# Source Semantic Feature-Shape Canaries

`manifests/feature_shape_canaries.json` is a focused source-owned advisory suite for quickly checking whether semantic benchmark output still preserves important source shapes.

> [!TIP]
> Run this suite before the full source-owned corpus when you want a fast first-pass answer to: “did this change break a recognizable semantic shape?”

It intentionally reuses checked-in source/binary pairs instead of adding new compiled artifacts. That keeps the canary cheap to review and safe to run on any checkout that already supports the source semantic benchmark corpus.

## What it covers

The suite groups existing fixtures by semantic shape rather than architecture alone:

- x86-64 Windows C pointer/array side effects (`sum_array`, `fill_matrix`, `swap`)
- AArch64 baremetal C switch/loop/branch/global-sink behavior (`control_flow.c`)
- AArch64 Apple baremetal C constants/global-sink behavior (`llvm_smoke.c`)

The manifest uses explicit `behavior_cases` so supported side-effect functions compare observable behavior through arrays or global sinks instead of relying only on static text similarity.

## Run

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --manifest benchmark/source_semantic_benchmark/manifests/feature_shape_canaries.json \
  --fission-bin target/release/fission_cli \
  --timeout-sec 45 \
  --jobs 1 \
  --output-dir benchmark/artifacts/source_semantic_benchmark/feature-shape-canaries-latest
```

<details>
<summary>Diagnosis-focused run with debug materialization</summary>

Use this when a row regresses or when the summary points to a low-scoring semantic shape. It materializes the existing CLI debug surfaces next to the benchmark artifacts.

```bash
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py \
  --manifest benchmark/source_semantic_benchmark/manifests/feature_shape_canaries.json \
  --fission-bin target/release/fission_cli \
  --timeout-sec 45 \
  --jobs 1 \
  --include-debug-decomp \
  --materialize-debug-triage \
  --output-dir benchmark/artifacts/source_semantic_benchmark/feature-shape-canaries-debug-latest
```

Start with:

- `debug_triage/` for materialized `decomp`, `disasm`, `xrefs`, and `inventory function-facts` captures
- `source_semantic_summary.md` for low-score repro commands
- `source_semantic_rows.json` for row-level owner/debug fields

</details>

## Reading results

Start with:

- `source_semantic_summary.json`
- `source_semantic_summary.md`
- `source_semantic_rows.json`

Useful first-pass signals:

- `behavior_pass_rate` for observable semantic preservation
- `static_semantic_score_percent` for text/shape drift when dynamic behavior is unsupported
- tag breakdowns for `pointer-array`, `matrix-write`, `switch`, `loop`, `branch`, `constants`, and `global-sink`
- low-score row repro commands in the Markdown summary

> [!IMPORTANT]
> This suite is advisory. It is intended to make regressions easy to see before promoting any source-semantic lane to a release-blocking gate.
