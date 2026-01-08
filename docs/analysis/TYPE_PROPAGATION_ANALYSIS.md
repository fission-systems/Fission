# Type Propagation Analysis: Ghidra vs Fission

## 문제 상황

### 증상
```c
// Ghidra (목표)
local_18 = create_item(0x3e9,"TestItem",DAT_1400040c8);

// Fission (현재)
pvStack_18 = create_item(0x3e9,"TestItem",0x4048feb851eb851f);
```

Fission은 부동소수점 상수를 16진수 리터럴로 표시하여 가독성이 매우 낮습니다.
- `0x4048feb851eb851f` = `49.99` (double)
- `0x4059000000000000` = `100.0` (double)

---

## Ghidra 타입 전파 메커니즘 분석

### 1. 전체 구조

```
ActionInferTypes (coreaction.cc:4873-5283)
├─ buildLocaltypes()      // 각 Varnode의 초기 타입 설정
├─ propagateOneType()     // DFS로 타입 전파
│  └─ propagateTypeEdge() // 각 간선에서 타입 전파
│     └─ TypeOp::propagateType() // 연산별 타입 전파 규칙
│        ├─ TypeOpLoad::propagateType()
│        ├─ TypeOpStore::propagateType()
│        └─ ...
└─ writeBack()            // 전파된 타입 적용
```

### 2. LOAD 연산의 타입 전파

**파일**: `ghidra_decompiler/decompile/typeop.cc`

**TypeOpLoad::propagateType (라인 487-500)**:
```cpp
Datatype *TypeOpLoad::propagateType(Datatype *alttype,PcodeOp *op,Varnode *invn,Varnode *outvn,
				    int4 inslot,int4 outslot)
{
  if ((inslot==0)||(outslot==0)) return (Datatype *)0; // Don't propagate along this edge
  if (invn->isSpacebase()) return (Datatype *)0;
  Datatype *newtype;
  if (inslot == -1) {	 // Propagating output to input (value to ptr)
    AddrSpace *spc = op->getIn(0)->getSpaceFromConst();
    newtype = propagateToPointer(tlst,alttype,outvn->getSize(),spc->getWordSize());
  }
  else
    newtype = propagateFromPointer(tlst, alttype, outvn->getSize());
  return newtype;
}
```

**핵심 함수: propagateFromPointer (라인 206-226)**:
```cpp
Datatype *TypeOp::propagateFromPointer(TypeFactory *t,Datatype *dt,int4 sz)
{
  if (dt->getMetatype() != TYPE_PTR)
    return (Datatype *)0;  // ❌ 포인터가 아니면 전파 실패!
  
  Datatype *ptrto = ((TypePointer *)dt)->getPtrTo();
  if (ptrto->isVariableLength())
    return (Datatype *)0;
  if (ptrto->getSize() == sz)
    return ptrto;  // ✅ 포인터가 가리키는 타입 반환
  
  // ... 크기 불일치 처리 ...
  return (Datatype *)0;
}
```

### 3. 동작 과정 예시

**어셈블리**:
```asm
movsd xmm0, [0x1400040C8]  ; double 값 로드
```

**P-Code**:
```
%xmm0 = LOAD(ram, 0x1400040C8)
```

**Ghidra의 처리**:

1. **초기 상태**: `0x1400040C8`는 상수 Varnode
2. **타입 질의**: 전역 스코프에서 주소 조회
3. **심볼 발견**: `DAT_1400040c8` (타입: `double`)
4. **포인터 생성**: `ptr<double>` 타입 
5. **LOAD 전파**: `propagateFromPointer` 호출
6. **결과 타입**: `double`
7. **출력**: `DAT_1400040c8`

---

## Fission 현재 구현 분석

### 1. TypePropagator 구조

**파일**: `ghidra_decompiler/src/analysis/TypePropagator.cc`

```cpp
int TypePropagator::propagate(Funcdata* fd) {
    // Phase 1: 초기 타입 설정
    build_local_types(fd);
    
    // Phase 2: 함수 호출 반환 타입 전파
    propagate_call_return_types(fd);
    
    // Phase 3: CALL 연산에서 전파
    // ...
    
    // Phase 4: 간선 기반 전파 (Ghidra 스타일)
    for (vn_iter = fd->beginLoc(); vn_iter != fd->endLoc(); ++vn_iter) {
        Varnode* vn = *vn_iter;
        propagate_one_type(vn);
    }
    
    // Phase 5: 스택 포인터 타입 추론
    infer_stack_pointer_types(fd);
    
    // Phase 6: 변경사항 적용
    bool changed = write_back(fd);
    
    return inferred_types.size();
}
```

