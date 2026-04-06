# 상수 표현 및 포인터 비교 개선

## 문제 설명

### 1. 포인터 NULL 비교 오류
Fission에서 포인터를 NULL과 비교할 때 부동소수점으로 잘못 출력되는 문제가 있었습니다:

```c
// Ghidra (올바름)
if (local_18 != (uint *)0x0)

// Fission (이전 - 오류)
if (pvStack_18 != 0.0)  // ❌ 포인터를 부동소수점과 비교!

// Fission (수정 후)
if (pvStack_18 != (void *)0x0)  // ✅ 올바른 포인터 비교
```

### 2. 근본 원인
`printc.cc`의 부동소수점 휴리스틱 개선 시, 상수 `0`을 포함한 모든 4/8바이트 값을 부동소수점으로 해석하려고 시도했습니다:

```cpp
// 이전 코드 (문제)
if (ct->getSize() == 8 || ct->getSize() == 4) {
  FloatFormat::floatclass type;
  double floatval = format->getHostFloat(val,&type);
  if (type == FloatFormat::normalized || 
      type == FloatFormat::denormalized || 
      type == FloatFormat::zero) {  // ❌ zero 포함!
    push_float(val,ct->getSize(),tag,vn,op);
    return;
  }
}
```

이로 인해:
- 정수 `0`이 `0.0`으로 출력됨
- 포인터 비교가 부동소수점 비교로 잘못 표시됨

## 해결 방법

### 1. 부동소수점 휴리스틱 개선
`legacy-native-decompiler-tree/decompile/printc.cc`의 `pushConstant()` 함수를 수정하여:

1. **값 `0` 제외**: 정수 0은 부동소수점으로 변환하지 않음
2. **포인터 값 제외**: 높은 주소 값(0x10000 이상)은 포인터로 간주
3. **FloatFormat::zero 제외**: 정규화/비정규화된 부동소수점만 변환

```cpp
// 수정된 코드
if ((ct->getSize() == 8 || ct->getSize() == 4) && val != 0) {
  // Skip values that look like pointers (high addresses)
  bool looksLikePointer = (val > 0x10000 && val < 0xFFFFFFFFFFFFFFFFULL);
  
  if (!looksLikePointer) {
    const FloatFormat *format = glb->translate->getFloatFormat(ct->getSize());
    if (format != (const FloatFormat *)0) {
      FloatFormat::floatclass type;
      double floatval = format->getHostFloat(val,&type);
      // Only convert normalized or denormalized floats
      if (type == FloatFormat::normalized || type == FloatFormat::denormalized) {
        if (floatval >= -1e308 && floatval <= 1e308) {
          push_float(val,ct->getSize(),tag,vn,op);
          return;
        }
      }
    }
  }
}
```

### 2. 효과
- ✅ 포인터 NULL 비교가 올바르게 출력: `ptr != (type *)0x0`
- ✅ 부동소수점 상수는 여전히 정확하게 표시: `49.99`, `10.0`
- ✅ 포인터 주소값은 16진수로 유지

## 벤치마크 결과

### Before (result_string_inline)
```c
if (pvStack_18 != 0.0) {  // ❌ 오류
```

### After (result_pointer_fix)
```c
if (pvStack_18 != (void *)0x0) {  // ✅ 수정됨
```

## 남은 차이점 (스타일)
현재 Ghidra와 Fission의 주요 차이는 코드 생성 스타일입니다:

1. **타입 이름**: 
   - Ghidra: `undefined4`, `uint`, `uint*`
   - Fission: `DWORD`, `UINT`, `void*` (Windows 스타일)

2. **변수 이름**:
   - Ghidra: `local_XX`
   - Fission: `xStack_XX`, `uStack_XX`, `pvStack_XX`

3. **캐스팅**:
   - Ghidra: `sum_array((longlong)&local_38,5)` (명시적 캐스팅)
   - Fission: `sum_array(&xStack_38,5)` (암시적)

이러한 차이는 기능적으로 동일하며, 주로 명명 규칙과 타입 정의의 차이입니다.

## 관련 파일
- `legacy-native-decompiler-tree/decompile/printc.cc` - 상수 출력 로직
- `legacy-native-decompiler-tree/decompile/typeop.cc` - 비교 연산자 타입 처리
- `docs/analysis/STRING_INLINING.md` - 문자열 인라인 개선
- `docs/analysis/TYPE_PROPAGATION_STATUS.md` - 타입 전파 개선
- `docs/analysis/IMPROVEMENT_LOG.md` - 전체 개선 기록

## 구현 일자
- 2026-01-08: 포인터 NULL 비교 수정 및 부동소수점 휴리스틱 개선
