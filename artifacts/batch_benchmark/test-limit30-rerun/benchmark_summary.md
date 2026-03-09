# Whole Decomp Benchmark: test_control_flow_x64_O0.exe

## Summary

- Shared functions: 30
- Both decompiled successfully: 25
- Aggregate normalized similarity: 24.46%
- Average normalized similarity: 56.96%
- Fission wall clock: 5.537s
- Ghidra wall clock: 1.613s
- Wall speedup vs Ghidra: 0.291x

## Coverage

- Fission functions: 30 (success 25)
- Fission reported-success before cleanup: 28
- Fission explicit errors: 2
- Fission synthetic failures: 3
- Ghidra functions: 30 (success 30)
- Fission-only addresses: 0
- Ghidra-only addresses: 0

## Speed Breakdown

- Fission init: 0.183s
- Fission pure decomp: 4.470s
- Fission postprocess: 0.027s
- Ghidra pure decomp: 0.170s

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
| `0x1400017a8` | `read_until_sentinel(int const*, int)` | `read_until_sentinel` | 54.02% |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
