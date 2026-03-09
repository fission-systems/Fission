# Fission 디컴파일러 벤치마크 보고서

**작성일**: 2026-03-09  
**대상**: putty.exe (Windows x64), test_control_flow_x64_O0.exe  
**비교 대상**: Ghidra 11.4.2 (pyghidra)

---

## 1. 요약

| 지표 | putty.exe (100함수) | test_control_flow (30함수) |
|------|---------------------|----------------------------|
| Fission 소요시간 | **96.8초** | **5.3초** |
| Ghidra 소요시간 | **5.8초** | **1.5초** |
| Wall speedup (Fission/Ghidra) | **0.06x** (~17배 느림) | **0.29x** (~3.5배 느림) |
| 정규화 유사도 (평균) | **32.98%** | **58.86%** |
| 둘 다 성공 | 74/92 공유 | 26/30 공유 |

---

## 2. 상세 결과

### 2.1 putty.exe (실제 바이너리, 100함수)

- **커버리지**
  - 공유 함수: 92개
  - Fission 성공: 78개 (reported 96, explicit error 4, synthetic failure 18)
  - Ghidra 성공: 100개
  - 양쪽 성공: 74개

- **속도 분해**
  - Fission init: 1.0초
  - Fission 순수 디컴파일: 93.8초
  - Fission 후처리: 1.0초
  - Ghidra 순수 디컴파일: 3.7초

- **품질**
  - Aggregate normalized similarity: 3.40%
  - Average normalized similarity: 32.98%
  - Median: 29.88%
  - Min~Max: 1.51% ~ 88.97%

- **병목 (Fission Native Hot Paths)**
  - `FUN_0x14000a120`: 16.1초 → `postprocess_ms`, `cfg_structurizer_ms`, `analysis_passes_ms`
  - `FUN_0x140007da0`: 13.9초 → `postprocess_ms`, `cfg_structurizer_ms`, `main_perform_ms`
  - `FUN_0x140001160`: 12.9초 → `main_perform_ms`, `follow_flow_ms`, `postprocess_ms`

### 2.2 test_control_flow_x64_O0.exe (테스트용 바이너리, 30함수)

- **커버리지**
  - 공유 함수: 30개 (100%)
  - Fission 성공: 26개 (synthetic failure 4)
  - Ghidra 성공: 30개

- **품질**
  - Average normalized similarity: **58.86%** (putty 대비 상대적으로 높음)
  - Aggregate normalized similarity: 31.45%

- **특징**: 단순한 제어 흐름 코드에서 유사도가 더 높게 나옴

---

## 3. 병목 분석

주요 병목 구간 (putty 기준):

| Phase | 역할 | 비고 |
|-------|------|------|
| `postprocess_ms` | 후처리/최적화 | 많은 함수에서 1~10초대 |
| `cfg_structurizer_ms` | CFG 구조화 | postprocess와 연관 |
| `main_perform_ms` | Ghidra native decomp 수행 | 복잡한 함수에서 8~9초 |
| `analysis_passes_ms` | 분석 패스 | 중간 비중 |
| `follow_flow_ms` | 흐름 추적 | 일부 함수에서 1.7초 |

이전 버전(putty-limit100-hotpath-next3)에서는 `postprocess_ms`가 단일 함수에서 **90초** 이상 소요되던 케이스가 있었으나, 최신(remaining-opt-validation)에서는 **cfg_structurizer** 중심으로 최적화되어 상대적으로 안정화된 것으로 보임.

---

## 4. 주의사항

- **최근 실행 이슈**: `--limit 20`, `--timeout 900` 설정으로 실행 시 Fission이 900초 내에 완료하지 못해 타임아웃 발생. 초기화(FID/GDT 로드, DataSection 스캔) 후 특정 함수에서 정체되거나, 환경/빌드 차이 가능성 있음.
- **리소스 모니터링**: `psutil` 의존성으로 CPU/RSS 수집 가능. 최근 런에서는 타임아웃으로 리소스 데이터 미확보.

---

## 5. 역스케일링(Negative Scaling) → 동적 워커 조절로 해결

- **단일 스레드**: 26초
- **멀티 스레드 (8 워커, limit 20)**: 62초 (역스케일링)
- **동적 워커 (limit 20)**: **26초** (워커 1개로 자동 전환)

**원인**: 워커당 init(FID/GDT/.sla 파싱)이 무거워, 함수 수가 적을 때 8개 워커가 오히려 느려짐.

**해결**: 워커당 최소 50개 함수를 목표로 동적 스케일링 적용.
```rust
let ideal_workers = (functions.len() / 50).max(1);
let num_workers = ideal_workers.min(rayon::current_num_threads().max(1));
```
- limit 20 → 1 워커 → 26초
- limit 100+ → 다중 워커 → 병렬 이득

## 6. 권장 다음 단계

1. **성능**
   - `cfg_structurizer`, `postprocess` 구간 프로파일링 및 최적화
   - `main_perform` (Ghidra FFI) 호출 비용 분석
   - **타임아웃 원인 조사**: `docs/debug/TIMEOUT_DEBUG_GUIDE.md` 참고.
     - `find_timeout_culprit.py`로 범인 함수 식별
     - `RAYON_NUM_THREADS=1`로 병렬 vs 순차 분리

2. **품질**
   - 유사도 30% 전후 함수들 상세 비교 (네이밍, 구조, 상수 복원)
   - synthetic failure 18건 유형 분석 및 개선

3. **커버리지**
   - Fission-only / Ghidra-only 8개씩 원인 분석
   - explicit error 4건 수정

4. **안정성**
   - 동일 환경에서 limit 10, 20, 50 벤치마크 반복 실행하여 재현성 확인

---

## 7. 참고 아티팩트

| 경로 | 설명 |
|------|------|
| `artifacts/batch_benchmark/remaining-opt-validation/putty-limit100-final/` | putty 최신 벤치마크 |
| `artifacts/batch_benchmark/remaining-opt-validation/control-flow-limit30/` | control-flow 테스트 벤치마크 |
| `scripts/test/batch_benchmark/full_decomp_benchmark.py` | 벤치마크 스크립트 |
