# Ghidra vs Fission: 기능 비교 분석

## 실행 요약

Fission은 **Ghidra의 디컴파일러 라이브러리를 직접 사용**하므로, Ghidra의 핵심 분석 엔진(100개 이상의 simplification rules, type inference, control flow analysis 등)은 **이미 실행되고 있습니다**.

차이점은 주로:
1. **출력 스타일** (변수 이름, 타입 이름)
2. **추가 분석 레이어** (Fission이 Ghidra 위에 추가한 기능)

## Ghidra의 주요 분석 단계

### Core Actions (coreaction.cc 라인 5420-5663)

```cpp
// 1. 기본 분석
- ActionUnreachable: 도달 불가능한 코드 제거
- ActionHeritage: SSA 형식 구축
- ActionVarnodeProps: Varnode 속성 설정

// 2. 프로토타입 복구
- ActionParamDouble: 더블 파라미터 처리
- ActionActiveParam: 활성 파라미터 식별
- ActionReturnRecovery: 리턴 값 복구

// 3. 데드 코드 제거 및 최적화
- ActionDeadCode: 데드 코드 제거
- ActionRestrictLocal: 로컬 변수 제한
- ActionRestructureVarnode: Varnode 재구조화

// 4. 타입 추론
- ActionInferTypes: 타입 추론 (핵심!)
- ActionConstantPtr: 포인터 상수 전파

// 5. Simplification Rules (100개 이상)
- RuleEarlyRemoval, RuleTermOrder, RuleSelectCse
- RuleCollectTerms, RulePullsubMulti, ...
- RuleCollapseConstants, RulePropagateCopy
- RuleZextEliminate, RuleSlessToLess, ...
- [총 100+ 규칙이 실행됨]

// 6. 제어 흐름 복구
- ActionBlockStructure: 블록 구조 복구
- ActionRedundBranch: 중복 분기 제거
- ActionStructureTransform: 구조 변환

// 7. 변수 병합 및 정리
- ActionAssignHigh: High-level 변수 할당
- ActionMergeCopy: 복사 병합
- ActionMergeType: 타입 병합
- ActionHideShadow: 숨겨진 변수 처리

// 8. Cleanup Rules
- RuleMultNegOne, RuleAddUnsigned, Rule2Comp2Sub
- RuleFloatSignCleanup, RuleExpandLoad
- RulePtrsubCharConstant: 문자열 포인터 처리
- RuleSplitCopy, RuleSplitLoad, RuleSplitStore
```

## Fission의 분석 파이프라인

### AnalysisPipeline.cpp (라인 400-600)

```cpp
// 1. Ghidra 기본 디컴파일 실행
action.perform(fd);  // ← Ghidra의 모든 액션이 여기서 실행됨!

// 2. Fission 추가 분석
- infer_self_pointer_returns(): 포인터 리턴 추론
- infer_callee_pointer_returns(): 호출된 함수 포인터 리턴 추론
- StructureAnalyzer: 구조체 복구
- GlobalDataAnalyzer: 전역 데이터 분석
- TypePropagator: 추가 타입 전파

// 3. (비활성화됨) Stack Frame 구조 복구
// StackFrameAnalyzer는 현재 주석 처리되어 있음
// 이유: Ghidra의 기본 로컬 변수 처리가 더 나음
```

## 비교 결과

| 항목 | Ghidra | Fission | 상태 |
|------|--------|---------|------|
| **핵심 디컴파일 엔진** | ✅ Ghidra 자체 | ✅ Ghidra 라이브러리 사용 | **동일** |
| **100+ Simplification Rules** | ✅ 모두 실행 | ✅ 모두 실행 (Ghidra 엔진 사용) | **동일** |
| **타입 추론** | ✅ ActionInferTypes | ✅ ActionInferTypes + TypePropagator | **Fission 더 강화** |
| **구조체 복구** | ⚠️ 제한적 | ✅ StructureAnalyzer | **Fission 더 강화** |
| **전역 데이터 분석** | ⚠️ 제한적 | ✅ GlobalDataAnalyzer | **Fission 더 강화** |
| **데이터 섹션 스캔** | ❌ 없음 | ✅ DataSectionScanner | **Fission 추가** |
| **변수 이름 스타일** | `local_XX` | `xStack_XX`, `uStack_XX` | **차이** |
| **타입 이름 스타일** | `undefined4`, `uint` | `DWORD`, `UINT` | **차이** |

