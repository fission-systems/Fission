---
trigger: model_decision
description: 커밋 메시지 작성 시 적용
---

# Git 커밋 규칙

## 커밋 메시지 형식

```
<type>: <subject>

<body>

<footer>
```

## 타입

| 타입 | 설명 |
|------|------|
| `feat` | 새 기능 |
| `fix` | 버그 수정 |
| `docs` | 문서 변경 |
| `style` | 코드 스타일 (동작 변경 없음) |
| `refactor` | 리팩토링 |
| `test` | 테스트 추가/수정 |
| `chore` | 빌드/설정 변경 |
| `perf` | 성능 개선 |

## 예시

```
feat: Swift 메타데이터 기반 타입 복구

## 변경 사항
- __swift5_fieldmd 섹션 파싱 추가
- 필드 이름 추출 및 InferredTypeInfo 변환
- AppleAnalyzer에 analyze_swift_types() 메서드 추가

## 파일
- crates/fission-loader/src/loader/macho/apple.rs
```

## 규칙

1. **subject**
   - 50자 이내
   - 대문자 시작 금지
   - 마침표 없음
   - 명령형 사용 ("Add" not "Added")

2. **body**
   - 72자 줄바꿈
   - What과 Why 설명
   - 변경된 파일 목록 포함

3. **footer**
   - Breaking changes 표시
   - Issue 참조
