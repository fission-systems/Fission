# Whole Decomp Benchmark: putty.exe

## Summary

- Shared functions: 92
- Both decompiled successfully: 49
- Aggregate normalized similarity: 2.86%
- Average normalized similarity: 33.15%
- Fission wall clock: 157.621s
- Ghidra wall clock: 4.991s
- Wall speedup vs Ghidra: 0.032x

## Coverage

- Fission functions: 100 (success 50)
- Fission reported-success before cleanup: 68
- Fission explicit errors: 32
- Fission synthetic failures: 18
- Ghidra functions: 100 (success 100)
- Fission-only addresses: 8
- Ghidra-only addresses: 8

## Speed Breakdown

- Fission init: 0.260s
- Fission pure decomp: 157.037s
- Fission postprocess: 0.064s
- Ghidra pure decomp: 3.140s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001160` | `FUN_0x140001160` | `FUN_140001160` | 2.09% |
| `0x140007da0` | `FUN_0x140007da0` | `FUN_140007da0` | 3.48% |
| `0x140007710` | `FUN_0x140007710` | `FUN_140007710` | 6.31% |
| `0x1400107d0` | `FUN_0x1400107d0` | `FUN_1400107d0` | 8.40% |
| `0x140007560` | `FUN_0x140007560` | `FUN_140007560` | 11.78% |
| `0x140006fe0` | `FUN_0x140006fe0` | `FUN_140006fe0` | 13.20% |
| `0x140009690` | `FUN_0x140009690` | `FUN_140009690` | 13.85% |
| `0x140007460` | `FUN_0x140007460` | `FUN_140007460` | 14.15% |
| `0x14000bdc0` | `FUN_0x14000bdc0` | `FUN_14000bdc0` | 14.88% |
| `0x140007260` | `FUN_0x140007260` | `FUN_140007260` | 16.63% |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
