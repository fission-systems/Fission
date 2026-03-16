# ida76sp1 Fixed Seeds

`ida76sp1`는 x64 포터블 멀티-DLL C++/plugin corpus다. 이 코퍼스는 `ida64.exe`, `idat64.exe`, 공용 DLL, 대표 plugin을 함께 다뤄서 large C++ GUI + shared DLL + plugin ecosystem 회귀를 측정하는 데 쓴다.

이번 1차 baseline은 다음 5개 바이너리로 고정한다.

- [`/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.exe`](/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.exe)
- [`/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/idat64.exe`](/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/idat64.exe)
- [`/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.dll`](/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida64.dll)
- [`/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida.dll`](/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/ida.dll)
- [`/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/plugins/hexrays.dll`](/Users/sjkim1127/Fission/samples/windows/x64/ida76sp1/plugins/hexrays.dll)

## Seed Policy

- source: `fission_cli --list`
- filter: `[import]`, `[thunk]` 제외
- strategy: filtered function list의 first, 25%, 50%, 75%, tail quantile 5개
- 목적: small / medium / heavy를 deterministic하게 섞은 fixed-seed 회귀 세트 유지

정식 seed manifest는 [`/Users/sjkim1127/Fission/docs/benchmark/ida76sp1_fixed_seeds.json`](/Users/sjkim1127/Fission/docs/benchmark/ida76sp1_fixed_seeds.json)에 둔다.

## Baseline Summary

| Binary | Filtered Functions | Direct Preview | Fallback | Legacy | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ida64.exe` | 10484 | 4 | 0 | 1 | 0 |
| `idat64.exe` | 5318 | 4 | 0 | 1 | 0 |
| `ida64.dll` | 11344 | 4 | 0 | 1 | 1 |
| `ida.dll` | 11240 | 4 | 0 | 1 | 0 |
| `hexrays.dll` | 8022 | 3 | 0 | 2 | 1 |

## Baseline Artifacts

- [`/tmp/v71_ida64_exe/ida64_legacy_vs_preview.md`](/tmp/v71_ida64_exe/ida64_legacy_vs_preview.md)
- [`/tmp/v71_idat64_exe/idat64_legacy_vs_preview.md`](/tmp/v71_idat64_exe/idat64_legacy_vs_preview.md)
- [`/tmp/v71_ida64_dll/ida64_legacy_vs_preview.md`](/tmp/v71_ida64_dll/ida64_legacy_vs_preview.md)
- [`/tmp/v71_ida_dll/ida_legacy_vs_preview.md`](/tmp/v71_ida_dll/ida_legacy_vs_preview.md)
- [`/tmp/v71_hexrays_dll/hexrays_legacy_vs_preview.md`](/tmp/v71_hexrays_dll/hexrays_legacy_vs_preview.md)

JSON artifacts는 같은 디렉터리에 함께 둔다. 이 baseline은 IDA decompiler와의 비교용이 아니라, Fission 내부 fixed-seed regression과 향후 cross-image symbol/type propagation 실험의 출발점으로 유지한다.

## Initial Watchlist

1. `hexrays.dll`은 direct preview 비율이 가장 낮고 timeout도 1건 있다.
2. `ida64.dll`은 direct preview 비율은 높지만 timeout 1건이 있다.
3. `idat64.exe`, `ida64.exe`, `ida.dll`은 direct preview 비율이 상대적으로 안정적이라 x64 멀티-DLL 회귀 세트의 기준점으로 쓰기 좋다.
