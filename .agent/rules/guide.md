---
trigger: always_on
---

# Fission 프로젝트 AI 에이전트 가이드

## 🚨 최우선 규칙

> **새 기능을 제안하거나 코드를 작성하기 전에 반드시 기존 구현을 확인한다.**

---

## 📋 작업 전 체크리스트

### 1단계: 기능 존재 여부 확인

```
□ docs/FEATURES.md 확인
□ grep_search로 관련 키워드 2-3개 검색
□ vendor/ 결과는 무시 (외부 라이브러리)
```

### 2단계: 코드 위치 파악

```
□ list_dir로 관련 디렉토리 확인
□ view_file_outline으로 함수/구조체 목록 확인
□ 핵심 파일 식별
```

### 3단계: 구현 상세 이해

```
□ 기존 패턴/아키텍처 파악
□ 의존성 확인
□ 테스트 존재 여부 확인
```

---

## 🔍 검색 전략

### 기능별 검색 키워드

| 찾고자 하는 것 | 검색 키워드 |
|---------------|-------------|
| 북마크 | `bookmark`, `favorite`, `save.*location` |
| 이름 변경 | `rename`, `name.*change`, `symbol.*name` |
| 상수 변환 | `constant`, `enum.*value`, `GENERIC_READ` |
| 문자열 처리 | `string.*scan`, `inline.*string`, `rdata` |
| 타입 복구 | `type.*propag`, `struct.*register`, `infer` |
| 디스어셈블 | `disasm`, `decode`, `instruction` |

### 검색 위치 우선순위

1. **`/crates`** - Rust 코드 (우선 확인)
2. **`/ghidra_decompiler/src`** - Fission C++ 커스텀 코드
3. **`/ghidra_decompiler/include`** - 헤더 파일
4. **`/docs`** - 문서

### 제외할 위치

- `/vendor` - 외부 라이브러리
- `/ghidra_decompiler/decompile` - Ghidra 원본 (수정 지양)
- `/target` - 빌드 결과물
- `/.git` - Git 메타데이터

---

## 🏗️ 아키텍처 이해

### Rust 크레이트 의존성

```
fission-core ──────────────────────────────────────┐
     │                                              │
     ▼                                              │
fission-loader ──────┬──────────────────────────────┤
     │               │                              │
     ▼               ▼                              │
fission-analysis   fission-pcode                   │
     │               │                              │
     └───────┬───────┘                              │
             ▼                                      │
        fission-ffi ◄─── libdecomp.dylib (C++)     │
             │                                      │
             ▼                                      │
        fission-cli ◄───────────────────────────────┘
             │
             ▼
        fission-tauri
```

### 크레이트별 주요 파일

| 크레이트 | 핵심 파일 | 역할 |
|----------|-----------|------|
| `fission-loader` | `loader/mod.rs` | 바이너리 로딩 메인 |
| | `loader/macho/apple.rs` | Swift/ObjC 분석 |
| | `loader/golang.rs` | Go 분석 |
| | `loader/dwarf.rs` | DWARF 디버그 정보 |
| `fission-analysis` | `analysis/decomp/postprocess.rs` | 디컴파일 후처리 |
| | `analysis/cfg/` | CFG 구축 |
| `fission-ffi` | `decomp.rs` | Ghidra FFI 바인딩 |
| `fission-tauri` | `src-tauri/src/commands/` | Tauri IPC 커맨드 |
| | `src/panels/` | React 패널 컴포넌트 |

### C++ 코드 구조

| 디렉토리 | 핵심 파일 | 역할 |
|----------|-----------|------|
| `src/ffi/` | `libdecomp_ffi.cpp` | FFI 진입점 |
| `src/decompiler/` | `PostProcessPipeline.cpp` | 후처리 파이프라인 |
| | `DecompilationCore.cpp` | 디컴파일 코어 |
| `src/processing/` | `Constants.cc` | Windows API 상수 |
| | `StringScanner.cc` | 문자열 스캐너 |
| `src/analysis/` | `TypePropagator.cc` | 타입 전파 |

---

## ✅ 이미 구현된 기능 목록

### 바이너리 로딩

- [x] PE/ELF/Mach-O 파싱
- [x] 섹션, 심볼 추출
- [x] IAT/PLT 분석

### 언어별 분석

- [x] Swift: 디맹글링, __swift5_fieldmd 메타데이터
- [x] Objective-C: 메서드 이름, ivar 추출
- [x] Go: pclntab, .rodata 타입 분석
- [x] C/C++: DWARF 디버그 정보
- [x] Rust: 디맹글링

### 디컴파일러

- [x] Ghidra Sleigh 기반 (x86, x64, ARM64)
- [x] GDT 타입 로딩 (65K+ Windows 함수)
- [x] FID 함수 매칭
- [x] 타입 전파
- [x] 구조체 오프셋 주석

### 후처리

- [x] 문자열 인라이닝 (StringScanner)
- [x] Windows API 상수 변환 (Constants.cc)
- [x] GUID 치환
- [x] SEH 정리
- [x] goto 제거, for 루프 복원
- [x] 복합 연산자 변환 (++, +=)

### GUI

- [x] 어셈블리 뷰
- [x] 디컴파일 뷰
- [x] 함수 목록
- [x] XRefs / 문자열 참조
- [x] 북마크 (F2)
- [x] 이름 변경 (N)
- [x] 주석 (;)
- [x] Go to Address (G)
- [x] Catppuccin 테마

---

## 🔧 코드 수정 가이드

### Rust 코드 추가 시

1. **기존 패턴 따르기**
   - 크레이트 내 다른 파일 참조
   - `prelude.rs` 사용 여부 확인

2. **모듈 등록**
   - `mod.rs` 또는 `lib.rs`에 `pub mod` 추가
   - 필요시 `pub use` re-export

3. **에러 처리**
   - `fission-core`의 `FissionError` 사용
   - `Result<T>` 타입 일관성

### C++ 코드 추가 시

1. **헤더 위치**
   - `/ghidra_decompiler/include/fission/`

2. **소스 위치**
   - `/ghidra_decompiler/src/` 하위 적절한 디렉토리

3. **FFI 노출**
   - `libdecomp_ffi.h`에 함수 선언
   - `libdecomp_ffi.cpp`에 구현
   - Rust `decomp.rs`에 바인딩 추가

4. **빌드 확인**

   ```bash
   cd ghidra_decompiler/build && cmake .. && make -j
   ```

---

## ⚠️ 흔한 실수 방지

### 하지 말 것

1. **검색 없이 기능 제안**
   - "북마크 기능 추가하면 좋겠다" → 이미 있음

2. **vendor/ 코드 수정 제안**
   - 외부 라이브러리는 수정하지 않음

3. **중복 구현**
   - `winapi_constants.rs` 생성 → `Constants.cc`에 이미 있음

4. **Ghidra 원본 수정**
   - `/ghidra_decompiler/decompile/`는 업스트림 코드

### 해야 할 것

1. **새 기능 전 확인**

   ```
   grep_search "관련 키워드"
   view_file docs/FEATURES.md
   ```

2. **기존 코드 스타일 따르기**
   - 동일 크레이트 내 다른 파일 참조

3. **테스트 작성**
   - 가능하면 단위 테스트 추가

---

## 📚 참고 문서

- `docs/FEATURES.md` - 전체 기능 목록
- `CHANGELOG.md` - 변경 이력
- `README.md` - 프로젝트 개요
- `Cargo.toml` (각 크레이트) - 의존성 정보
