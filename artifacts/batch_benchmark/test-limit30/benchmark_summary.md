# Whole Decomp Benchmark: test_control_flow_x64_O0.exe

## Summary

- Shared functions: 30
- Both decompiled successfully: 28
- Aggregate normalized similarity: 20.05%
- Average normalized similarity: 50.86%
- Fission wall clock: 5.964s
- Ghidra wall clock: 1.534s
- Wall speedup vs Ghidra: 0.257x

## Coverage

- Fission functions: 30 (success 28)
- Ghidra functions: 30 (success 30)
- Fission-only addresses: 0
- Ghidra-only addresses: 0

## Lowest Similarity Samples

| Address | Fission | Ghidra | Norm Similarity |
|---|---|---|---:|
| `0x140001430` | `__gcc_register_frame` | `__gcc_register_frame` | 0.00% |
| `0x140001c80` | `__do_global_ctors` | `__do_global_ctors` | 0.00% |
| `0x140001d00` | `__main` | `__main` | 0.00% |
| `0x140001010` | `__tmainCRTStartup` | `__tmainCRTStartup` | 11.29% |
| `0x14000191d` | `memzero_manual(char*, int)` | `memzero_manual` | 14.31% |
| `0x140001612` | `count_digits(int)` | `count_digits` | 15.42% |
| `0x1400014e9` | `sparse_switch(int)` | `sparse_switch` | 20.24% |
| `0x140001d30` | `__dyn_tls_dtor` | `__dyn_tls_dtor` | 21.13% |
| `0x140001d50` | `__dyn_tls_init` | `__dyn_tls_init` | 27.32% |
| `0x140001c30` | `__do_global_dtors` | `__do_global_dtors` | 29.27% |

## Artifacts

- `fission_full.json`: raw Fission whole-decomp output
- `ghidra_full.json`: raw pyghidra whole-decomp output
- `benchmark_summary.json`: merged metrics and per-function comparison
