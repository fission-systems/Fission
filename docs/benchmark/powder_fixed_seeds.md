# Powder Fixed Seeds

[`/Users/sjkim1127/Fission/samples/windows/x64/Powder.exe`](/Users/sjkim1127/Fission/samples/windows/x64/Powder.exe) 는 x64 단일 EXE 게임/시뮬레이션 코퍼스다. utility 계열보다 상태 머신, event/update loop, custom data structure, giant function shape를 보기 좋아서 x64 실전 회귀 세트로 편입한다.

## Seed Policy

- source: `fission_cli --list`
- filter: `[import]` 제외, zero-sized 함수 제외
- strategy: internal non-zero function list의 size quantile을 기준으로 `small / medium / medium-heavy / heavy / giant` 5개를 고정
- 목적: x64 게임 계열 단일 EXE에서 direct preview, fallback kind, giant function readability를 반복 측정

정식 seed manifest는 [`/Users/sjkim1127/Fission/docs/benchmark/powder_fixed_seeds.json`](/Users/sjkim1127/Fission/docs/benchmark/powder_fixed_seeds.json)에 둔다.

## Fixed Seeds

- `0x140394a5d`
- `0x1401e6049`
- `0x1404b761c`
- `0x140161bc0`
- `0x14043f1c8`

## Baseline Summary

| Binary | Filtered Functions | Direct Preview | Fallback | Legacy | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `Powder.exe` | 16729 | 4 | 1 | 1 | 0 |

## Baseline Artifacts

- [`/tmp/powder_x64_fixed5/Powder_legacy_vs_preview.md`](/tmp/powder_x64_fixed5/Powder_legacy_vs_preview.md)
- [`/tmp/powder_x64_fixed5/Powder_legacy_vs_preview.json`](/tmp/powder_x64_fixed5/Powder_legacy_vs_preview.json)

## Initial Read

1. `4/5`는 direct `mlil_preview`로 완료된다.
2. legacy는 selected seeds 기준 `5/5` 전부 실패했다.
3. `0x14043f1c8`는 giant function이고 현재 direct preview 대신 explicit assembly fallback으로 끝난다.
4. `0x140394a5d`는 direct preview는 되지만 `xVar`/`reg` readability residue가 남아 있어 품질 타깃으로 좋다.

## Watchlist

1. `0x14043f1c8`
   - x64 giant fallback case
   - direct preview 복구 가능성 / timeout-free explicit fallback 유지 여부를 계속 본다
2. `0x140394a5d`
   - direct preview readability case
   - temp/reg/branch surface 정리 타깃
