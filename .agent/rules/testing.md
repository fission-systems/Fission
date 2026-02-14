---
trigger: model_decision
description: 테스트 코드 작성 시 적용
---

# 테스트 작성 규칙

## 테스트 위치

| 종류 | 위치 |
|------|------|
| 단위 테스트 | 파일 하단 `#[cfg(test)]` 모듈 |
| 통합 테스트 | `crates/<name>/tests/` |
| 바이너리 테스트 | `tests/binaries/` |

## 단위 테스트 구조

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_기능_정상케이스() {
        // Arrange
        let input = ...;
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn test_기능_에러케이스() {
        let result = function_under_test(invalid_input);
        assert!(result.is_err());
    }
}
```

## 테스트 네이밍

- `test_<기능>_<시나리오>`
- 예: `test_parse_swift_metadata_valid_section`

## 테스트 바이너리

### 위치

```
tests/binaries/
├── test_swift.swift + test_swift_bin
├── test_go.go + test_go_bin
├── test_objc.m + test_objc_bin
└── test_rust.rs + test_rust_bin
```

### 새 테스트 바이너리 추가

1. 소스 파일 작성
2. 컴파일
3. `tests/binaries/README.md` 업데이트

## 실행

```bash
# 전체 테스트
cargo test

# 특정 크레이트
cargo test -p fission-loader

# 특정 테스트
cargo test test_name -- --nocapture
```