### 2. Fission의 처리 과정

**어셈블리**:
```asm
movsd xmm0, [0x1400040C8]  ; double 값 로드
```

**P-Code**:
```
%xmm0 = LOAD(ram, 0x1400040C8)
```

**Fission의 현재 처리**:

1. **초기 상태**: `0x1400040C8`는 상수 Varnode
2. **타입**: `undefined8` 또는 `QWORD` (정수)
3. **LOAD 전파**: `TypeOpLoad::propagateType` 호출
4. **문제**: `0x1400040C8`가 `TYPE_PTR`가 아님!
5. **전파 실패**: `propagateFromPointer`에서 nullptr 반환
6. **결과 타입**: `undefined` 또는 `QWORD`
7. **출력**: `0x4048feb851eb851f` (16진수 리터럴)

---

## 근본 원인 분석

### 문제 1: 데이터 섹션 심볼 없음

**Ghidra**:
- 바이너리 로드 시 `.rdata` 섹션 스캔
- 데이터 주소마다 심볼 생성 (`DAT_<address>`)
- 타입 자동 추론 (패턴 인식)
- 전역 스코프에 등록

**Fission**:
- ❌ 데이터 섹션 자동 스캔 없음
- ❌ `DAT_` 심볼 생성 없음
- ❌ 데이터 타입 자동 추론 없음
- ❌ 상수는 그냥 상수로 유지

### 문제 2: 상수의 포인터화 실패

**Ghidra의 동작**:
```cpp
// 상수 주소 0x1400040C8가 데이터 섹션을 가리킴
Varnode* constVn = ...;  // 값: 0x1400040C8
Symbol* sym = globalScope->queryByAddr(constVn->getOffset());  // ✅ DAT_1400040c8 발견
Datatype* dt = sym->getType();  // double
Datatype* ptr_dt = makePointer(dt);  // ptr<double>
constVn->setType(ptr_dt);  // 타입 지정
```

**Fission의 현재 동작**:
```cpp
// 상수 주소 0x1400040C8
Varnode* constVn = ...;  // 값: 0x1400040C8
// ❌ 심볼 조회 시도하지만 아무것도 없음
// ❌ 타입은 uint8로 유지
// ❌ 포인터로 변환되지 않음
```

### 문제 3: 타입 전파 체인 단절

```
[상수 0x1400040C8] --LOAD--> [로드된 값]
     ↓ 타입?                    ↓ 전파?
   undefined8                   실패 ❌
   (TYPE_PTR 아님)
```

**전파가 작동하려면**:
```
[상수 0x1400040C8] --LOAD--> [로드된 값]
     ↓ 타입?                    ↓ 전파?
ptr<double>                   double ✅
   (TYPE_PTR)
```

---

## 해결 방안

### 방안 A: 데이터 섹션 심볼 자동 생성 (권장) ⭐

**복잡도**: 중간  
**효과**: 높음  
**Ghidra 방식과 동일**: ✅

#### 구현 계획

**1단계: 바이너리 로딩 시 데이터 섹션 스캔**

```cpp
// ghidra_decompiler/src/loaders/DataSectionScanner.cpp (새 파일)

class DataSectionScanner {
public:
    struct DataSymbol {
        uint64_t address;
        int size;
        Datatype* type;
        std::string name;
    };
    
    std::vector<DataSymbol> scanDataSection(
        const std::vector<uint8_t>& binary,
        uint64_t section_va,
        size_t section_size
    ) {
        std::vector<DataSymbol> symbols;
        
        // 8바이트 경계마다 스캔
        for (size_t offset = 0; offset + 8 <= section_size; offset += 8) {
            uint64_t value = *reinterpret_cast<const uint64_t*>(&binary[offset]);
            
            // Float/Double 패턴 감지
            if (looksLikeDouble(value)) {
                DataSymbol sym;
                sym.address = section_va + offset;
                sym.size = 8;
                sym.type = TYPE_FLOAT;  // double
                sym.name = "DAT_" + toHex(sym.address);
                symbols.push_back(sym);
                offset += 0;  // 다음은 8바이트 건너뛰기
            }
        }
        
        return symbols;
    }
    
private:
    bool looksLikeDouble(uint64_t bits) {
        // IEEE 754 double 검증
        FloatFormat format(8);
        FloatFormat::floatclass fclass;
        double value = format.getHostFloat(bits, &fclass);
        
        // Normalized, denormalized, zero만 허용 (NaN, Inf 제외)
        if (fclass != FloatFormat::normalized &&
            fclass != FloatFormat::denormalized &&
            fclass != FloatFormat::zero) {
            return false;
        }
        
        // 합리적인 범위인지 확인
        return (value >= -1e308 && value <= 1e308);
    }
};
```

