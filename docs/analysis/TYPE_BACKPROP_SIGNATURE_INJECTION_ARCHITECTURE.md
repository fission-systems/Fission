# 타입 역전파(Type Back-propagation) — 시그니처 주입 아키텍처 설계

## 1. 개요

**목표:** `fission-signatures`의 함수 시그니처(인자 타입, 반환 타입)를 C++ Ghidra 코어에 주입하여, `MessageBoxA(local_1, local_2, ...)`와 같은 호출부에서 `local_1`을 `HWND`, `local_2`를 `LPCSTR`로 역추론하게 만든다.

**핵심 원리:** Ghidra는 이미 **캘리(callee) 프로토타입이 알려져 있으면** `ActionInferTypes`와 `TypePropagator`가 P-code 데이터 플로우를 따라 **호출자(caller) 인자 타입을 자동 역전파**한다. 따라서 **외부 시그니처를 코어에 주입**하는 것이 유일한 진입점이다.

---

## 2. 현재 아키텍처 분석

### 2.1 Rust 측: fission-signatures

| 구성요소 | 경로 | 역할 |
|----------|------|------|
| `WinApiDatabase` | `src/win_api.rs` | `HashMap<String, ApiSignature>` 형태의 전역 DB |
| `ApiSignature` | | `name`, `return_type`, `params: Vec<ParamInfo>` |
| `ParamInfo` | | `name`, `type_name`, `enum_group` (선택) |
| JSON 소스 | `data/win_api/*.json` | user32, kernel32, ntdll, advapi32, ws2_32, winhttp, wininet, shell32, bcrypt |

**예시 (user32.json):**
```json
{
  "name": "MessageBoxA",
  "return_type": "int",
  "params": [
    { "name": "hWnd", "type_name": "HWND" },
    { "name": "lpText", "type_name": "LPCSTR" },
    { "name": "lpCaption", "type_name": "LPCSTR" },
    { "name": "uType", "type_name": "UINT", "enum_group": "MB_TYPE" }
  ]
}
```

**FFI 관점:** 현재 `fission-signatures`는 Rust 전용이며, C++와 직접 연동되는 코드는 없다. 시그니처를 넘기려면:
- JSON 문자열 직렬화 (가장 단순)
- C ABI 구조체 배열 (복잡, 문자열 수명 관리 필요)

---

### 2.2 C++ 측: PrototypeEnforcer + Ghidra

| 구성요소 | 경로 | 역할 |
|----------|------|------|
| `PrototypeEnforcer` | `src/types/PrototypeEnforcer.cc` | 함수명 → `PrototypePieces` → `Architecture::setPrototype` |
| `win_api_db` | (정적 변수) | `map<string, WinApiSignatureDef>` — **현재 `.txt` 파일에서 로드** |
| `build_win_api_prototype()` | | `win_api_db`에서 함수명으로 조회 → `PrototypePieces` 생성 |
| `resolve_winapi_type()` | | 타입명(`HWND`, `LPCSTR` 등) → `TypeFactory`의 `Datatype*` |

**현재 시그니처 소스:**
- `win_api_signatures.txt`: `name|return_type|p1:t1,p2:t2` (파이프/콤마 구분)
- **파일이 저장소에 없음** — `load_win_api_db()` 실패 시 `win_api_db`는 비어 있음
- 대체: `build_builtin_prototype()` (printf, malloc, strcmp 등 하드코딩)

**주입 시점 (DecompilationCore.cpp:547-574):**
```
1. proto_enforcer.enforce_iat_prototypes(ctx->symbols)  // 모든 IAT 심볼
2. proto_enforcer.enforce_single_prototype(arch, addr, func_name)  // 현재 함수
3. ctx->arch->clearAnalysis(fd)
4. TypePropagator::seed_before_action(fd)  // 시드
5. current_action->perform(*fd)  // ActionInferTypes 등 수행
```

**핵심:** `Architecture::setPrototype(const PrototypePieces& pieces)`는 **함수 이름**으로 `symboltab`에서 `Funcdata`를 찾아 `fd->getFuncProto().setPieces(pieces)`를 호출한다. 따라서 **시그니처는 반드시 심볼명으로 조회**되며, `ctx->symbols`(addr→name)에 있는 이름과 일치해야 한다.

---

### 2.3 FFI 경로 (fission-cli ↔ C++)

