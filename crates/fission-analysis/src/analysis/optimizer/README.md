# Decompiler Optimizer

디컴파일러 출력 품질을 향상시키는 최적화 모듈입니다. Ghidra 11.4.2의 최적화 규칙을 참고하여 구현되었습니다.

## 구현된 최적화

### 1. 비트 연산 단순화 (Bit Operations Simplification)
Ghidra의 Rule 기반 최적화를 Rust로 구현:

- **RuleAndMask**: `(x | 0xFF) & 0xFF` → `x & 0xFF`
- **RuleOrMask**: `(x & 0xFF) | 0xFF` → `0xFF`
- **RuleBxor2NotEqual**: `(x ^ y) == 0` → `x == y`
- **RuleShiftBitops**: `(x << 8) >> 8` → `x & 0x00FFFFFFFFFFFFFF`
- **기본 단순화**: 
  - `x & 0` → `0`
  - `x & -1` → `x`
  - `x | 0` → `x`
  - `x ^ 0` → `x`

### 2. 제어 흐름 정규화 (Control Flow Normalization)
Ghidra의 ActionNormalizeBranches 구현:

- **이중 부정 제거**: `if (!!x)` → `if (x)`
- **조건 반전**: `if (!(x < y))` → `if (x >= y)`
- **상수 순서 정규화**: `if (5 == x)` → `if (x == 5)`
- **비교 연산자 뒤집기**: `if (5 < x)` → `if (x > 5)`

### 3. 임시 변수 인라인화 (Temporary Variable Inlining)
Ghidra의 ActionMarkImplied 유사 기능:

- **단일 사용 변수 제거**: 
  ```rust
  temp_1 = x + 1;
  return temp_1;
  ```
  →
  ```rust
  return x + 1;
  ```

- **임시 변수 패턴 인식**: `temp_*`, `uVar*`, `iVar*`, `_tmp*`

## 사용 예제

```rust
use fission::analysis::optimizer::{Optimizer, OptimizerConfig};

// 기본 설정으로 생성
let mut optimizer = Optimizer::new();

// 표현식 최적화
let expr = Expr::BinOp {
    op: BinOpKind::And,
    left: Box::new(Expr::Var("x".to_string())),
    right: Box::new(Expr::Const(0)),
};
let optimized = optimizer.optimize_expr(expr); // 결과: Const(0)

// 문장 최적화
let stmts = vec![
    Stmt::Expr(Expr::Assign {
        target: "temp_1".to_string(),
        value: Box::new(Expr::Const(42)),
    }),
    Stmt::Return(Some(Expr::Var("temp_1".to_string()))),
];
let optimized = optimizer.optimize_stmts(stmts); // temp_1 인라인됨
```

## 설정 옵션

```rust
use fission::analysis::optimizer::{Optimizer, OptimizerConfig};

let config = OptimizerConfig {
    enable_bitops: true,          // 비트 연산 최적화
    enable_control_flow: true,    // 제어 흐름 정규화
    enable_temp_inline: true,     // 임시 변수 인라인화
};

let optimizer = Optimizer::with_config(config);
```

## 아키텍처

```
optimizer/
├── mod.rs           // 메인 Optimizer 구조체
├── bitops.rs        // 비트 연산 최적화 규칙
├── control_flow.rs  // 제어 흐름 정규화
└── temp_inline.rs   // 임시 변수 인라인화
```

## 테스트

```bash
cargo test --lib analysis::optimizer
```

**결과**: 15개 테스트 모두 통과 ✓

## 향후 개선 계획

### Medium Priority
- **포인터 연산 최적화**: `(char*)ptr + offset*4 + 8` → `ptr[offset].member`
- **Common Subexpression Elimination (CSE)**: 중복 계산 제거

### Low Priority  
- **타입 기반 최적화**: 읽기 전용 변수 → 상수 변환
- **구조체 멤버 재구성**: 타입 정보 기반 최적화

## 참고 자료

- Ghidra 11.4.2 소스코드
  - `ruleaction.hh`: 135개의 Rule 클래스
  - `action.hh/cc`: Action 파이프라인
  - `coreaction.hh`: 핵심 Action 구현
- Ghidra Documentation: [NSA Ghidra](https://ghidra-sre.org/)

## 라이선스

Apache 2.0 (Ghidra와 동일)
