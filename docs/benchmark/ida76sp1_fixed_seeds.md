# ida76sp1 Fixed Seeds

`ida76sp1` is an x64 portable multi-DLL C++ / plugin corpus. It combines `ida64.exe`, `idat64.exe`, shared DLLs, and a representative plugin, making it useful for regression on large C++ GUI binaries plus shared-DLL and plugin-ecosystem behavior.

The initial baseline is fixed to these five binaries:

- [`samples/windows/x64/ida76sp1/ida64.exe`](../../samples/windows/x64/ida76sp1/ida64.exe)
- [`samples/windows/x64/ida76sp1/idat64.exe`](../../samples/windows/x64/ida76sp1/idat64.exe)
- [`samples/windows/x64/ida76sp1/ida64.dll`](../../samples/windows/x64/ida76sp1/ida64.dll)
- [`samples/windows/x64/ida76sp1/ida.dll`](../../samples/windows/x64/ida76sp1/ida.dll)
- [`samples/windows/x64/ida76sp1/plugins/hexrays.dll`](../../samples/windows/x64/ida76sp1/plugins/hexrays.dll)

## Seed Policy

- source: `fission_cli --list`
- filter: exclude `[import]` and `[thunk]`
- strategy: use five deterministic quantile seeds from the filtered function list: first, 25%, 50%, 75%, tail
- purpose: keep a fixed-seed regression set with a deterministic mix of small / medium / heavy cases

The canonical seed manifest lives in [`ida76sp1_fixed_seeds.json`](./ida76sp1_fixed_seeds.json).

## Baseline Summary

| Binary | Filtered Functions | Direct Preview | Fallback | Legacy | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ida64.exe` | 10484 | 4 | 0 | 1 | 0 |
| `idat64.exe` | 5318 | 4 | 0 | 1 | 0 |
| `ida64.dll` | 11344 | 4 | 0 | 1 | 1 |
| `ida.dll` | 11240 | 4 | 0 | 1 | 0 |
| `hexrays.dll` | 8022 | 3 | 0 | 2 | 1 |

## Artifact Policy

JSON and markdown compare artifacts are generated locally per benchmark run. The checked-in manifest and summary notes are the repository source of truth.

## Initial Watchlist

1. `hexrays.dll` has the lowest direct-preview ratio and still includes one timeout.
2. `ida64.dll` has strong direct-preview coverage but still includes one timeout.
3. `idat64.exe`, `ida64.exe`, and `ida.dll` have relatively stable direct-preview ratios and work well as baseline points for x64 multi-DLL regression.
