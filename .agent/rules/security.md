---
trigger: manual
---

# 보안 코드 리뷰 가이드

이 규칙은 `/security` 명령으로 수동 호출됩니다.

## 체크리스트

### 메모리 안전성

- [ ] Buffer overflow 가능성 확인
- [ ] Use-after-free 패턴 확인
- [ ] 정수 오버플로우 확인
- [ ] Unsafe 블록 최소화 및 문서화

### 입력 검증

- [ ] 사용자 입력의 길이 제한
- [ ] 경로 순회 (../) 방지
- [ ] 바이너리 파싱 시 경계 검사
- [ ] 악성 바이너리 핸들링

### FFI 안전성

- [ ] NULL 포인터 체크
- [ ] 버퍼 크기 검증
- [ ] 메모리 해제 책임 명확화
- [ ] 예외가 FFI 경계를 넘지 않음

### 의존성

- [ ] `cargo audit` 실행
- [ ] 의존성 버전 고정
- [ ] 사용하지 않는 의존성 제거

## 실행 명령

```bash
# 의존성 취약점 검사
cargo install cargo-audit
cargo audit

# Clippy 보안 관련 린트
cargo clippy -- -W clippy::all -W clippy::pedantic

# unsafe 코드 확인
grep -r "unsafe" crates/ --include="*.rs"
```

## 바이너리 파싱 주의사항

1. **오프셋 검증**

   ```rust
   if offset + size > data.len() {
       return Err(FissionError::loader("Invalid offset"));
   }
   ```

2. **루프 제한**

   ```rust
   let max_iterations = 10000;
   for i in 0..count.min(max_iterations) {
       // ...
   }
   ```

3. **재귀 깊이 제한**

   ```rust
   fn parse(data: &[u8], depth: usize) -> Result<T> {
       if depth > 100 {
           return Err(FissionError::loader("Max depth exceeded"));
       }
       // ...
   }
   ```
