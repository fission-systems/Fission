# 스타일 차이 분석 및 개선 방향

## 현재 스타일 차이점

### 1. 타입 이름
| Ghidra 표준 | Fission | 비고 |
|-------------|---------|------|
| `undefined4` | `DWORD` | Windows 스타일 |
| `uint` | `UINT` | Windows 스타일 |
| `uint*` | `void*` | 포인터 타입 |

### 2. 변수 이름
| Ghidra | Fission | 차이 |
|--------|---------|------|
| `local_38` | `xStack_38` | 접두사 차이 |
| `local_c` | `uStack_c` | 타입 표시 (`u` = unsigned) |
| `local_18` | `pvStack_18` | 타입 표시 (`pv` = pointer void) |

### 3. 캐스팅
| Ghidra | Fission |
|--------|---------|
| `sum_array((longlong)&local_38,5)` | `sum_array(&xStack_38,5)` |

## 스타일 개선의 이점

### 1. ✅ **일관성 및 표준 준수**
- **Ghidra 생태계 호환성**: 다른 Ghidra 기반 도구 및 플러그인과의 호환성 향상
- **커뮤니티 표준**: 리버스 엔지니어링 커뮤니티가 익숙한 명명 규칙
- **문서화**: 기존 Ghidra 문서 및 튜토리얼과 일치

### 2. ✅ **벤치마크 향상**
- **Similarity 점수**: 현재 20% → 예상 60-80%로 향상 가능
- **자동 비교**: diff 도구로 쉽게 비교 가능
- **품질 측정**: 객관적인 품질 지표 제공

### 3. ✅ **가독성 향상**
```c
// 현재 (Fission)
DWORD xStack_38;  // "x"가 무엇을 의미하는지 불명확

// 개선 후
undefined4 local_38;  // 명확한 의미: 스택의 로컬 변수
```

### 4. ❌ **단점**
- **Windows 타입 손실**: `DWORD`, `UINT` 같은 명시적 Windows 타입 정보 손실
- **플랫폼 중립성**: Ghidra 표준은 플랫폼 중립적이지만 덜 명시적

## Fission의 현재 구현

### 타입 변환 메커니즘 (`PostProcessors.cc`)
```cpp
// 라인 561-565: undefined4 -> DWORD 변환
pos = 0;
while ((pos = result.find("undefined4", pos)) != std::string::npos) {
    result.replace(pos, 10, "DWORD");
    pos += 5;
}
```

이는 **후처리 문자열 치환**으로 구현되어 있습니다.

### 변수 명명 규칙 (`varmap.cc`)
Ghidra는 `ScopeLocal::buildVariableName` (라인 551-580)에서:
```cpp
s << spacename;  // "Stack"
if (start <= 0) {
  s << 'X';      // 로컬 스택: "StackX"
}
s << '_' << hex << start;  // "StackX_38" → Fission에서 "xStack_38"
```

Fission은 이 로직을 사용하지만, 출력에서는 순서가 다릅니다:
- Ghidra 의도: "Stack"
- Fission 출력: "xStack_38" (접두사 "x"가 추가됨)

## 개선 방안

### 옵션 A: Ghidra 표준으로 전환 (권장)
**장점**:
- ✅ Similarity +40-60% 향상
- ✅ 표준 준수
- ✅ 도구 호환성

**단점**:
- ❌ Windows 타입 명시성 감소
- ❌ 기존 Fission 사용자 혼란

**구현**:
1. `PostProcessors.cc`의 타입 치환 제거
2. 변수 명명 규칙 조정

### 옵션 B: 설정 가능한 스타일
**구현**:
```rust
pub enum OutputStyle {
    GhidraStandard,  // undefined4, local_XX
    WindowsStyle,    // DWORD, xStack_XX
}
```

**장점**:
- ✅ 사용자 선택 가능
- ✅ 양쪽 장점 활용

**단점**:
- ❌ 유지보수 부담 증가
- ❌ 복잡도 증가

### 옵션 C: 현재 유지
**장점**:
- ✅ Windows 바이너리 분석에 적합
- ✅ 명시적 타입 정보

**단점**:
- ❌ Similarity 낮음
- ❌ 표준 미준수

## 권장사항

### 우선순위 판단 기준

1. **Fission의 목표가 무엇인가?**
   - Ghidra 호환성? → **옵션 A** (Ghidra 표준)
   - Windows 전문 디컴파일러? → **옵션 C** (현재 유지)
   - 범용 디컴파일러? → **옵션 B** (설정 가능)

2. **사용자층**
   - 리버스 엔지니어링 전문가 → Ghidra 표준 선호
   - Windows 악성코드 분석가 → Windows 타입 선호
   - 일반 개발자 → 가독성 중심

3. **실용성**
   - Similarity 20% → 60-80%는 **큰 개선**
   - 변수 이름 변경은 **상대적으로 쉬움** (타입 전파보다)
   - 벤치마크 통과는 **프로젝트 신뢰도** 향상

## 구현 난이도

| 항목 | 난이도 | 예상 효과 |
|------|--------|-----------|
| 타입 이름 변경 | 하 | +10-15% similarity |
| 변수 이름 변경 | 중 | +30-40% similarity |
| 캐스팅 추가 | 중 | +5-10% similarity |

**총 예상 효과**: 현재 20% → 65-85% similarity

## 결론

**스타일 개선의 이점은 명확합니다**:
- ✅ 벤치마크 점수 대폭 향상
- ✅ Ghidra 생태계 호환성
- ✅ 프로젝트 신뢰도 향상

**하지만**:
- ⚠️ Windows 타입 명시성 손실
- ⚠️ 기존 코드와의 일관성 문제

**제안**: 옵션 B (설정 가능)를 장기 목표로 하되, 우선 **옵션 A (Ghidra 표준)**를 기본값으로 구현하여 벤치마크를 통과한 후, 나중에 Windows 스타일을 추가 옵션으로 제공하는 것이 합리적입니다.
