# Whole Decomp Benchmark: test_control_flow_x64_O0.exe

## Summary

- Shared functions: 30
- Both decompiled successfully: 27
- Aggregate normalized similarity: 31.28%
- Average normalized similarity: 58.90%
- Fission wall clock: 5.039s
- Ghidra wall clock: 1.628s
- Wall speedup vs Ghidra: 0.323x

## Coverage

- Fission functions: 30 (success 27)
- Fission reported-success before cleanup: 30
- Fission explicit errors: 0
- Fission synthetic failures: 3
- Ghidra functions: 30 (success 30)
- Fission-only addresses: 0
- Ghidra-only addresses: 0

## Speed Breakdown

- Fission init: 0.176s
- Fission pure decomp: 4.700s
- Fission postprocess: 0.029s
- Ghidra pure decomp: 0.170s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001010` | `__tmainCRTStartup` | `__tmainCRTStartup` | 11.21% |
| `0x14000191d` | `memzero_manual(char*, int)` | `memzero_manual` | 14.31% |
| `0x140001612` | `count_digits(int)` | `count_digits` | 15.42% |
| `0x1400014e9` | `sparse_switch(int)` | `sparse_switch` | 20.24% |
| `0x140001d30` | `__dyn_tls_dtor` | `__dyn_tls_dtor` | 20.81% |
| `0x140001d50` | `__dyn_tls_init` | `__dyn_tls_init` | 25.95% |
| `0x140001c30` | `__do_global_dtors` | `__do_global_dtors` | 29.27% |
| `0x14000157e` | `classify_temperature(int)` | `classify_temperature` | 38.63% |
| `0x140001450` | `day_name(int)` | `day_name` | 48.20% |
| `0x140001a00` | `main` | `main` | 50.99% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001000` | `__mingw_invalidParameterHandler` | 2.321185 | postprocess_ms=4.010ms, analysis_passes_ms=1.186ms, main_perform_ms=0.426ms | callee=0, callgraph=0 |
| `0x140001010` | `__tmainCRTStartup` | 1.145465 | postprocess_ms=782.712ms, analysis_passes_ms=274.631ms, stage2_rerun_ms=81.330ms | callee=18, callgraph=0 |
| `0x140001a00` | `main` | 0.235304 | postprocess_ms=158.603ms, analysis_passes_ms=53.539ms, stage1_rerun_ms=18.071ms | callee=14, callgraph=0 |
| `0x140001d30` | `__dyn_tls_dtor` | 0.171730 | postprocess_ms=145.789ms, analysis_passes_ms=16.014ms, stage1_rerun_ms=7.109ms | callee=3, callgraph=0 |
| `0x140001d50` | `__dyn_tls_init` | 0.128332 | postprocess_ms=115.531ms, main_perform_ms=8.428ms, follow_flow_ms=1.794ms | callee=0, callgraph=0 |
| `0x1400013e0` | `WinMainCRTStartup` | 0.094153 | analysis_passes_ms=81.921ms, postprocess_ms=9.846ms, main_perform_ms=0.643ms | callee=1, callgraph=0 |
| `0x140001400` | `mainCRTStartup` | 0.093520 | analysis_passes_ms=81.767ms, postprocess_ms=9.397ms, main_perform_ms=0.588ms | callee=1, callgraph=0 |
| `0x1400016b3` | `matrix_multiply(int const (*) [4], int const (*) [4], int (*) [4])` | 0.074059 | postprocess_ms=52.959ms, analysis_passes_ms=8.778ms, stage2_rerun_ms=7.911ms | callee=0, callgraph=0 |
| `0x1400014e9` | `sparse_switch(int)` | 0.042258 | postprocess_ms=35.115ms, main_perform_ms=3.875ms, follow_flow_ms=0.972ms | callee=0, callgraph=0 |
| `0x14000191d` | `memzero_manual(char*, int)` | 0.041519 | postprocess_ms=30.675ms, analysis_passes_ms=4.388ms, stage2_rerun_ms=3.714ms | callee=0, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
