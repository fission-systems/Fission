---
trigger: glob
globs: ["crates/**/*.rs"]
---

# Rust 코드 작성 규칙

## 코드 스타일

1. **에러 처리**
   - `unwrap()` 대신 `?` 연산자 사용
   - 커스텀 에러는 `fission-core`의 `FissionError` 사용
   - `Result<T>` 타입 일관성 유지

2. **모듈 구조**
   - 새 파일 생성 시 `mod.rs`에 등록 필수
   - `pub use`로 외부 노출할 타입 re-export
   - `prelude.rs` 패턴 따르기

3. **네이밍 규칙**
   - 구조체: `PascalCase`
   - 함수/메서드: `snake_case`
   - 상수: `SCREAMING_SNAKE_CASE`
   - 모듈: `snake_case`

4. **문서화**
   - 공개 함수에 `///` 문서 주석
   - 모듈 상단에 `//!` 모듈 문서

## 의존성

- 크레이트 간 의존성 순환 금지
- `fission-core`는 다른 fission 크레이트에 의존하지 않음
- 외부 크레이트 추가 시 Cargo.toml 버전 명시

## 테스트

- 단위 테스트는 파일 하단 `#[cfg(test)]` 모듈에 작성
- 통합 테스트는 `tests/` 디렉토리에 작성
