---
description: 새 디컴파일러 기능 추가 워크플로우
---

# Add Feature Workflow

Fission에 새 기능을 추가하는 단계별 가이드

---

## 📋 1단계: 기존 구현 확인

### 문서 확인

// turbo

```bash
cat docs/FEATURES.md
```

### 관련 키워드 검색

```
grep_search로 다음 검색:
- 관련 기능 키워드 (예: "bookmark", "rename")
- 유사 기능 (예: "save", "store")
```

### 검색 위치

- `/crates` (Rust 코드)
- `/ghidra_decompiler/src` (C++ 코드)
- `vendor/` 결과는 무시

---

## 🏗️ 2단계: 구현 위치 결정

### Rust 전용 기능

| 기능 유형 | 크레이트 |
|----------|---------|
| 바이너리 파싱 | `fission-loader` |
| 분석/최적화 | `fission-analysis` |
| GUI | `fission-ui` |
| CLI | `fission-cli` |

### Ghidra 연동 필요

| 기능 유형 | 위치 |
|----------|------|
| 디컴파일 수정 | `ghidra_decompiler/src/decompiler/` |
| 타입 분석 | `ghidra_decompiler/src/analysis/` |
| 후처리 | `ghidra_decompiler/src/processing/` |
| FFI 바인딩 | `fission-ffi` + `libdecomp_ffi.cpp` |

---

## ✍️ 3단계: 코드 작성

### Rust 파일 추가

1. 새 파일 생성: `src/새기능.rs`
2. `mod.rs`에 모듈 등록: `pub mod 새기능;`
3. 필요시 `prelude.rs`에 re-export

### C++ 파일 추가

1. 헤더: `include/fission/category/NewFeature.h`
2. 소스: `src/category/NewFeature.cc`
3. CMakeLists.txt에 파일 추가

### FFI 연동 추가

1. `libdecomp_ffi.h`에 함수 선언
2. `libdecomp_ffi.cpp`에 구현
3. `decomp.rs`에 extern 바인딩
4. Rust 래퍼 함수 작성

---

## 🔨 4단계: 빌드 및 테스트

### C++ 빌드 (변경시)

```bash
cd ghidra_decompiler/build && make -j
```

### Rust 빌드

// turbo

```bash
cargo build
```

### 테스트 실행

// turbo

```bash
cargo test
```

---

## 📝 5단계: 문서 업데이트

### FEATURES.md 업데이트

```bash
# 새 기능을 docs/FEATURES.md에 추가
```

### CHANGELOG.md 업데이트

```bash
# 변경사항 기록
```

---

## ✅ 6단계: 커밋

### 변경사항 확인

// turbo

```bash
git status
git diff
```

### 커밋

```bash
git add -A
git commit -m "feat: 기능 설명"
```

### 푸시

```bash
git push origin main
```