**2단계: 전역 스코프에 심볼 등록**

```cpp
// ghidra_decompiler/src/core/BinaryLoader.cpp

void registerDataSymbols(
    Architecture* arch,
    const std::vector<DataSectionScanner::DataSymbol>& symbols
) {
    Scope* globalScope = arch->symboltab->getGlobalScope();
    TypeFactory* types = arch->types;
    AddrSpace* ramSpace = arch->getDefaultDataSpace();
    
    for (const auto& sym : symbols) {
        Address addr(ramSpace, sym.address);
        
        // 타입 가져오기
        Datatype* dt = nullptr;
        if (sym.type == TYPE_FLOAT) {
            dt = (sym.size == 8) ? types->getBase(8, TYPE_FLOAT)  // double
                                 : types->getBase(4, TYPE_FLOAT); // float
        }
        
        // 심볼 추가
        globalScope->addSymbol(sym.name, dt, addr, Address());
        
        std::cerr << "[DataScanner] Created symbol: " << sym.name 
                  << " at 0x" << std::hex << sym.address
                  << " type=" << dt->getName() << std::endl;
    }
}
```

**3단계: 로딩 파이프라인 통합**

```cpp
// ghidra_decompiler/src/decompiler/LoaderPipeline.cpp

void loadBinary(DecompContext* ctx, const std::string& path) {
    // ... 기존 로딩 코드 ...
    
    // 데이터 섹션 찾기
    for (const auto& block : ctx->memory_blocks) {
        if (block.name == ".rdata" || block.name == ".data") {
            DataSectionScanner scanner;
            auto symbols = scanner.scanDataSection(
                ctx->binary_data,
                block.va_addr,
                block.file_size
            );
            
            registerDataSymbols(ctx->arch.get(), symbols);
            
            std::cerr << "[Loader] Registered " << symbols.size() 
                      << " data symbols from " << block.name << std::endl;
        }
    }
}
```

### 방안 B: 상수 포인터 힌트 주입

**복잡도**: 낮음  
**효과**: 중간  
**임시 해결책**: ✅

```cpp
// ghidra_decompiler/src/analysis/ConstantPointerHints.cpp

void injectConstantPointerHints(Funcdata* fd, Architecture* arch) {
    TypeFactory* types = arch->types;
    AddrSpace* ramSpace = arch->getDefaultDataSpace();
    
    // LOAD 연산 찾기
    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (op->code() != CPUI_LOAD) continue;
        
        Varnode* ptrVn = op->getIn(1);
        if (!ptrVn->isConstant()) continue;
        
        uint64_t addr = ptrVn->getOffset();
        int loadSize = op->getOut()->getSize();
        
        // 데이터 섹션 주소인지 확인
        if (!isInDataSection(addr, ctx)) continue;
        
        // 메모리에서 값 읽기
        uint64_t value = readMemory(ctx, addr, loadSize);
        
        // Float/Double로 보이는지 확인
        if (loadSize == 8 || loadSize == 4) {
            FloatFormat* fmt = arch->translate->getFloatFormat(loadSize);
            if (fmt) {
                FloatFormat::floatclass fclass;
                double fval = fmt->getHostFloat(value, &fclass);
                
                if (fclass == FloatFormat::normalized ||
                    fclass == FloatFormat::denormalized ||
                    fclass == FloatFormat::zero) {
                    // 포인터 타입 생성
                    Datatype* floatType = types->getBase(loadSize, TYPE_FLOAT);
                    Datatype* ptrType = types->getTypePointer(
                        ptrVn->getSize(), floatType, ramSpace->getWordSize()
                    );
                    
                    // 타입 힌트 설정
                    ptrVn->setTempType(ptrType);
                }
            }
        }
    }
}
```

