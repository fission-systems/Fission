# Postprocess 모듈 구조 가이드

## 목적

`crates/fission-analysis/src/analysis/decomp/postprocess.rs`의 단일 대형 구현을
카테고리별 모듈로 분리해 유지보수성과 회귀 안정성을 높인다.

핵심 원칙:

- 외부 API(`PostProcessor::process`)는 유지한다.
- 패스 실행 순서(Phase A/B)는 유지한다.
- 기능 추가 시 기존 모듈 책임 경계를 우선 존중한다.

---

## 현재 모듈 트리

- `postprocess.rs`
  - 오케스트레이션/설정/기본 생성자
  - `process()` 패스 체인 정의
- `postprocess/arithmetic.rs`
  - 산술 이디엄 복구
  - 매직 디비전/시프트/CONCAT 정리
- `postprocess/cleanup.rs`
  - 보일러플레이트 제거, 캐스트 보정, dead assign 정리
- `postprocess/loops.rs`
  - loop 재구성 (`while` ↔ `for`, idiom 인식)
- `postprocess/structure.rs`
  - if/while 구조 단순화
- `postprocess/switch_recon.rs`
  - if-else 체인/BST 기반 switch 재구성
- `postprocess/naming.rs`
  - 변수/필드/DWARF 기반 이름 정리
- `postprocess/condition.rs`
  - 조건 부정 유틸 (`negate_condition`)
- `postprocess/tests.rs`
  - postprocess 회귀 테스트

---

## 모듈 책임 규칙

### 1) `postprocess.rs`는 "조립"만 담당

- 새 변환 로직은 가능하면 하위 모듈에 추가한다.
- `postprocess.rs`에는 다음만 남긴다:
  - `PostProcessor` 구조체/설정
  - `process()` 순서 정의
  - 최소한의 공통 엔트리 로직

### 2) 정규식/패턴은 기능 모듈 내부에 지역화

- 산술 관련 정규식은 `arithmetic.rs`
- 루프 관련 정규식은 `loops.rs`
- 네이밍 관련 정규식은 `naming.rs`

### 3) 공통 유틸은 전용 helper 모듈로 승격

- 2개 이상 모듈에서 공유되면 helper 분리 검토
- 현재 예: `condition.rs::negate_condition`

---

## 패스 추가 가이드

새 패스를 추가할 때:

1. 책임 모듈 선택 (`arithmetic/loops/structure/...`)
2. `impl PostProcessor`에 `pub(super)` 함수 추가
3. `process()`에서 호출 위치를 명시적으로 결정
4. `postprocess/tests.rs`에 최소 1개 회귀 테스트 추가
5. `cargo test -p fission-analysis postprocess::tests:: -- --nocapture` 검증

주의:

- 패스 순서 변경은 출력 품질에 직접 영향이 있으므로 독립 커밋/독립 검증을 권장
- 문자열 기반 변환은 false-positive 방지를 위해 입력 패턴을 가능한 좁게 제한

---

## 최근 분리 결과(요약)

- `switch`/`loops`/`cleanup`/`structure`/`naming` 분리 완료
- `apply_arithmetic_idioms`, `recover_divisor`를 `arithmetic.rs`로 이동
- `negate_condition`을 `condition.rs`로 분리
- 테스트를 `postprocess/tests.rs`로 분리하고 조건/구조 회귀 케이스 추가

현재 기준 빌드/핵심 테스트는 통과 상태이며,
기존 `python` feature 관련 `unexpected cfg` 경고만 유지된다.
