# Batch Benchmark

Fission currently has two benchmark entrypoints:

- `compare_legacy_preview.py`: fixed-seed function-level benchmark
- `full_decomp_benchmark.py`: whole-binary benchmark

Both now support the same 3-way engine story:

- `pyghidra`: Python-host baseline
- `legacy`: native FFI baseline
- `preview`: Rust preview pipeline

## Requirements

- `pyghidra`
- optional `psutil` for RSS / CPU metrics
- `GHIDRA_INSTALL_DIR` configured, or `vendor/ghidra/ghidra_11.4.2_PUBLIC`
- a `fission_cli` binary built with `native_decomp`

## Example Usage

```bash
# Function-level 3-way comparison on fixed seeds
python3 scripts/test/batch_benchmark/compare_legacy_preview.py \
  samples/windows/x64/putty.exe \
  --addresses 0x140006380 \
  --with-ghidra \
  --repeat 3 \
  --fission-bin target/release/fission_cli \
  --output-dir artifacts/compare_legacy_preview/putty-fixed

# Full decompilation (large binaries may take 20-30+ minutes)
python3 scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC \
  --output-dir artifacts/batch_benchmark/putty-full

# Faster validation: only the first N functions (recommended)
python3 scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/test_control_flow_x64_O0.exe \
  --limit 30 \
  --timeout 300
```

## Generated Artifacts

- function-level:
  - `*_legacy_vs_preview.json`: combined fixed-seed artifact with `pyghidra`, `legacy`, and `preview`
  - `*_legacy_vs_preview.md`: human-readable fixed-seed report
- whole-binary:
  - `legacy_full.json`: raw native FFI legacy output
  - `preview_full.json`: raw Rust preview output
- `ghidra_full.json`: raw full-decompilation JSON from pyghidra
- `benchmark_summary.json`: metadata plus per-function comparison results
- `benchmark_summary.md`: human-readable summary
- `legacy_stdout.log`, `legacy_stderr.log`: legacy engine logs
- `preview_stdout.log`, `preview_stderr.log`: preview engine logs

## Quality Metrics

- fixed-seed:
  - `goto_count`
  - `top_level_label_count`
  - `must_emit_label_count`
  - `empty_if_count`
  - `constant_if_count`
  - `residue_score`
  - preview routing / fallback / structuring counters
- whole-binary:
  - address-based function matching
  - success rate and matching coverage
  - per-function raw / normalized similarity
  - aggregate normalized similarity across the concatenated corpus

## Speed Metrics

- function-level:
  - `min/avg/median/p95` wall-clock timing per engine
- whole-binary:
  - `init_sec`
  - `total_decomp_sec`
  - `total_postprocess_sec`
  - `wall_clock_sec`
- optional resources:
  - `max_rss_mb`
  - `avg_rss_mb`
  - `avg_cpu_pct`
  - `max_cpu_pct`

## Investigating Timeout Causes

If `--limit 20` results in a 900-second timeout:

```bash
# Identify the culprit function by testing each one individually
python scripts/test/batch_benchmark/find_timeout_culprit.py samples/windows/x64/putty.exe --limit 20 --timeout 120 --verbose
```

For the full procedure, see [`docs/debug/TIMEOUT_DEBUG_GUIDE.md`](../../../docs/debug/TIMEOUT_DEBUG_GUIDE.md).

## Current Validation Snapshot

- `test_control_flow_x64_O0.exe --limit 30`
  - Fission: `init 0.183s`, `decomp 4.470s`, `post 0.027s`, success `25/30`
  - Ghidra: `init 1.412s`, `decomp 0.170s`, success `30/30`
  - Fission synthetic failure `3`, explicit error `2`
- `putty.exe --limit 100`
  - Fission: `init 0.260s`, `decomp 157.037s`, success `50/100`
  - Ghidra: `init 1.767s`, `decomp 3.140s`, success `100/100`
  - the main bottleneck is currently closer to per-function decompilation than preparation
