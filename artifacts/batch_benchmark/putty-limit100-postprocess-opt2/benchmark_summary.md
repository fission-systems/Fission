# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 78
- Aggregate normalized similarity: 2.81%
- Average normalized similarity: 32.35%
- Fission wall clock: 197.725s
- Ghidra wall clock: 4.736s
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

- Fission init: 0.934s
- Fission pure decomp: 194.735s
- Fission postprocess: 1.008s
- Ghidra pure decomp: 3.271s

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
| `0x140001160` | `FUN_0x140001160` | 101.228871 | postprocess_ms=89609.814ms, main_perform_ms=8787.095ms, follow_flow_ms=1690.052ms | callee=59, callgraph=0 |
| `0x14000a120` | `FUN_0x14000a120` | 15.836114 | postprocess_ms=11510.810ms, analysis_passes_ms=2628.352ms, stage1_rerun_ms=1302.067ms | callee=6, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 15.399314 | postprocess_ms=11831.548ms, main_perform_ms=1588.182ms, analysis_passes_ms=1470.524ms | callee=7, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 9.857622 | postprocess_ms=7739.124ms, analysis_passes_ms=1606.582ms, main_perform_ms=437.392ms | callee=9, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 8.253796 | postprocess_ms=4035.421ms, analysis_passes_ms=3203.767ms, main_perform_ms=746.048ms | callee=17, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 8.023320 | postprocess_ms=225.274ms, analysis_passes_ms=57.260ms, main_perform_ms=3.888ms | callee=3, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 7.212352 | analysis_passes_ms=4165.143ms, postprocess_ms=1802.086ms, stage1_rerun_ms=1276.976ms | callee=39, callgraph=0 |
| `0x140006380` | `FUN_0x140006380` | 5.865066 | analysis_passes_ms=4731.016ms, postprocess_ms=882.585ms, stage1_rerun_ms=243.160ms | callee=5, callgraph=0 |
| `0x140011890` | `FUN_0x140011890` | 3.310668 | postprocess_ms=2091.788ms, analysis_passes_ms=840.838ms, stage2_rerun_ms=279.343ms | callee=9, callgraph=0 |
| `0x14000cf10` | `FUN_0x14000cf10` | 3.040381 | analysis_passes_ms=1576.717ms, postprocess_ms=1352.396ms, stage1_rerun_ms=91.197ms | callee=29, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
