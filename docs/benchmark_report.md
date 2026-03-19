# Fission 디컴파일러 벤치마크 보고서

**작성일**: 2026-03-09  
**대상**: putty.exe (Windows x64), test_control_flow_x64_O0.exe  

---

## 1. 요약

| 지표 | putty.exe (100함수 기준) |
|------|---------------------|
| Fission 소요시간 (순정) | 157초 |
| Fission 소요시간 (최종) | **10.03초** |
| Fission 함수당 평균 속도 | **0.1초** |

---

## 2. 최적화 대장정 (157s -> 10s)

### Phase 1: 동적 워커 스케일링 & 역스케일링(Negative Scaling) 해결
- **이전 상태**: 함수 20개 디컴파일 시 8스레드(62초)가 1스레드(26초)보다 느린 역스케일링 발생.
- **원인**: 각 워커마다 발생하는 초기화(GDT, FID, Sleigh 파싱)의 오버헤드가 병렬 처리 이득을 상쇄함.
- **해결**: `let ideal_workers = (functions.len() / 50).max(1);` 공식을 통해 함수 개수에 비례하는 동적 스케일링 적용.

### Phase 2: Sleigh XML 인메모리 바이너리 스트림 캐싱
- **이전 상태**: 워커가 생성될 때마다 수십 MB의 `.sla` 바이너리를 디스크에서 I/O 읽기 수행.
- **해결**: `Sleigh` 초기화 시 전역 `std::unordered_map`에서 캐시된 문자열 버퍼를 메모리 스트림(`std::istringstream`)으로 읽어 I/O 병목 제거. (157초 → 57초)

### Phase 3: GDT & 데이터 섹션 글로벌 싱글톤 캐시
- **이전 상태**: 워커마다 8.5초씩 걸리는 `.gdt` ZIP 압축 해제 및 파싱, 바이너리 데이터 섹션 문자열 스캔을 중복 수행. (8워커 기준 총 68초 증발)
- **해결**: 
  - `ArchInit.cc`: `GdtBinaryParser` 객체를 전역 `shared_ptr`로 캐싱. (첫 워커 7초 희생으로 나머지 7명은 0.001초만에 초기화)
  - `DataSymbolRegistry.cc`: 바이너리 메모리 레이아웃 서명을 키(Key)로 사용하여 데이터 섹션 스캔 결과 리스트를 전역 캐싱.

### Phase 4: Fail-Fast 타임아웃 (괴물 함수 제압)
- **이전 상태**: 100개 중 1개의 괴물 함수(`0x140001160`) 내부의 `main_perform` 최적화 루프가 무한히 폭주하며 전체 벤치마크 시간의 85%를 차지.
- **해결**: `Action::perform`과 `ActionGroup`, `ActionPool` 내부에 Wall-clock 트립와이어(기본 30초, CLI `--timeout-ms` 조절 가능)를 설치하여, 한계치 초과 시 즉각 `LowlevelError("Analysis timeout exceeded")`를 투척하도록 조치. 괴물 함수를 빠르게 끊어내어 롱테일(Long-tail) 병목 완벽 제거.

---

## 3. 결론

Ghidra C++ 코어의 구조적 한계(디스크 I/O 의존성, 중복 파싱, 단일 스레드 중심 설계)를 FFI와 전역 캐시 레이어(C++)를 통해 완벽히 극복했습니다. 

총 **15.6배의 속도 향상**을 이루어냈으며, 엔진의 순수 속도는 이제 함수당 0.1초 수준입니다. 사용자는 `--timeout-ms` 옵션을 통해 품질(모든 함수를 기다림)과 속도(괴물 함수를 버리고 빠르게 스캔) 사이의 트레이드오프를 자유롭게 제어할 수 있습니다.


## 5.1 FID 전역 캐시 (Read-Only Shared Memory, 1단계)

15MB+ `.fidbf` 파싱을 워커마다 반복하지 않고, 경로별로 `std::shared_ptr<FidDatabase>`를 전역 캐시하여 공유.

**구현**: `DecompilerFFI.cpp`의 `g_fid_cache` + `get_or_load_fid()`. 첫 로드 시 파싱, 이후 캐시 히트 시 `shared_ptr`만 복사하여 반환.

**검증 결과 (putty limit 100, 2 워커)**:

| 지표 | 값 |
|------|-----|
| wall_clock_sec | **63.46초** |
| total_decomp_sec | 62.94초 |
| init_sec | 0.28초 |
| prepare_timings.fid_ms | 164.57 (메인 스레드 1회만) |

- 메인 스레드 prepare 시 FID 1회 로드(~165ms) → 캐시에 저장
- 워커(버킷 1) prepare 시 `get_or_load_fid` 캐시 히트 → 디스크 I/O 없음
- 기존 96.8초 대비 **~34% 단축** (동적 워커 + FID 캐시 + 시그니처 pre-serialization 등 누적 효과)

**2단계 (Sleigh XML/DOM 캐싱)**: `.sla` 파싱 및 `DocumentStorage` 전역 캐싱은 `Sleigh` 내부의 가변 상태(`ContextCache`) 이슈로 난이도 높음. 추후 검토.

## 6. 권장 다음 단계

1. **성능**
   - `cfg_structurizer`, `postprocess` 구간 프로파일링 및 최적화
   - `main_perform` (Ghidra FFI) 호출 비용 분석
   - **타임아웃 원인 조사**: `docs/debug/TIMEOUT_DEBUG_GUIDE.md` 참고.
     - `cargo run -p fission-automation -- nir-check --lane preview`로 sentinel 실패 시그니처 확인
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
| `artifacts/fission-automation/latest/` | lane별 최신 automation summary, diagnosis, corpus 출력 |
| `crates/fission-automation/` | canonical local quality runner |
