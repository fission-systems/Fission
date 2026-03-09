# 타임아웃(무한 루프) 디버깅 가이드

`--decomp-limit 20` 실행 시 900초 타임아웃이 발생한다면, 단순한 성능 저하가 아니라 **무한 루프**나 **정규식 파멸적 역추적(Catastrophic Backtracking)** 버그 가능성이 높습니다. 본 가이드는 범인 함수를 식별하고 프로파일링하는 방법을 정리합니다.

---

## 1단계: 타임아웃 유발 함수 식별

### 자동 스크립트 (권장)

```bash
# 처음 20개 함수를 각각 120초 제한으로 테스트
python scripts/test/batch_benchmark/find_timeout_culprit.py samples/windows/x64/putty.exe --limit 20 --timeout 120

# 상세 타이밍 출력
python scripts/test/batch_benchmark/find_timeout_culprit.py putty.exe --limit 20 --timeout 120 --verbose
```

**결과 해석:**
- `[TIMEOUT]` 표시된 주소: 120초 내 완료 실패 → **의심 함수**
- `[OK]` 이지만 60초 이상: 극단적 병목 후보
- 모두 완료 시: **병렬 실행** 시 상호작용(레이스, 락) 또는 **초기화 단계** 문제 가능성. 아래 "병렬 비활성화" 시도 권장.

### 수동 실행 (단일 함수)

```bash
# 함수 목록에서 주소 확인
./target/release/fission_cli samples/windows/x64/putty.exe -l --json | head -80

# 특정 함수만 디컴파일 (120초 제한)
timeout 120 ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 --benchmark --ghidra-compat -o /tmp/out.json
```

**주의:** `timeout`은 GNU coreutils 명령어. macOS에서는 `gtimeout`(brew install coreutils) 또는 별도 타임아웃 없이 `time`으로 측정 후 Ctrl+C로 중단해도 됩니다.

### 병렬 비활성화로 900초 타임아웃 원인 분리

단일 함수는 완료되는데 `--decomp-all --decomp-limit 20`만 타임아웃된다면, 병렬 실행 이슈일 수 있습니다.

```bash
# 단일 스레드로 실행 (순차 처리)
RAYON_NUM_THREADS=1 python scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe --limit 20 --timeout 600
```

- `RAYON_NUM_THREADS=1`에서 완료되면: 병렬/락 이슈 가능성
- 여전히 타임아웃이면: 특정 함수 무한 루프 또는 초기화 비용

---

## 2단계: Rust 프로파일링

범인 함수를 타겟으로 지정한 뒤, 다음 도구로 어느 패스에서 CPU를 소모하는지 확인합니다.

### cargo-flamegraph (권장)

```bash
cargo install flamegraph

# 범인 함수 디컴파일 프로파일
cargo flamegraph --bin fission_cli -- \
  samples/windows/x64/putty.exe --decomp 0x140001160 --benchmark -o /tmp/out.json
```

생성된 `flamegraph.svg`에서 넓은 막대가 CPU 집중 구간입니다.

**예상 병목:**
- `cfg_structurizer` / `postprocess` → CFG 구조화 또는 문자열 정규식
- `main_perform` / FFI 쪽 → Ghidra native 호출
- `analysis_passes` → 분석 루프

### macOS Instruments (Time Profiler)

```bash
xcrun xctrace record --template 'Time Profiler' -- \
  ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 --benchmark -o /tmp/out.json
```

### samply (크로스 플랫폼)

```bash
cargo install samply
samply record ./target/release/fission_cli samples/windows/x64/putty.exe \
  --decomp 0x140001160 -o /tmp/out.json
```

---

## 3단계: FFI 비용 및 캐싱 점검

매 함수 디컴파일마다 다음이 반복되는지 확인합니다:

- [ ] **시그니처 JSON**: 전체 시그니처를 매번 파싱하는지
- [ ] **GDT/타입 DB**: 바이너리당 한 번만 로드되는지
- [ ] **직렬화**: Ghidra ↔ Rust 간 JSON 직렬화/역직렬화 빈도

관련 코드:
- `fission-ffi/src/decomp/` — FFI 경계
- `fission-analysis/src/analysis/decomp/prepare.rs` — 초기화 및 prepare 옵션

---

## 4단계: 의심 구간별 점검

### postprocess (문자열 인라이닝 등)

- 정규식 패턴이 ReDoS(Regular expression Denial of Service)에 취약한지 확인
- 긴 입력에서 `.*`, `(.*)*` 같은 패턴은 파멸적 역추적 유발 가능
- `regex` 크레이트 사용 시 `regex::Regex::is_match` 대신 제한된 매칭 고려

### cfg_structurizer

- 그래프 순회에서 순환 참조로 인한 무한 루프 여부
- 방문한 노드 체크 누락 여부

### Ghidra native (main_perform)

- C++ 쪽 `DecompilationCore`에서 특정 IR 패턴에 대한 무한 루프
- 로그 레벨 상향 후 C++ 빌드에서 진행 상황 확인

---

## 5단계: 로그로 진행 상황 확인

```bash
# Rust 로그 (verbose)
./target/release/fission_cli putty.exe --decomp 0x140001160 --verbose 2>&1 | tee debug.log

# 특정 모듈 trace
RUST_LOG=fission_analysis=trace,fission_ffi=debug cargo run -p fission-cli -- putty.exe --decomp 0x140001160
```

초기화(FID, GDT 로드) 후 어떤 함수에서 멈추는지, stderr에 마지막으로 출력된 메시지를 확인합니다.

---

## 참고: 알려진 느린 함수 (putty.exe)

벤치마크 기준 병목 상위:

| 주소        | 함수명             | 참고 (putty-limit100-final) |
|------------|---------------------|-----------------------------|
| 0x140001160 | FUN_0x140001160    | 12.9s, main_perform + follow_flow |
| 0x140007da0 | FUN_0x140007da0    | 13.9s, postprocess + cfg_structurizer |
| 0x14000a120 | FUN_0x14000a120    | 16.1s, postprocess + cfg_structurizer |

이 함수들이 타임아웃 범인일 가능성이 높습니다. `--decomp 0x140001160` 등으로 먼저 단독 테스트해 보세요.
