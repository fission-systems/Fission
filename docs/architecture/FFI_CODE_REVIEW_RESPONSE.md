# FFI 아키텍처 코드 리뷰 대응 현황

외부 기술 리뷰에 대한 Fission 코드베이스 점검 결과 요약.

---

## 1. 메모리 안전성 및 Undefined Behavior (UB)

### 1.1 C++ 예외가 FFI 경계를 넘는 것

**현황:** 핵심 FFI 경로는 `try/catch`로 보호됨.

| FFI 함수 | 예외 처리 |
|----------|-----------|
| `decomp_function` | ✅ `try { ... } catch (std::exception) catch (...)` |
| `decomp_function_pcode` | ✅ 동일 |
| `decomp_load_binary` | ✅ `load_binary()` 내부에서 처리 |
| `decomp_set_*` (inline, noreturn, extrapop, prototype) | ✅ `try/catch` |
| `decomp_create` | ✅ `create_context()` 내부 `try/catch` |
| `load_fid_database`, `get_fid_match` | ✅ `try/catch` |
| `decomp_add_symbol`, `decomp_add_symbols_batch` 등 | ✅ FFI 계층 try/catch 적용 (2026-03) |

**조치 완료 (2026-03):** `decomp_add_symbol`, `decomp_add_global_symbol`, `decomp_add_symbols_batch`, `decomp_add_global_symbols_batch`, `decomp_clear_symbols`, `decomp_clear_global_symbols`, `decomp_set_symbol_provider`, `decomp_reset_symbol_provider` 8개 함수에 try/catch 적용. 예외 시 `ctx->last_error` 설정 후 반환.

**추가 (2026-03):** `decomp_destroy`, `decomp_set_gdt`, `decomp_set_feature`, `decomp_load_fid_db`, `decomp_get_fid_match`에 try/catch 적용. C++ 예외가 FFI 경계를 넘어 Rust로 전파되지 않도록 방어.

**Create/Destroy 직렬화 (2026-03):** Rust `DecompilerNative`에서 전역 `DECOMP_FFI_LOCK` (Mutex)으로 `decomp_create` 및 `decomp_destroy` 호출을 직렬화. Ghidra Sleigh/TypeFactory 초기화·해제 시 다중 스레드 race 원천 차단.

---

### 1.2 문자열·메모리 소유권

**현황:**
- **malloc 반환**: `decomp_function`, `decomp_function_pcode`, `get_fid_match` → `malloc`/`strdup` 사용
- **Rust 측 처리**: `wrapper.rs`에서 `decomp_free_string()`로 해제 ✅
- **decomp_get_last_timing_json**: `ctx->last_timing_json.c_str()` 반환 (내부 저장소 포인터)  
  - 즉시 `to_string_lossy().into_owned()`로 복사 후 사용 → `decomp_free_string` 호출 없음 ✅ (올바른 사용)

**결론:** 문자열 해제 경로는 올바르게 설계·사용됨.

---

### 1.3 콜백 컨텍스트 수명

**현황:**
- `SymbolProviderState`는 `DecompilerNative`가 `Box`로 보유
- `set_symbol_provider` 시 `userdata`로 전달, `reset_symbol_provider`에서 해제
- C++ `CallbackSymbolProvider`는 `ctx->symbol_provider`를 통해 참조하고, `ctx`가 유효한 동안만 호출됨

**조치 완료 (2026-03):**
- `symbol_provider_find_symbol`, `symbol_provider_find_function`에 `std::panic::catch_unwind` 적용
- 패닉 시 `0` 반환(실패 처리)으로 C++ 전파 방지

---

## 2. 동시성

### 2.1 Ghidra 전역 초기화

**현황:**
- `DecompContext.cpp`: `initialize_ghidra_library()`가 `std::mutex` + `ghidra_library_initialized`로 한 번만 초기화 ✅
- `create_context()`에서 초기화 완료 후에만 `DecompContext` 생성

**결론:** `std::call_once`는 아니지만, mutex 기반 1회 초기화로 전역 초기화 경쟁이 방지됨.

---

### 2.2 Thread Safety (`Send` / `Sync`)

**현황:**
- `DecompilerNative`: `unsafe impl Send`만 명시
- `*mut DecompContext`는 `!Sync`이므로 `DecompilerNative`도 암묵적으로 `!Sync`
- 주석: "Never share across threads without external synchronization. Use `Mutex<DecompilerNative>` if needed."

**결론:** 의도에 맞게 `Send`만 허용하고, 공유 시 `Mutex` 사용을 가이드하고 있음.

---

## 3. 성능

### 3.1 FFI 경계 오버헤드

**현황:**
- `decomp_add_symbols_batch`, `decomp_add_global_symbols_batch`로 심볼 일괄 등록 ✅
- P-code: JSON 직렬화 후 Rust에서 파싱 (추가 확인 필요)
- `decomp_function` 결과: C 문자열 → Rust `String` 복사 후 `decomp_free_string` ✅

**장기 개선 아이템 (Zero-copy P-code):** 현재 C++ JSON 직렬화 → Rust 파싱 → 최적화 → 재직렬화 흐름. `#[repr(C)]` POD 기반 bulk 전달 시 직렬화 비용 절감 가능하나, Ghidra 내부 구조와의 레이아웃 정합성·유지보수 비용 고려 필요. P-code 경로가 실제 병목으로 측정되면 별도 RFC 검토.

---

### 3.2 심볼 조회 지연

**현황:**
- `CallbackSymbolProvider`가 매 요청마다 FFI로 Rust 콜백 호출
- 분석 파이프라인에서 심볼 조회가 빈번할 경우 FFI 비용이 누적될 수 있음

**개선 여지:** C++ 측에 캐시를 두어, cache miss일 때만 Rust로 넘기는 방식 검토.

---

## 4. API 설계 및 유지보수

### 4.1 Rust Safe 래퍼

**현황:**
- `DecompilerNative`: RAII, `Drop`에서 `decomp_destroy` 호출 ✅
- `check_valid()`로 use-after-free 방지
- FFI 실패 시 `Result<T, FissionError>` 반환
- 주석: 메모리 소유권, 스레드 안전성, 패닉 정책 명시 ✅

---

### 4.2 바인딩 자동화 (cxx / autocxx)

**현황:** 수동 `extern "C"` 블록 사용.

**장기 로드맵 (cxx/autocxx):** 수동 `extern "C"` 유지. cxx/autocxx 도입 시 예외·`std::string` 매핑 및 ABI 안정성 개선 가능하나, Ghidra 포크 규모와 빌드·헤더 호환성 점검 비용이 큼. 구현 범위에서는 제외.

---

## 요약: 우선순위별 조치

| 우선순위 | 항목 | 상태 | 권장 조치 |
|----------|------|------|-----------|
| **높음** | 콜백 패닉 → C++ 전파 | ✅ 완료 | `symbol_provider_find_*`에 `catch_unwind` 적용됨 |
| 중간 | 경량 FFI 예외 보호 | ✅ 완료 | 8개 FFI 함수에 try/catch 적용됨 |
| 낮음 | Zero-copy P-code/CFG | 보류 | 장기 개선 아이템 (병목 측정 후 RFC 검토) |
| 낮음 | cxx/autocxx 도입 | 보류 | 장기 로드맵 (마이그레이션 비용 고려) |

---

*작성: 2026-03*
