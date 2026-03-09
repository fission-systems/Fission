# Whole Decomp Benchmark: test_control_flow_x64_O0.exe

## Summary

- Shared functions: 30
- Both decompiled successfully: 26
- Aggregate normalized similarity: 31.45%
- Average normalized similarity: 58.86%
- Fission wall clock: 5.294s
- Ghidra wall clock: 1.532s
- Wall speedup vs Ghidra: 0.289x

## Coverage

- Fission functions: 30 (success 26)
- Fission reported-success before cleanup: 30
- Fission explicit errors: 0
- Fission synthetic failures: 4
- Ghidra functions: 30 (success 30)
- Fission-only addresses: 0
- Ghidra-only addresses: 0

## Speed Breakdown

- Fission init: 0.319s
- Fission pure decomp: 4.469s
- Fission postprocess: 0.303s
- Ghidra pure decomp: 0.166s

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001010` | `__tmainCRTStartup` | `__tmainCRTStartup` | 11.21% |
| `0x14000191d` | `memzero_manual(char*, int)` | `memzero_manual` | 14.31% |
| `0x140001612` | `count_digits(int)` | `count_digits` | 15.42% |
| `0x140001d30` | `__dyn_tls_dtor` | `__dyn_tls_dtor` | 19.90% |
| `0x1400014e9` | `sparse_switch(int)` | `sparse_switch` | 20.24% |
| `0x140001d50` | `__dyn_tls_init` | `__dyn_tls_init` | 25.95% |
| `0x140001c30` | `__do_global_dtors` | `__do_global_dtors` | 29.27% |
| `0x14000157e` | `classify_temperature(int)` | `classify_temperature` | 38.63% |
| `0x140001450` | `day_name(int)` | `day_name` | 48.20% |
| `0x140001a00` | `main` | `main` | 50.99% |

## Fission Native Hot Paths

| Address | Function | Decomp Sec | Top Phases | Helper Counts |
|---|---|---:|---|---|
| `0x140001000` | `__mingw_invalidParameterHandler` | 2.307155 | postprocess_ms=3.472ms, cfg_structurizer_ms=2.987ms, analysis_passes_ms=0.601ms | callee=0, callgraph=0 |
| `0x140001010` | `__tmainCRTStartup` | 1.108285 | postprocess_ms=796.957ms, cfg_structurizer_ms=372.667ms, analysis_passes_ms=224.380ms | callee=4, callgraph=0 |
| `0x140001a00` | `main` | 0.234028 | postprocess_ms=158.336ms, cfg_structurizer_ms=70.215ms, analysis_passes_ms=52.571ms | callee=12, callgraph=0 |
| `0x140001d30` | `__dyn_tls_dtor` | 0.166590 | postprocess_ms=153.257ms, cfg_structurizer_ms=91.536ms, main_perform_ms=7.045ms | callee=0, callgraph=0 |
| `0x140001d50` | `__dyn_tls_init` | 0.126546 | postprocess_ms=113.008ms, cfg_structurizer_ms=45.586ms, main_perform_ms=8.206ms | callee=0, callgraph=0 |
| `0x1400016b3` | `matrix_multiply(int const (*) [4], int const (*) [4], int (*) [4])` | 0.075603 | postprocess_ms=53.095ms, cfg_structurizer_ms=23.780ms, analysis_passes_ms=10.175ms | callee=0, callgraph=0 |
| `0x14000191d` | `memzero_manual(char*, int)` | 0.042367 | postprocess_ms=30.700ms, cfg_structurizer_ms=12.011ms, analysis_passes_ms=5.238ms | callee=0, callgraph=0 |
| `0x1400014e9` | `sparse_switch(int)` | 0.041307 | postprocess_ms=33.455ms, cfg_structurizer_ms=22.629ms, main_perform_ms=3.793ms | callee=0, callgraph=0 |
| `0x140001450` | `day_name(int)` | 0.038477 | postprocess_ms=30.231ms, cfg_structurizer_ms=22.712ms, main_perform_ms=3.954ms | callee=0, callgraph=0 |
| `0x1400017a8` | `read_until_sentinel(int const*, int)` | 0.033214 | postprocess_ms=25.394ms, cfg_structurizer_ms=12.200ms, analysis_passes_ms=3.147ms | callee=0, callgraph=0 |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
