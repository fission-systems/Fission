# RetDec Backend Integration Plan

## 목표

Fission의 디컴파일 출력 품질을 개선하기 위해 RetDec의 `llvmir2hll` 모듈을 통합합니다.
현재 Ghidra와의 유사도 ~30%를 **60% 이상**으로 끌어올리는 것이 목표입니다.

## 아키텍처 설계

### 현재 파이프라인

```
Binary → Ghidra P-code Lifter → CFG → Type Propagation → C Code Emitter
```

### 목표 파이프라인 (하이브리드)

```
Binary → Ghidra P-code Lifter → CFG → [RetDec Enhanced CFG Optimizer] → C Code Emitter
                                      ↓
                               [Optional: LLVM IR Export]
                                      ↓
                               RetDec llvmir2hll (고품질 C 생성)
```

## 단계별 구현 계획

### Phase 1: RetDec 라이브러리 빌드 (Day 1)

- [ ] RetDec 의존성 다운로드 및 빌드
- [ ] `libretdec-llvmir2hll.a/dylib` 생성 확인
- [ ] Fission CMake에 RetDec 링킹 추가

### Phase 2: P-code to LLVM IR 변환기 (Day 2-3)

- [ ] `ghidra_decompiler/src/ir/PcodeToLLVM.cc` 신규 작성
- [ ] Ghidra P-code 오퍼레이션 → LLVM IR Instruction 매핑
  - COPY → Load/Store
  - BRANCH/CBRANCH → br/br_cond
  - CALL → call
  - INT_ADD/SUB/MUL → add/sub/mul
  - LOAD/STORE → getelementptr + load/store
- [ ] 함수 단위 LLVM Module 생성 API

### Phase 3: RetDec HLL Writer 통합 (Day 4)

- [ ] `fission/decompiler/RetDecBackend.h/cc` 신규 작성
- [ ] LLVM Module → RetDec `LlvmIr2Hll::runOnModule()` 호출
- [ ] C 코드 문자열 추출 및 반환
- [ ] 기존 `PostProcessor`와 통합 (함수명, 주석 병합)

### Phase 4: 옵션 및 폴백 (Day 5)

- [ ] CLI 플래그: `--backend=ghidra|retdec`
- [ ] GUI 설정: Decompiler Backend 선택 드롭다운
- [ ] RetDec 실패 시 Ghidra 기본 출력으로 폴백

### Phase 5: 테스트 및 벤치마크 (Day 6)

- [ ] Complex Test Suite 재실행
- [ ] 유사도 점수 비교 (Before/After)
- [ ] 리포트 생성 및 CHANGELOG 업데이트

## 기술 상세

### P-code → LLVM IR 매핑 표

| P-code Op | LLVM IR |
|-----------|---------|
| COPY | `= %src` (직접 할당) |
| INT_ADD | `add i32 %a, %b` |
| INT_SUB | `sub i32 %a, %b` |
| INT_MULT | `mul i32 %a, %b` |
| INT_DIV | `sdiv i32 %a, %b` |
| INT_AND | `and i32 %a, %b` |
| INT_OR | `or i32 %a, %b` |
| INT_XOR | `xor i32 %a, %b` |
| LOAD | `load i32, i32* %ptr` |
| STORE | `store i32 %val, i32* %ptr` |
| BRANCH | `br label %target` |
| CBRANCH | `br i1 %cond, label %true, label %false` |
| CALL | `call @funcname(...)` |
| RETURN | `ret i32 %val` |

### RetDec API 사용 예시

```cpp
#include "retdec/llvmir2hll/llvmir2hll.h"
#include "retdec/config/config.h"

std::string decompileWithRetDec(llvm::Module& module) {
    std::string output;
    retdec::config::Config config;
    
    retdec::llvmir2hll::LlvmIr2Hll decompiler(&config);
    decompiler.setOutputString(&output);
    decompiler.runOnModule(module);
    
    return output;
}
```

## 예상 리스크 및 완화

| 리스크 | 완화 방안 |
|--------|----------|
| LLVM 버전 불일치 | RetDec이 요구하는 LLVM 버전(14.x) 확인 후 빌드 |
| P-code 미지원 Op | 폴백 로직으로 Ghidra 기본 출력 사용 |
| RetDec 빌드 시간 | 사전 빌드된 바이너리 캐싱 또는 CI/CD 활용 |
| 메모리 사용량 증가 | LLVM Module 즉시 해제, 함수 단위 처리 |

## 성공 기준

- [ ] Complex Test Suite 평균 유사도 **50% 이상** 달성
- [ ] 단일 함수 디컴파일 시간 **5초 이내** 유지
- [ ] GUI에서 Backend 선택 가능
- [ ] RetDec 실패 시 자동 폴백 작동

## 참고 자료

- RetDec GitHub: <https://github.com/avast/retdec>
- LLVM IR Reference: <https://llvm.org/docs/LangRef.html>
- Ghidra P-code Reference: ghidra_decompiler/decompile/cpp/opbehavior.hh
