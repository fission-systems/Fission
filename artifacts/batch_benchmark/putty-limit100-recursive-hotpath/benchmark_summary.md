# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 78
- Aggregate normalized similarity: 2.81%
- Average normalized similarity: 32.39%
- Fission wall clock: 197.358s
- Ghidra wall clock: 4.513s
- Wall speedup vs Ghidra: 0.023x

## Coverage

- Fission functions: 100 (success 82)
- Fission reported-success before cleanup: 100
- Fission explicit errors: 0
- Fission synthetic failures: 18
- Ghidra functions: 100 (success 100)
- Fission-only addresses: 8
- Ghidra-only addresses: 8

## Speed Breakdown

- Fission init: 0.960s
- Fission pure decomp: 194.565s
- Fission postprocess: 0.976s
- Ghidra pure decomp: 3.019s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001160` | `FUN_0x140001160` | `FUN_140001160` | 2.09% |
| `0x140007da0` | `FUN_0x140007da0` | `FUN_140007da0` | 3.48% |
| `0x14000cf10` | `FUN_0x14000cf10` | `FUN_14000cf10` | 5.11% |
| `0x140007710` | `FUN_0x140007710` | `FUN_140007710` | 6.31% |
| `0x140006380` | `FUN_0x140006380` | `FUN_140006380` | 7.66% |
| `0x1400107d0` | `FUN_0x1400107d0` | `FUN_1400107d0` | 8.40% |
| `0x1400073d0` | `FUN_0x1400073d0` | `FUN_1400073d0` | 10.91% |
| `0x140006ef0` | `FUN_0x140006ef0` | `FUN_140006ef0` | 10.99% |
| `0x140006cf0` | `FUN_0x140006cf0` | `FUN_140006cf0` | 11.01% |
| `0x140007560` | `FUN_0x140007560` | `FUN_140007560` | 11.78% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001160` | `FUN_0x140001160` | 102.188984 | postprocess_ms=90515.804ms, main_perform_ms=8897.106ms, follow_flow_ms=1685.749ms | callee=59, callgraph=0 |
| `0x14000a120` | `FUN_0x14000a120` | 15.791615 | postprocess_ms=11641.663ms, analysis_passes_ms=2513.988ms, stage1_rerun_ms=1229.057ms | callee=6, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 14.977511 | postprocess_ms=11787.049ms, main_perform_ms=1429.679ms, analysis_passes_ms=1300.685ms | callee=7, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 9.911806 | postprocess_ms=7880.717ms, analysis_passes_ms=1549.986ms, stage2_rerun_ms=414.132ms | callee=9, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 8.176704 | postprocess_ms=248.127ms, analysis_passes_ms=62.516ms, main_perform_ms=3.779ms | callee=3, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 8.110488 | postprocess_ms=4099.585ms, analysis_passes_ms=3089.568ms, stage2_rerun_ms=710.377ms | callee=17, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 6.991929 | analysis_passes_ms=3897.930ms, postprocess_ms=1931.039ms, stage1_rerun_ms=1136.276ms | callee=39, callgraph=0 |
| `0x140006380` | `FUN_0x140006380` | 5.264198 | analysis_passes_ms=4167.790ms, postprocess_ms=857.781ms, stage1_rerun_ms=213.258ms | callee=5, callgraph=0 |
| `0x140011890` | `FUN_0x140011890` | 3.203893 | postprocess_ms=2132.898ms, analysis_passes_ms=729.039ms, stage2_rerun_ms=220.791ms | callee=9, callgraph=0 |
| `0x14000cf10` | `FUN_0x14000cf10` | 3.064692 | analysis_passes_ms=1491.971ms, postprocess_ms=1467.680ms, stage1_rerun_ms=82.889ms | callee=29, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