### 방안 C: 출력 단계에서 후처리

**복잡도**: 낮음  
**효과**: 낮음  
**이미 시도함**: ⚠️ (부분적 성공)

현재 `printc.cc`의 개선 (우선순위 2에서 구현):
- 비교 연산에서는 작동 (`!= 0.0`)
- 함수 인자에서는 실패 (이미 타입이 결정된 후)

---

## 비교표

| 항목 | Ghidra | Fission (현재) | 차이점 |
|------|--------|----------------|---------|
| **데이터 섹션 스캔** | ✅ 자동 | ❌ 없음 | 심볼 생성 안됨 |
| **DAT_ 심볼** | ✅ 생성 | ❌ 없음 | 참조 불가 |
| **타입 추론** | ✅ 패턴 인식 | ⚠️ 부분적 | Float 감지 미흡 |
| **포인터 생성** | ✅ 자동 | ❌ 수동 필요 | 전파 실패 |
| **LOAD 전파** | ✅ 작동 | ⚠️ 조건부 | 포인터 필요 |
| **출력 형식** | `DAT_addr` | `0xHEX` | 가독성 저하 |

---

## 추천 구현 순서

### Phase 1: 데이터 스캔 및 심볼 생성 (1-2일)
1. `DataSectionScanner` 클래스 구현
2. Float/Double 패턴 감지 로직
3. 전역 스코프 심볼 등록
4. 테스트 및 검증

### Phase 2: 로더 통합 (반나절)
1. `LoaderPipeline`에 스캐너 호출 추가
2. `.rdata`, `.data` 섹션 처리
3. 로그 출력 추가

### Phase 3: 타입 전파 검증 (반나절)
1. 비교 벤치마크 재실행
2. `DAT_` 심볼 출력 확인
3. 유사도 측정

### Phase 4: 추가 타입 지원 (선택)
1. 32비트 float 지원
2. 문자열 상수 지원
3. 구조체 참조 지원

---

## 예상 효과

### 성공 시 출력

**Before**:
```c
pvStack_18 = create_item(0x3e9,"TestItem",0x4048feb851eb851f);
uStack_1c = calculate_discount(0xf,0x4059000000000000);
```

**After**:
```c
pvStack_18 = create_item(0x3e9,"TestItem",DAT_1400040c8);
uStack_1c = calculate_discount(0xf,DAT_1400040d0);
```

또는 더 나은 경우:
```c
pvStack_18 = create_item(0x3e9,"TestItem",49.99);
uStack_1c = calculate_discount(0xf,100.0);
```

### 유사도 개선 예상

| 함수 | 현재 | 예상 | 증가 |
|------|------|------|------|
| add | 100% | 100% | - |
| multiply | 100% | 100% | - |
| print_message | 80% | 85% | +5% |
| main | 20% | 50-60% | +30-40% ⭐ |

---

## 참조

### Ghidra 소스
- `coreaction.cc`: ActionInferTypes (라인 4873-5283)
- `typeop.cc`: TypeOpLoad::propagateType (라인 487-500)
- `typeop.cc`: propagateFromPointer (라인 206-226)
- `database_ghidra.cc`: ScopeGhidra::addSymbol (라인 355-362)

### Fission 소스
- `TypePropagator.cc`: propagate() (라인 594-652)
- `printc.cc`: pushConstant() 개선 (라인 1806-1835)

### 관련 문서
- `docs/analysis/IMPROVEMENT_LOG.md`: 우선순위 1, 2 개선
- `docs/analysis/KNOWN_ISSUES.md`: TypePropagator 이슈

---

## 결론

**왜 실패했는가?**
- Fission은 Ghidra의 **타입 전파 메커니즘은 사용**하지만
- Ghidra의 **데이터 섹션 처리 메커니즘은 누락**됨
- 결과: 타입 전파 체인이 시작점에서 단절

**해결책**:
- 데이터 섹션 스캔 및 심볼 생성 구현 (방안 A)
- Ghidra와 동일한 방식으로 처리
- 타입 전파는 이미 작동 중 → 입력만 수정하면 됨

**난이도**: 중간 (Ghidra 메커니즘을 이미 사용 중이므로)  
**기대 효과**: 높음 (유사도 +30-40% 예상)