| FFI 함수 | 역할 |
|----------|------|
| `decomp_load_binary` | 바이너리 로드 |
| `decomp_add_symbol` | 단일 심볼 (addr, name) 추가 |
| `decomp_add_symbols_batch` | 배치 심볼 추가 |
| `decomp_decompile` | 특정 주소 디컴파일 |

**흐름:** Rust 로더가 IAT/FID 심볼을 추출 → `decomp_add_symbol`로 전달 → C++ `ctx->symbols`에 저장 → 디컴파일 시 `PrototypeEnforcer`가 `ctx->symbols`의 이름으로 프로토타입 적용.

---

## 3. 브릿지 설계 옵션

### 옵션 A: JSON 블롭 FFI (권장)

**방식:** Rust가 `fission-signatures`의 시그니처를 JSON으로 직렬화 → `decomp_set_signatures_json(ctx, json_str)` 같은 새 FFI로 C++에 전달.

**장점:**
- 구현 단순, 문자열만 넘김
- fission-signatures와 1:1 매핑
- 런타임에 JSON 경로 의존 없음

**단점:**
- C++ 쪽에 JSON 파서 필요 (현재 코드베이스에 JSON 사용 여부 확인 필요)

---

### 옵션 B: 공유 JSON 파일 경로

**방식:** `decomp_set_signature_path(ctx, path)`로 `crates/fission-signatures/data/win_api/` 경로를 넘기고, C++가 해당 디렉터리의 `*.json`을 로드.

**장점:**
- C++가 기존 `load_win_api_db`와 유사하게 파일 읽기만 하면 됨
- FFI 변경 최소화

**단점:**
- 경로 해석(상대/절대) 이슈
- 빌드/패키징 시 JSON 경로 항상 접근 가능해야 함

---

### 옵션 C: C ABI 구조체 배열

**방식:** `decomp_set_signatures(ctx, SignatureEntry* entries, size_t count)` 형태로, 각 엔트리가 `(name, return_type, param_names[], param_types[], ...)` 등.

**장점:**
- 파싱 오버헤드 없음

**단점:**
- 문자열 수명/소유권 관리 복잡
- Rust→C 문자열 변환 로직 다수
- 시그니처 스키마 변경 시 FFI 계약 전면 수정

---

## 4. 권장 설계: 옵션 A (JSON 블롭) + C++ 파서

### 4.1 전제 및 C++ JSON 인프라

- **이미 사용 가능:** `fission/utils/json_utils.h`에 다음이 구현되어 있음:
  - `extract_json_string(json, key)` — 객체에서 문자열 추출
  - `extract_json_array(json_content)` — 배열 파싱, 각 요소를 JSON 문자열로 반환 (`FunctionMatcher::load_signatures`에서 동일 패턴 사용)
- 중첩 구조 `params: [...]`는 `extract_json_string(obj, "params")`로 `[...]` 부분을 얻은 뒤 `extract_json_array`로 재파싱 가능.

### 4.2 직렬화 포맷 (fission-signatures 호환)

Rust `ApiSignature`를 담는 JSON 배열:
```json
[
  {
    "name": "MessageBoxA",
    "return_type": "int",
    "params": [
      { "name": "hWnd", "type_name": "HWND" },
      { "name": "lpText", "type_name": "LPCSTR" },
      ...
    ]
  },
  ...
]
```

`data/win_api/*.json` 파일들을 **concatenate**하거나, `include_str!`로 읽어 하나의 JSON 배열로 합친 뒤 FFI로 전달.

### 4.3 FFI 확장

**새 함수 (libdecomp_ffi.h):**
```c
/**
 * Inject function signatures for type inference.
 * Call before or after load_binary; signatures apply to all subsequent decompilations.
 * Overwrites any previously set signatures for the same function names.
 *
 * @param ctx Decompiler context
 * @param json_signatures JSON array of ApiSignature (see fission-signatures win_api format)
 * @return 0 on success, non-zero on parse error
 */
DECOMP_API int decomp_set_signatures_json(
    DecompContext* ctx,
    const char* json_signatures
);
```

