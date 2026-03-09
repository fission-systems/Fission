# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 74
- Aggregate normalized similarity: 3.40%
- Average normalized similarity: 32.98%
- Fission wall clock: 96.845s
- Ghidra wall clock: 5.782s
- Wall speedup vs Ghidra: 0.060x

## Coverage

- Fission functions: 100 (success 78)
- Fission reported-success before cleanup: 96
- Fission explicit errors: 4
- Fission synthetic failures: 18
- Ghidra functions: 100 (success 100)
- Fission-only addresses: 8
- Ghidra-only addresses: 8

## Speed Breakdown

- Fission init: 1.005s
- Fission pure decomp: 93.835s
- Fission postprocess: 0.995s
- Ghidra pure decomp: 3.689s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001160` | `FUN_0x140001160` | `FUN_140001160` | 1.51% |
| `0x140007da0` | `FUN_0x140007da0` | `FUN_140007da0` | 3.47% |
| `0x14000cf10` | `FUN_0x14000cf10` | `FUN_14000cf10` | 4.99% |
| `0x140007710` | `FUN_0x140007710` | `FUN_140007710` | 6.31% |
| `0x140006380` | `FUN_0x140006380` | `FUN_140006380` | 7.55% |
| `0x1400107d0` | `FUN_0x1400107d0` | `FUN_1400107d0` | 8.40% |
| `0x140006ef0` | `FUN_0x140006ef0` | `FUN_140006ef0` | 10.15% |
| `0x140001080` | `FUN_0x140001080` | `FUN_140001080` | 10.20% |
| `0x140006cf0` | `FUN_0x140006cf0` | `FUN_140006cf0` | 10.83% |
| `0x1400073d0` | `FUN_0x1400073d0` | `FUN_1400073d0` | 10.91% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x14000a120` | `FUN_0x14000a120` | 16.134488 | postprocess_ms=11571.589ms, cfg_structurizer_ms=9028.970ms, analysis_passes_ms=2734.477ms | callee=0, callgraph=0 |
| `0x140007da0` | `FUN_0x140007da0` | 13.934983 | postprocess_ms=11818.265ms, cfg_structurizer_ms=9973.011ms, main_perform_ms=1597.308ms | callee=1, callgraph=0 |
| `0x140001160` | `FUN_0x140001160` | 12.864456 | main_perform_ms=9483.857ms, follow_flow_ms=1772.736ms, postprocess_ms=1432.109ms | callee=11, callgraph=0 |
| `0x14000ded0` | `FUN_0x14000ded0` | 9.896577 | postprocess_ms=7858.223ms, cfg_structurizer_ms=6567.519ms, analysis_passes_ms=1522.458ms | callee=1, callgraph=0 |
| `0x140001000` | `FUN_0x140001000` | 8.030760 | analysis_passes_ms=10.422ms, main_perform_ms=3.716ms, follow_flow_ms=0.934ms | callee=1, callgraph=0 |
| `0x140008900` | `FUN_0x140008900` | 6.828981 | postprocess_ms=4127.996ms, cfg_structurizer_ms=3260.742ms, analysis_passes_ms=1696.517ms | callee=5, callgraph=0 |
| `0x1400052b0` | `FUN_0x1400052b0` | 5.291118 | analysis_passes_ms=2006.585ms, postprocess_ms=1968.504ms, main_perform_ms=1255.844ms | callee=7, callgraph=0 |
| `0x140011890` | `FUN_0x140011890` | 2.977466 | postprocess_ms=2014.717ms, cfg_structurizer_ms=1360.788ms, analysis_passes_ms=599.204ms | callee=1, callgraph=0 |
| `0x140006380` | `FUN_0x140006380` | 1.998010 | analysis_passes_ms=914.298ms, postprocess_ms=805.042ms, cfg_structurizer_ms=445.071ms | callee=0, callgraph=0 |
| `0x14000be20` | `FUN_0x14000be20` | 1.878658 | postprocess_ms=1122.323ms, cfg_structurizer_ms=657.705ms, analysis_passes_ms=553.918ms | callee=0, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
