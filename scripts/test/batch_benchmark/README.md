# Batch Benchmark

`full_decomp_benchmark.py` compares full-binary decompilation quality and speed between Fission and Ghidra (`pyghidra`).

## Requirements

- `pyghidra`
- `GHIDRA_INSTALL_DIR` configured, or `vendor/ghidra/ghidra_11.4.2_PUBLIC`
- a `fission_cli` binary built with `native_decomp`

## Example Usage

```bash
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

- `fission_full.json`: raw full-decompilation JSON from Fission
- `ghidra_full.json`: raw full-decompilation JSON from pyghidra
- `benchmark_summary.json`: metadata plus per-function comparison results
- `benchmark_summary.md`: human-readable summary
- `fission_stdout.log`, `fission_stderr.log`: Fission execution logs

## Quality Metrics

- address-based function matching
- success rate and matching coverage
- per-function raw / normalized similarity
- aggregate normalized similarity across the concatenated corpus

## Speed Metrics

- `init_sec`: initialization time
- `total_decomp_sec`: total pure decompilation time
- `total_postprocess_sec`: total Rust postprocessing time
- `wall_clock_sec`: total end-to-end runtime

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
