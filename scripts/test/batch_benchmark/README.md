# Batch Benchmark

`full_decomp_benchmark.py` 는 전체 바이너리를 기준으로 Fission 과 Ghidra(`pyghidra`)의
디컴파일 품질과 속도를 함께 비교합니다.

## 요구 사항

- `pyghidra` 설치
- `GHIDRA_INSTALL_DIR` 또는 `vendor/ghidra/ghidra_11.4.2_PUBLIC`
- `native_decomp` 가 포함된 `fission_cli` 바이너리

## 실행 예시

```bash
# 전체 디컴파일 (대형 바이너리는 20~30분 이상 소요 가능)
python3 scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC \
  --output-dir artifacts/batch_benchmark/putty-full

# 빠른 검증: 처음 N개 함수만 (권장)
python3 scripts/test/batch_benchmark/full_decomp_benchmark.py \
  samples/windows/x64/test_control_flow_x64_O0.exe \
  --limit 30 \
  --timeout 300
```

## 생성 아티팩트

- `fission_full.json`: Fission 전체 디컴파일 원본 JSON
- `ghidra_full.json`: pyghidra 전체 디컴파일 원본 JSON
- `benchmark_summary.json`: 메타데이터와 함수별 비교 결과
- `benchmark_summary.md`: 사람이 읽기 쉬운 요약
- `fission_stdout.log`, `fission_stderr.log`: Fission 실행 로그

## 품질 지표

- 주소 기준 함수 매칭
- 성공률과 매칭 커버리지
- 함수별 raw / normalized similarity
- 전체 연결본 aggregate normalized similarity

## 속도 지표

- `init_sec`: 초기화 시간
- `total_decomp_sec`: 순수 디컴파일 시간 합
- `total_postprocess_sec`: Rust 후처리 시간 합
- `wall_clock_sec`: 전체 실행 시간

## 현재 검증 결과

- `test_control_flow_x64_O0.exe --limit 30`
  - Fission: `init 0.183s`, `decomp 4.470s`, `post 0.027s`, 성공 `25/30`
  - Ghidra: `init 1.412s`, `decomp 0.170s`, 성공 `30/30`
  - Fission synthetic failure `3`, explicit error `2`
- `putty.exe --limit 100`
  - Fission: `init 0.260s`, `decomp 157.037s`, 성공 `50/100`
  - Ghidra: `init 1.767s`, `decomp 3.140s`, 성공 `100/100`
  - 현재 병목은 준비보다 함수별 디컴파일 본체에 가까움
