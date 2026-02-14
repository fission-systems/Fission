---
description: 언어별 메타데이터 분석 기능 추가 워크플로우
---

# Add Language Support Workflow

새 프로그래밍 언어 메타데이터 분석 추가 가이드

---

## 📋 기존 구현 참조

### 현재 지원 언어

| 언어 | 파일 | 메타데이터 |
|------|------|-----------|
| Swift | `loader/macho/apple.rs` | `__swift5_fieldmd`, `__swift5_reflstr` |
| ObjC | `loader/macho/apple.rs` | `__objc_classlist`, ivar |
| Go | `loader/golang.rs` | `.gopclntab`, `.rodata` 타입 |
| C/C++ | `loader/dwarf.rs` | DWARF 디버그 섹션 |

### 참조 파일 확인

// turbo

```bash
ls -la crates/fission-loader/src/loader/
```

---

## 🏗️ 구현 단계

### 1단계: 분석기 파일 생성

```bash
# 새 분석기 파일 생성
touch crates/fission-loader/src/loader/새언어.rs
```

### 2단계: 모듈 등록

`loader/mod.rs`에 추가:

```rust
pub mod 새언어;
```

### 3단계: 기본 구조 작성

```rust
//! 새언어 메타데이터 분석기

use crate::loader::{LoadedBinary, FunctionInfo};
use crate::prelude::*;

pub struct NewLangAnalyzer<'a> {
    binary: &'a LoadedBinary,
}

impl<'a> NewLangAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        Self { binary }
    }

    pub fn analyze(&self) -> Result<Vec<FunctionInfo>> {
        // 구현
    }
}
```

---

## 🔍 메타데이터 분석 패턴

### 섹션 찾기

```rust
let section = self.binary.sections.iter()
    .find(|s| s.name == "__특정섹션");
```

### 바이트 읽기

```rust
let data = self.binary.get_bytes(addr, size)?;
```

### 문자열 읽기

```rust
fn read_string_at(&self, addr: u64) -> Option<String> {
    let bytes = self.binary.get_bytes(addr, 256)?;
    let len = bytes.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[..len]).to_string())
}
```

---

## 🔗 로더 통합

### loader/mod.rs 수정

```rust
// 언어 감지 후 분석기 호출
if detection.language().map_or(false, |d| d.name == "새언어") {
    let analyzer = 새언어::NewLangAnalyzer::new(&binary);
    if let Ok(funcs) = analyzer.analyze() {
        // 함수 병합
    }
}
```

---

## ✅ 테스트

### 테스트 바이너리 생성

```bash
# 해당 언어로 테스트 바이너리 컴파일
```

### 분석 확인

// turbo

```bash
./target/release/fission --cli tests/binaries/test_newlang_bin info
./target/release/fission --cli tests/binaries/test_newlang_bin functions
```

---

## 📝 문서 업데이트

1. `docs/FEATURES.md`에 언어 지원 추가
2. `.agent/rules/guide.md`에 파일 위치 추가
