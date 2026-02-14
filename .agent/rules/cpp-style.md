---
trigger: glob
globs: ["ghidra_decompiler/**/*.cpp", "ghidra_decompiler/**/*.cc", "ghidra_decompiler/**/*.h"]
---

# C++ 코드 작성 규칙

## 디렉토리 구조

| 위치 | 용도 |
|------|------|
| `include/fission/` | 헤더 파일 |
| `src/` | 구현 파일 |
| `src/ffi/` | FFI 인터페이스 |
| `decompile/` | Ghidra 원본 (수정 금지) |

## 코드 스타일

1. **네이밍**
   - 클래스: `PascalCase`
   - 함수: `snake_case` 또는 `camelCase`
   - 멤버 변수: `m_` 접두사 또는 끝에 `_`
   - 상수: `SCREAMING_SNAKE_CASE`

2. **네임스페이스**
   - `fission::` 네임스페이스 사용
   - 하위 네임스페이스: `fission::analysis::`, `fission::decompiler::`

3. **메모리 관리**
   - raw 포인터 대신 스마트 포인터 선호
   - 예외 안전성 고려

## FFI 규칙

1. **함수 선언** (`libdecomp_ffi.h`)
   - `DECOMP_API` 매크로 사용
   - `extern "C"` 링키지
   - Doxygen 스타일 문서화

2. **에러 처리**
   - `DecompError` enum 반환
   - 예외는 FFI 경계에서 catch

## 빌드

```bash
cd ghidra_decompiler/build
cmake ..
make -j
```

변경 후 반드시 빌드 확인!
