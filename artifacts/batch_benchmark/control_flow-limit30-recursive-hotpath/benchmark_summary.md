# Whole Decomp Benchmark: test_control_flow_x64_O0.exe

## Summary

- Shared functions: 30
- Both decompiled successfully: 27
- Aggregate normalized similarity: 31.71%
- Average normalized similarity: 57.45%
- Fission wall clock: 5.766s
- Ghidra wall clock: 1.944s
- Wall speedup vs Ghidra: 0.337x

## Coverage

- Fission functions: 30 (success 27)
- Fission reported-success before cleanup: 30
- Fission explicit errors: 0
- Fission synthetic failures: 3
- Ghidra functions: 30 (success 30)
- Fission-only addresses: 0
- Ghidra-only addresses: 0

## Speed Breakdown

- Fission init: 0.309s
- Fission pure decomp: 4.953s
- Fission postprocess: 0.305s
- Ghidra pure decomp: 0.194s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001010` | `__tmainCRTStartup` | `__tmainCRTStartup` | 11.29% |
| `0x14000191d` | `memzero_manual(char*, int)` | `memzero_manual` | 14.31% |
| `0x140001612` | `count_digits(int)` | `count_digits` | 15.42% |
| `0x1400014e9` | `sparse_switch(int)` | `sparse_switch` | 20.24% |
| `0x140001d30` | `__dyn_tls_dtor` | `__dyn_tls_dtor` | 21.13% |
| `0x140001d50` | `__dyn_tls_init` | `__dyn_tls_init` | 27.32% |
| `0x140001c30` | `__do_global_dtors` | `__do_global_dtors` | 29.27% |
| `0x14000157e` | `classify_temperature(int)` | `classify_temperature` | 38.63% |
| `0x140001450` | `day_name(int)` | `day_name` | 48.20% |
| `0x140001a00` | `main` | `main` | 49.06% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001000` | `__mingw_invalidParameterHandler` | 2.460593 | postprocess_ms=164.440ms, analysis_passes_ms=0.448ms, main_perform_ms=0.191ms | callee=0, callgraph=0 |
| `0x140001010` | `__tmainCRTStartup` | 1.126552 | postprocess_ms=639.280ms, analysis_passes_ms=398.767ms, stage1_rerun_ms=80.509ms | callee=18, callgraph=0 |
| `0x140001a00` | `main` | 0.232662 | postprocess_ms=153.413ms, analysis_passes_ms=56.345ms, stage1_rerun_ms=17.902ms | callee=14, callgraph=0 |
| `0x140001d30` | `__dyn_tls_dtor` | 0.178936 | postprocess_ms=148.700ms, analysis_passes_ms=19.455ms, stage1_rerun_ms=7.717ms | callee=3, callgraph=0 |
| `0x140001d50` | `__dyn_tls_init` | 0.138349 | postprocess_ms=124.578ms, main_perform_ms=8.289ms, follow_flow_ms=1.810ms | callee=0, callgraph=0 |
| `0x1400013e0` | `WinMainCRTStartup` | 0.093820 | analysis_passes_ms=83.339ms, postprocess_ms=7.781ms, main_perform_ms=0.802ms | callee=1, callgraph=0 |
| `0x140001400` | `mainCRTStartup` | 0.091148 | analysis_passes_ms=81.533ms, postprocess_ms=7.263ms, main_perform_ms=0.654ms | callee=1, callgraph=0 |
| `0x1400016b3` | `matrix_multiply(int const (*) [4], int const (*) [4], int (*) [4])` | 0.084637 | postprocess_ms=62.047ms, analysis_passes_ms=10.424ms, stage2_rerun_ms=8.142ms | callee=0, callgraph=0 |
| `0x1400014e9` | `sparse_switch(int)` | 0.058882 | postprocess_ms=51.188ms, main_perform_ms=3.710ms, analysis_passes_ms=1.267ms | callee=0, callgraph=0 |
| `0x140001450` | `day_name(int)` | 0.058843 | postprocess_ms=50.447ms, main_perform_ms=4.022ms, analysis_passes_ms=1.644ms | callee=0, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
