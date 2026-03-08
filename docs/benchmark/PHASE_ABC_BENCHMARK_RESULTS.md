# Phase A/B/C 성능 최적화 벤치마크 결과

Phase A(심볼 캐싱), B(병렬 디컴파일), C(Zero-copy P-code FFI) 적용 후 벤치마크 및 회귀 테스트 결과.

---

## 1. 회귀 테스트 (Phase C Flat vs JSON)

### 1.1 단위 테스트

`fission-pcode`에 다음 테스트 추가됨 (`crates/fission-pcode/src/pcode/types.rs`):

| 테스트 | 설명 |
|--------|------|
| `test_flat_roundtrip_equivalence` | `PcodeFunction` → `to_flat_bytes()` → `from_flat_bytes()` 시 원본과 100% 동일 |
| `test_flat_vs_json_optimization_equivalence` | JSON 파싱 vs Flat 파싱 후 블록·op·varnode 구조 동일 |

```bash
cargo test -p fission-pcode --lib
# 32 passed
```

### 1.2 Edge Case 검증

- Flat 포맷 파서: `FPCD` 매직, 버전, 블록/op 개수 검증
- `Truncated`, `BadMagic`, `TooShort` 등 오류 경로 단위 테스트로 검증 가능 (추가 권장)

---

## 2. 벤치마크 수치

### 2.1 putty.exe (--decomp-limit 100)

| 지표 | 값 |
|------|-----|
| Functions | 100 |
| total_decomp_sec | ~88.7s |
| total_postprocess_sec | ~0.09s |
| wall_clock (1-thread) | ~87.5s |
| Top 5 slowest (addr, decomp_sec) | 0x14000a120(15.1s), 0x140007da0(13.9s), 0x140001160(12.8s), 0x14000ded0(9.4s), 0x140001000(8.1s) |

### 2.2 test_control_flow_x64_O0.exe (--decomp-limit 30)

| 지표 | 값 |
|------|-----|
| Functions | 30 |
| wall_clock | ~4.7s |

---

## 3. RAYON_NUM_THREADS 스케일링

### 3.1 test_control_flow (30 functions)

| RAYON_NUM_THREADS | wall (real) | user | sys |
|-------------------|-------------|------|-----|
| 1 | 5.24s | 4.59s | 0.12s |
| 2 | 4.65s | 4.53s | 0.10s |
| 4 | 4.62s | 4.52s | 0.09s |
| 8 | 4.61s | 4.50s | 0.09s |

**해석:** 30개 함수 기준으로는 스레드 증가 시 약 1.1x 정도 개선에 그침. 함수 수가 적고 개별 함수가 상대적으로 작음.

### 3.2 putty.exe (100 functions) — Before: 주소순 청크

| RAYON_NUM_THREADS | wall (real) | user | sys |
|-------------------|-------------|------|-----|
| 1 | 87.51s | 85.29s | 1.89s |
| 2 | 89.25s | 86.29s | 2.40s |
| 4 | 88.86s | 86.28s | 2.21s |
| 8 | 89.12s | 86.26s | 2.31s |

**원인:** 무거운 함수가 낮은 주소에 몰려 첫 번째 청크(메인 스레드)에 집중됨.

### 3.3 putty.exe (100 functions) — After: 라운드로빈 분배 (2026-03)

| RAYON_NUM_THREADS | wall (real) | user | sys | 스케일 |
|-------------------|-------------|------|-----|--------|
| 1 | 90.36s | 86.84s | 2.00s | 1.0x |
| 2 | 97.93s | 94.57s | 2.73s | 0.9x |
| 4 | 72.24s | 85.64s | 2.65s | **1.25x** |
| 8 | 41.97s | 50.74s | 0.82s | **2.15x** |

**해석:** 라운드로빈으로 무거운 함수를 워커에 분산 → 8코어에서 **~42초** (기존 89초 대비 **2.1x 개선**).

---

## 4. 권장 후속 작업 (우선순위)

| 순서 | 항목 | 상태 |
|------|------|------|
| 1 | **청크 분배 개선 (라운드로빈)** | ✅ 완료 (2026-03) — putty 8코어 2.1x 개선 |
| 2 | **Step 4: FFI 안정성** | ✅ 완료 — decomp_destroy, decomp_set_gdt, decomp_set_feature, decomp_load_fid_db, decomp_get_fid_match에 try/catch 추가 |
| 3 | **프로파일링** | 권장 — Flamegraph로 C++ 내부 전역 락·병목 확인 |
| 4 | **Phase D Arena** | 보류 — 현재 병목 확인 후 진행 |

---

## 5. 벤치마크 실행 방법

```bash
# Fission 단독 (빠른 검증)
export DYLD_LIBRARY_PATH="$(pwd)/target/release:$DYLD_LIBRARY_PATH"
./target/release/fission_cli samples/windows/x64/putty.exe --decomp-all --benchmark --ghidra-compat --profile balanced --decomp-limit 100 -o /tmp/fission.json

# Fission vs Ghidra 비교 (pyghidra 필요)
python3 scripts/test/batch_benchmark/full_decomp_benchmark.py samples/windows/x64/putty.exe --limit 100
```

---

## 6. SIGSEGV (139) 대응 현황

| 항목 | 상태 |
|------|------|
| TypePropagator value_is_pointer (dangling Datatype*) | ✅ 수정 |
| decomp_destroy 직렬화 (TEARDOWN_LOCK) | ✅ 적용 |
| 잔여 간헐적 크래시 (8스레드) | ⚠️ 발생 가능 |
| 권장 | `RAYON_NUM_THREADS=4` 이하로 안정성 우선 |

---

*최종 갱신: 2026-03-08*
