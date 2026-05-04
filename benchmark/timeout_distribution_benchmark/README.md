# Timeout distribution benchmark

Offline join of **Ghidra oracle JSON** and **Fission result JSON** by address, producing cross-tabs of success/failure, per-threshold counts of rows that are **slow or failed**, and latency percentiles (p50 / p95 / p99) for successful samples.

## Inputs

### `--oracle`

Output from [`benchmark/ghidra_oracle_benchmark/export_oracle.py`](../ghidra_oracle_benchmark/export_oracle.py) (`rows[].address`, `rows[].ghidra.decompile_success`, `rows[].ghidra.decompile_sec`).

Rows with a null address (snapshot-only rows) are excluded from the join.

### `--fission`

Auto-detects one of:

- Top-level `entries`: `{ "<addr>": { "success", "wall_sec" | "decomp_sec", ... } }`
- `functions[]`: each item has `address`, `success`, and `wall_sec` or `decomp_sec`
- `pairwise.pyghidra_vs_fission.comparisons[]`: `address` (or `seed_address`) plus success flags and timing fields

Address normalization uses the same [`normalize_address`](../full_benchmark/grand_finale_support/metrics.py) helper as Grand Finale.

## Example

```bash
python3 benchmark/timeout_distribution_benchmark/summarize_timeouts.py \
  --oracle benchmark/artifacts/ghidra_oracle/export_smoke.json \
  --fission path/to/fission_functions_or_benchmark_slice.json \
  --thresholds-sec 1,5,30,180 \
  --out benchmark/artifacts/timeout_distribution/summary.json
```

## Interpretation

- **Soft timeout** buckets include **failed rows plus successful rows whose latency exceeds the threshold** (this may differ from a decompiler engine hard timeout).
- Per-stage Fission timings (`preview_build_stats`, etc.) can be split out in a follow-on script; this tool focuses on summary fields.