**Rust 측 (fission-ffi or fission-cli):**
```rust
pub fn set_signatures_json(ctx: *mut DecompContext, sigs: &[ApiSignature]) -> Result<(), DecompError> {
    let json = serde_json::to_string(sigs)?;
    let json_c = CString::new(json)?;
    let ret = unsafe { decomp_set_signatures_json(ctx, json_c.as_ptr()) };
    if ret == 0 { Ok(()) } else { Err(...) }
}
```

### 4.4 C++ 구현 흐름

1. **`DecompContext` 확장**
   - `std::map<std::string, WinApiSignatureDef> injected_signatures;` 추가
   - `decomp_set_signatures_json` 호출 시 파싱하여 이 맵에 저장

2. **PrototypeEnforcer 수정**
   - `win_api_db` 대신 (또는 병합) `DecompContext::injected_signatures` 사용
   - `enforce_iat_prototypes` 호출 시 `ctx`를 넘기거나, `Architecture`와 함께 `ctx` 참조 전달

3. **JSON 파싱**
   - `json_utils` 또는 경량 라이브러리로 배열 파싱
   - 각 객체에서 `name`, `return_type`, `params` 추출 → `WinApiSignatureDef`로 변환

---

## 5. 구현 단계 (Phase별)

| Phase | 작업 | 담당 레이어 | 상태 |
|-------|------|-------------|------|
| P1 | `DecompContext`에 `injected_signatures` 추가 | C++ | ✅ 완료 |
| P2 | `decomp_set_signatures_json` FFI 구현 + JSON 파싱 | C++ | ✅ 완료 |
| P3 | `PrototypeEnforcer`가 `injected_signatures` 우선 사용하도록 수정 | C++ | ✅ 완료 |
| P4 | `fission-ffi`에 `set_signatures_json` 래퍼 추가 | Rust | ✅ 완료 |
| P5 | `prepare_native_decompiler_for_binary`에서 `WIN_API_DB` 시그니처 주입 | Rust | ✅ 완료 |

### P1~P3 구현 요약

- **InjectedSignature.h**: `InjectedParamInfo`, `InjectedApiSignature` 공유 구조체
- **json_utils**: `extract_json_array_for_key` 추가 (params 배열 파싱)
- **decomp_set_signatures_json**: fission-signatures JSON 포맷 파싱 후 `ctx->injected_signatures`에 저장
- **PrototypeEnforcer**: `build_injected_prototype` 추가, `enforce_single_prototype`에서 injected 우선 적용

---

## 6. FID + 시그니처 연동

- **FID 매칭**으로 `state.fid_function_names`(addr→name)에 `sprintf`, `fopen` 등이 채워짐.
- `fission-cli` 흐름에서는 이 정보가 `decomp_add_symbol`로 넘어가 `ctx->symbols`에 들어감.
- `decomp_set_signatures_json`으로 **CRT/라이브러리 시그니처**를 미리 주입해 두면, FID로 복원된 이름에 대해서도 프로토타입이 적용된다.
- `fission-signatures`의 `FunctionSignature`(패턴 매칭용)와 `ApiSignature`(타입 정보)는 **다른 목적**이지만, `ApiSignature`를 Win API + CRT 확장하면 FID 결과와 자연스럽게 연동된다.

---

## 7. 한계 및 참고사항

- **Ghidra `setPrototype`의 제약:** 함수는 **이미 symbol table에 등록**되어 있어야 한다. 따라서 `decomp_add_symbol`(또는 이에 상응하는 심볼 등록)이 **먼저** 호출된 뒤, 시그니처 주입이 이루어져야 한다.
- **호출 규칙:** `ProtoModel`(__cdecl, __stdcall, __fastcall)은 `Architecture`가 플랫폼에 맞게 선택. 시그니처에 calling convention을 포함할 경우, `PrototypePieces.model`에서 지정 가능.
- **타입 해상도:** `resolve_winapi_type`가 `HWND`, `LPCSTR` 등을 `void*`/`char*`로 매핑. `fission-signatures`의 타입명이 이 로직과 호환되면 그대로 사용 가능.

---

## 8. 다음 액션

1. **C++ JSON 파싱 확인:** `fission/utils/json_utils.h` 및 기존 사용처 검토.
2. **`win_api_signatures.txt` vs JSON:** C++ `load_win_api_db` 포맷과 JSON 스키마 매핑 테이블 작성.
3. **P1–P3** 구현 후, `MessageBoxA` 등으로 타입 역전파 동작 검증.
