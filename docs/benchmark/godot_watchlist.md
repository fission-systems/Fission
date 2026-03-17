# Godot Watchlist

[`/Users/sjkim1127/Fission/samples/windows/x64/Godot_v4.6.1-stable_win64.exe`](/Users/sjkim1127/Fission/samples/windows/x64/Godot_v4.6.1-stable_win64.exe) 는 164MB급 x64 단일 EXE 엔진 샘플이라 routine fixed-seed 회귀 세트보다 **2차 stress/watchlist corpus**로 다루는 편이 맞다. 이 바이너리는 `fission_cli --list` 자체가 짧은 시간 안에 끝나지 않아, 일반 코퍼스처럼 반복 전체 스캔을 돌리지 않고 one-time long scan으로 seed를 고정한다.

## Watchlist Role

- 역할: x64 large-engine / giant function / timeout-closure stress corpus
- 목적: 일반 회귀 수 증가가 아니라 large-engine worst-case coverage 보강
- 재실행 정책: 전체 `--list`는 반복 회귀에 넣지 않고, 아래 3개 주소만 targeted rerun

정식 watchlist manifest는 [`/Users/sjkim1127/Fission/docs/benchmark/godot_watchlist.json`](/Users/sjkim1127/Fission/docs/benchmark/godot_watchlist.json)에 둔다.

## Seed Policy

- source: x64 `.pdata` exception table one-time scan
- scan result: `.pdata` 기준 function-range `209650`개
- strategy: quantile fixed-seed가 아니라 manual curation
- 고정 역할:
  - `smaller-heavy` 1개
  - `heavy` 1개
  - `giant` 1개

이번 라운드에서는 direct preview readability 후보도 찾으려 했지만, 대표 후보가 120초 budget에서도 끝나지 않아 현재 baseline은 **timeout watchlist** 성격이 더 강하다.

## Fixed Watchlist

- `0x144574a30`
- `0x14435bac0`
- `0x141e17130`

## Baseline Summary

| Binary | Seeds | Direct Preview | Explicit Fallback | Legacy Success | Timeouts |
| --- | ---: | ---: | ---: | ---: | ---: |
| `Godot_v4.6.1-stable_win64.exe` | 3 | 0 | 0 | 0 | 3 |

## Baseline Artifacts

- [`/tmp/godot_watchlist/Godot_v4.6.1-stable_win64_legacy_vs_preview.md`](/tmp/godot_watchlist/Godot_v4.6.1-stable_win64_legacy_vs_preview.md)
- [`/tmp/godot_watchlist/Godot_v4.6.1-stable_win64_legacy_vs_preview.json`](/tmp/godot_watchlist/Godot_v4.6.1-stable_win64_legacy_vs_preview.json)

## Why These Three

1. `0x144574a30`
   - `smaller-heavy` lower-bound case
   - `.pdata` size `121` bytes인데도 20초 compare budget에서 legacy/preview 모두 결과를 못 낸다
2. `0x14435bac0`
   - `heavy` timeout case
   - `.pdata` size `5713` bytes로, engine-scale heavy body를 재현하는 대표 함수다
3. `0x141e17130`
   - `giant` worst-case case
   - `.pdata` size `296491` bytes로, 현재 binary에서 가장 큰 급의 giant function timeout watchlist다

## Initial Read

1. 이 샘플은 현재 fixed-seed general regression이 아니라 **timeout-closure / large-engine resilience** watchlist로 보는 것이 정확하다.
2. seed 3개 모두 20초 budget에서 legacy와 preview가 같은 timeout 상태로 끝났다.
3. routine regression에 무겁게 넣기보다, giant-function timeout closure나 future ARM64-scale large-engine 대비 stress corpus로 유지하는 편이 맞다.