## Fission이 추가한 기능

### 1. ✅ 데이터 섹션 심볼 자동 생성
```cpp
// DataSectionScanner.cc
- 부동소수점 상수 자동 감지
- 문자열 상수 자동 감지
- 심볼 자동 등록
```

### 2. ✅ 향상된 구조체 복구
```cpp
// StructureAnalyzer.cc
- 필드 접근 패턴 분석
- 중첩 구조체 지원
- 자동 구조체 정의 생성
```

### 3. ✅ 전역 데이터 구조 분석
```cpp
// GlobalDataAnalyzer.cc
- 전역 데이터 섹션 구조 추론
- 포인터 체인 추적
```

### 4. ✅ 확장된 타입 전파
```cpp
// TypePropagator.cc
- 호출 리턴 타입 전파
- 구조체 타입 전파
- 포인터 타입 전파
```

## Ghidra에는 있지만 Fission에 없는 기능

### 1. ⚠️ Variable Naming 커스터마이징
Ghidra는 `ScopeLocal::buildVariableName`에서 체계적으로 변수 이름을 생성하지만, Fission은 Ghidra 기본값을 사용한 후 후처리로 일부 변경합니다.

**영향**: 변수 이름이 Ghidra 표준과 다름 (`local_XX` vs `xStack_XX`)

### 2. ⚠️ 명시적 캐스팅 생성
Ghidra의 `ActionSetCasts`는 필요한 곳에 명시적 캐스팅을 추가하지만, Fission은 덜 적극적입니다.

**예시**:
```c
// Ghidra
sum_array((longlong)&local_38,5);

// Fission
sum_array(&xStack_38,5);
```

**영향**: 미미 (기능적으로는 동일)

### 3. ❌ 타입 이름 표준화
Ghidra는 `undefined4`, `uint` 등의 표준 타입을 사용하지만, Fission은 후처리로 `DWORD`, `UINT` 등 Windows 타입으로 변환합니다.

**영향**: Similarity 점수에 큰 영향 (현재 20%)

## 실제로 누락된 기능은?

### 답: 거의 없음! ✅

Fission은 Ghidra의 디컴파일러 라이브러리를 **완전히 사용**하므로:
- ✅ 모든 simplification rules 실행됨
- ✅ 타입 추론 실행됨
- ✅ 제어 흐름 복구 실행됨
- ✅ 변수 병합 실행됨

**실제 차이점**:
1. **출력 스타일** (변수/타입 이름) ← **이것이 Similarity 차이의 주 원인**
2. **추가 분석** (Fission이 더 많은 분석을 함)

## 결론

### Fission의 강점
- ✅ Ghidra의 모든 핵심 기능 포함
- ✅ 추가적인 고급 분석 (DataSectionScanner, StructureAnalyzer, TypePropagator)
- ✅ Windows 타입 시스템 (DWORD, UINT 등)

### 개선이 필요한 부분
- ⚠️ 변수 이름 표준화 (`xStack_XX` → `local_XX`)
- ⚠️ 타입 이름 표준화 (`DWORD` → `undefined4`)
- ⚠️ 명시적 캐스팅 추가

### 권장사항
**스타일 개선 (변수/타입 이름 표준화)**를 진행하면:
- Similarity 20% → 예상 65-85%로 대폭 향상
- Ghidra 생태계와의 호환성 향상
- 벤치마크 통과

**누락된 핵심 기능은 없으므로**, 스타일 조정만으로도 Ghidra와 동등한 품질을 달성할 수 있습니다!
