# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 78
- Aggregate normalized similarity: 2.81%
- Average normalized similarity: 32.38%
- Fission wall clock: 201.342s
- Ghidra wall clock: 4.662s
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

- Fission init: 0.934s
- Fission pure decomp: 197.764s
- Fission postprocess: 0.985s
- Ghidra pure decomp: 3.132s

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
| `0x140001160` | `FUN_0x140001160` | 103.141997 | postprocess_ms=91156.780ms, main_perform_ms=9095.326ms, follow_flow_ms=1724.183ms | callee=59, callgraph=0 |
| `0x14000a120` | `FUN_0x14000a120` | 16.057584 | postprocess_ms=11733.902ms, analysis_passes_ms=2600.657ms, main_perform_ms=1285.243ms | callee=6, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 15.458683 | postprocess_ms=11944.602ms, main_perform_ms=1538.291ms, analysis_passes_ms=1479.504ms | callee=7, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 9.925536 | postprocess_ms=7820.577ms, analysis_passes_ms=1612.533ms, stage1_rerun_ms=441.785ms | callee=9, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 8.382857 | postprocess_ms=4130.175ms, analysis_passes_ms=3273.673ms, stage2_rerun_ms=804.208ms | callee=17, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 8.085575 | postprocess_ms=216.957ms, analysis_passes_ms=56.249ms, main_perform_ms=4.118ms | callee=3, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 7.383937 | analysis_passes_ms=4100.883ms, postprocess_ms=1963.122ms, stage1_rerun_ms=1275.158ms | callee=39, callgraph=0 |
| `0x140006380` | `FUN_0x140006380` | 6.648566 | analysis_passes_ms=5484.179ms, postprocess_ms=902.211ms, stage2_rerun_ms=272.330ms | callee=5, callgraph=0 |
| `0x140011890` | `FUN_0x140011890` | 3.191147 | postprocess_ms=2066.367ms, analysis_passes_ms=756.067ms, main_perform_ms=241.795ms | callee=9, callgraph=0 |
| `0x14000cf10` | `FUN_0x14000cf10` | 3.004263 | analysis_passes_ms=1503.861ms, postprocess_ms=1388.755ms, main_perform_ms=88.532ms | callee=29, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
