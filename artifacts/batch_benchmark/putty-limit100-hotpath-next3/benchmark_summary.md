# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 78
- Aggregate normalized similarity: 2.81%
- Average normalized similarity: 32.35%
- Fission wall clock: 187.415s
- Ghidra wall clock: 4.519s
- Wall speedup vs Ghidra: 0.024x

## Coverage

- Fission functions: 100 (success 82)
- Fission reported-success before cleanup: 100
- Fission explicit errors: 0
- Fission synthetic failures: 18
- Ghidra functions: 100 (success 100)
- Fission-only addresses: 8
- Ghidra-only addresses: 8

## Speed Breakdown

- Fission init: 0.278s
- Fission pure decomp: 186.885s
- Fission postprocess: 0.093s
- Ghidra pure decomp: 3.043s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001160` | `FUN_0x140001160` | `FUN_140001160` | 2.09% |
| `0x140007da0` | `FUN_0x140007da0` | `FUN_140007da0` | 3.47% |
| `0x14000cf10` | `FUN_0x14000cf10` | `FUN_14000cf10` | 5.10% |
| `0x140007710` | `FUN_0x140007710` | `FUN_140007710` | 6.31% |
| `0x140006380` | `FUN_0x140006380` | `FUN_140006380` | 7.55% |
| `0x1400107d0` | `FUN_0x1400107d0` | `FUN_1400107d0` | 8.40% |
| `0x140001080` | `FUN_0x140001080` | `FUN_140001080` | 10.20% |
| `0x140006cf0` | `FUN_0x140006cf0` | `FUN_140006cf0` | 10.83% |
| `0x140006ef0` | `FUN_0x140006ef0` | `FUN_140006ef0` | 10.83% |
| `0x1400073d0` | `FUN_0x1400073d0` | `FUN_1400073d0` | 10.91% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001160` | `FUN_0x140001160` | 102.108625 | postprocess_ms=90480.907ms, main_perform_ms=8857.123ms, follow_flow_ms=1665.811ms | callee=59, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 14.681063 | postprocess_ms=11440.248ms, main_perform_ms=1460.107ms, analysis_passes_ms=1313.777ms | callee=7, callgraph=0 |
| `0x14000a120` | `FUN_0x14000a120` | 14.407038 | postprocess_ms=11170.044ms, analysis_passes_ms=1577.225ms, stage1_rerun_ms=1238.837ms | callee=6, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 9.184080 | postprocess_ms=7540.212ms, analysis_passes_ms=1160.882ms, stage2_rerun_ms=415.722ms | callee=9, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 8.053789 | postprocess_ms=228.816ms, analysis_passes_ms=53.045ms, main_perform_ms=3.859ms | callee=3, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 7.470217 | postprocess_ms=3960.276ms, analysis_passes_ms=2581.503ms, stage1_rerun_ms=694.736ms | callee=17, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 5.314025 | analysis_passes_ms=2349.049ms, postprocess_ms=1776.314ms, stage1_rerun_ms=1165.381ms | callee=39, callgraph=0 |
| `0x140006380` | `FUN_0x140006380` | 5.011553 | analysis_passes_ms=3981.070ms, postprocess_ms=786.269ms, stage1_rerun_ms=216.087ms | callee=5, callgraph=0 |
| `0x140011890` | `FUN_0x140011890` | 2.919771 | postprocess_ms=1930.199ms, analysis_passes_ms=648.456ms, stage1_rerun_ms=221.729ms | callee=9, callgraph=0 |
| `0x14000cf10` | `FUN_0x14000cf10` | 2.804490 | analysis_passes_ms=1385.852ms, postprocess_ms=1312.995ms, stage1_rerun_ms=84.588ms | callee=29, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
