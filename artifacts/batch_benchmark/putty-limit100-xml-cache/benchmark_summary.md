# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 63
- Aggregate normalized similarity: 5.02%
- Average normalized similarity: 34.53%
- Fission wall clock: 58.863s
- Ghidra wall clock: 4.453s
- Wall speedup vs Ghidra: 0.076x

## Resources (requires psutil)

- Install `pip install psutil` for resource usage metrics.

## Coverage

- Fission functions: 100 (success 65)
- Fission reported-success before cleanup: 83
- Fission explicit errors: 17
- Fission synthetic failures: 18
- Ghidra functions: 100 (success 100)
- Fission-only addresses: 8
- Ghidra-only addresses: 8

## Speed Breakdown

- Fission init: 0.260s
- Fission pure decomp: 57.832s
- Fission postprocess: 0.090s
- Ghidra pure decomp: 3.023s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001160` | `FUN_0x140001160` | `FUN_140001160` | 1.51% |
| `0x14000cf10` | `FUN_0x14000cf10` | `FUN_14000cf10` | 4.99% |
| `0x140007710` | `FUN_0x140007710` | `FUN_140007710` | 6.31% |
| `0x1400107d0` | `FUN_0x1400107d0` | `FUN_1400107d0` | 8.40% |
| `0x140007d30` | `FUN_0x140007d30` | `FUN_140007d30` | 10.02% |
| `0x140006ef0` | `FUN_0x140006ef0` | `FUN_140006ef0` | 10.15% |
| `0x1400073d0` | `FUN_0x1400073d0` | `FUN_1400073d0` | 10.72% |
| `0x140006cf0` | `FUN_0x140006cf0` | `FUN_140006cf0` | 10.82% |
| `0x140006fe0` | `FUN_0x140006fe0` | `FUN_140006fe0` | 13.20% |
| `0x140009690` | `FUN_0x140009690` | `FUN_140009690` | 13.27% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001160` | `FUN_0x140001160` | 11.259737 | main_perform_ms=8280.831ms, follow_flow_ms=1522.541ms, postprocess_ms=1312.841ms | callee=11, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 8.175323 | postprocess_ms=7214.567ms, cfg_structurizer_ms=6042.154ms, analysis_passes_ms=497.315ms | callee=2, callgraph=0 |
| `0x140001080` | `FUN_0x140001080` | 7.865810 | follow_flow_ms=0.899ms, main_perform_ms=0.000ms, analysis_passes_ms=0.000ms | callee=0, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 7.712457 | analysis_passes_ms=6.479ms, main_perform_ms=3.501ms, follow_flow_ms=0.758ms | callee=1, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 3.114078 | follow_flow_ms=414.433ms, main_perform_ms=0.000ms, analysis_passes_ms=0.000ms | callee=0, callgraph=0 |
| `0x14000a120` | `FUN_0x14000a120` | 2.601474 | follow_flow_ms=367.938ms, main_perform_ms=0.000ms, analysis_passes_ms=0.000ms | callee=0, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 2.160784 | follow_flow_ms=20.601ms, main_perform_ms=0.000ms, analysis_passes_ms=0.000ms | callee=0, callgraph=0 |
| `0x14000cf10` | `FUN_0x14000cf10` | 1.545590 | postprocess_ms=1313.479ms, cfg_structurizer_ms=814.330ms, analysis_passes_ms=128.234ms | callee=4, callgraph=0 |
| `0x14000be20` | `FUN_0x14000be20` | 1.533129 | postprocess_ms=1035.176ms, cfg_structurizer_ms=611.836ms, analysis_passes_ms=317.464ms | callee=0, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 1.457996 | follow_flow_ms=206.221ms, main_perform_ms=0.000ms, analysis_passes_ms=0.000ms | callee=0, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
