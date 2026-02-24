# Ghidra Decompiler — Architecture

> 최종 갱신: 2026-02-24  
> 대상 빌드: `ghidra_decompiler/build/` (CMake), `crates/fission-ffi` (Cargo)

---

## 목차

1. [개요](#1-개요)
2. [빌드 산출물](#2-빌드-산출물)
3. [레이어 구조](#3-레이어-구조)
4. [네임스페이스별 모듈 설명](#4-네임스페이스별-모듈-설명)
5. [요청 처리 흐름](#5-요청-처리-흐름)
6. [분석 패스 순서](#6-분석-패스-순서)
7. [FFI 경계 (C++ ↔ Rust)](#7-ffi-경계-c--rust)
8. [핵심 자료구조](#8-핵심-자료구조)
9. [설정 및 경로](#9-설정-및-경로)
10. [확장 가이드](#10-확장-가이드)

---

## 1. 개요

Fission 디컴파일러의 C++ 핵심 계층입니다.  
Ghidra의 libdecomp 라이브러리를 래핑하여 다음 두 가지 진입점을 제공합니다.

| 진입점 | 용도 |
|--------|------|
| `fission_decomp` (실행파일) | CLI / 파이프 프로세스 — Rust `fission-analysis`가 stdin/stdout으로 통신 |
| `libdecomp.dylib/.so` (공유 라이브러리) | in-process FFI — Rust `fission-ffi` crate가 `dlopen` 없이 직접 링크 |

---

## 2. 빌드 산출물

```
ghidra_decompiler/build/
├── fission_decomp          # CLI 실행파일 (CORE_SOURCES + FISSION_SOURCES)
├── libdecomp.dylib         # FFI 공유 라이브러리 (+ FFI_SOURCES)
└── fission_context_services_test  # 단위 테스트
```

**CMakeLists.txt 구성**

```
CORE_SOURCES   = Ghidra 원본 .cc 파일 (~50개)
FISSION_SOURCES = fission 모듈 .cc/.cpp (~50개)
FFI_SOURCES    = src/ffi/ 하위 7개 파일 (libdecomp용 추가)
```

---

## 3. 레이어 구조

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust (fission-analysis)                   │
│   stdin/stdout JSON  ──or──  libdecomp FFI (native_decomp)  │
└───────────────────────────┬─────────────────────────────────┘
                            │  C ABI (libdecomp_ffi.h)
┌───────────────────────────▼─────────────────────────────────┐
│                  fission::decompiler 계층                    │
│  DecompilationPipeline  AnalysisPipeline  PostProcessPipeline│
│  DecompilationCore      CFGStructurizer   PcodeExtractor     │
│  PcodeOptimizationBridge                                     │
└───────┬───────────────┬──────────────────┬──────────────────┘
        │               │                  │
┌───────▼──────┐ ┌──────▼──────┐ ┌────────▼────────┐
│fission::      │ │fission::    │ │fission::         │
│analysis       │ │types        │ │processing        │
│               │ │             │ │                  │
│CallGraphAnal. │ │StructureAna.│ │StringScanner     │
│TypePropagator │ │GlobalTypeReg│ │PostProcessors    │
│VTableAnalyzer │ │RttiAnalyzer │ │Constants         │
│FidDatabase    │ │PatternLoader│ │CFGStructurizer   │
│CallingConvDet.│ │TypeManager  │ └──────────────────┘
│GlobalDataAnal.│ │GuidParser   │
│TypeSharing    │ │GdtBinaryPars│
│EmulationAnal. │ └─────────────┘
│InternalMatcher│
└───────┬───────┘
        │
┌───────▼──────────────────────────────────────────────────────┐
│                    fission::loader / fission::core            │
│  BinaryDetector   MemoryImage   CliArchitecture               │
│  DecompilerContext  DataSymbolRegistry  ContextServices        │
└───────────────────────────┬──────────────────────────────────┘
                            │
┌───────────────────────────▼──────────────────────────────────┐
│                    Ghidra libdecomp (C++)                     │
│  Sleigh ISA  ·  PcodeOp IR  ·  Decompiler Actions           │
│  TypeDB  ·  Scope/Symbol  ·  Varnode/Funcdata               │
└──────────────────────────────────────────────────────────────┘
```

---

## 4. 네임스페이스별 모듈 설명

### 4.1 `fission::decompiler` — 최상위 파이프라인

| 파일 | 역할 |
|------|------|
| `DecompilationPipeline.cc` | `handle_load_bin()` / `handle_decompile()` — 전체 워크플로 조율 |
| `AnalysisPipeline.cpp` | `run_analysis_passes()` — 구조체·타입·전역·콜그래프 패스 순서 제어 |
| `DecompilationCore.cpp` | stdin/stdout JSON 루프, 요청 라우팅 |
| `PostProcessPipeline.cpp` | 디컴파일 결과 텍스트 후처리 파이프라인 |
| `PostProcessor.cc` | 조건식 단순화, 배열 초기화, 변수명 개선 |
| `CFGStructurizer.cc` | `goto` 제거, 루프 정규화 |
| `PcodeExtractor.cc` | Ghidra Funcdata → JSON Pcode 직렬화 |
| `PcodeOptimizationBridge.cc` | Rust Pcode 최적화기 연결 (push-registration / dlsym 우회) |
| `Limits.h` | `k_callee_follow_limit = 0x4000` 등 상수 중앙 관리 |

**`run_analysis_passes()` 두 오버로드**

```
FFI 경로:   run_analysis_passes(ffi::DecompContext*, fd, action, max_size)
배치 경로:  run_analysis_passes(BatchAnalysisContext&, fd, action, max_size)
```

두 경로 모두 동일한 자유 함수들을 공유하며, 컨텍스트 소스만 다릅니다.

---

### 4.2 `fission::analysis` — 분석 패스

| 클래스 | 입력 | 출력 |
|--------|------|------|
| `StructureAnalyzer` | `Funcdata*` | `TypeStruct*` 추론 결과, struct 정의 문자열 |
| `TypePropagator` | `Funcdata*` + `struct_registry` + `compiler_id` | 타입 주석 주입 횟수 / POSIX·Windows API 타입 |
| `CallGraphAnalyzer` | `Funcdata*` + `GlobalTypeRegistry*` | 콜 엣지, 타입 힌트 전파, pending reanalysis 주소 |
| `GlobalDataAnalyzer` | `Funcdata*` + data section 범위 | 전역 구조체 심볼 |
| `VTableAnalyzer` | 바이너리 바이트 + 이미지 베이스 | vtable 맵, 슬롯 힌트 |
| `TypeSharing` | `Funcdata*` | 함수 간 타입 공유 횟수 |
| `CallingConvDetector` | `Funcdata*` | `CallingConvention` 열거값 |
| `FidDatabase` | `.fidb` 경로 | 주소 → 함수명 매핑 |
| `EmulationAnalyzer` | `Funcdata*` | 에뮬레이션 기반 메타태그 |
| `InternalMatcher` | 바이너리 바이트 | 내부 패턴 매칭 결과 |

---

### 4.3 `fission::types` — 타입 시스템

| 클래스 | 역할 |
|--------|------|
| `GlobalTypeRegistry` | 함수 시그니처 레지스트리 + pending reanalysis 큐 (O(1) unordered_set) |
| `RttiAnalyzer` | PE/ELF RTTI에서 클래스 명 복원 |
| `StructureAnalyzer` | `Funcdata` 내 메모리 접근 패턴 → 구조체 추론 |
| `PatternLoader` | 바이트 패턴 → 표준 라이브러리 함수명 매핑 |
| `PrototypeEnforcer` | IAT 심볼 함수 프로토타입 강제 적용 |
| `GdtBinaryParser` | Ghidra `.gdt` 이진 파일 파싱 |
| `GuidParser` | Windows GUID → 이름 매핑 |
| `TypeManager` / `TypeResolver` | 타입 생성 및 해석 유틸리티 |

---

### 4.4 `fission::loader` — 바이너리 적재

| 클래스 | 역할 |
|--------|------|
| `BinaryDetector` | PE / ELF / Mach-O 매직 바이트 감지, `BinaryInfo` + `SectionInfo` 파싱 |
| `MemoryImage` / `MemoryLoadImage` | Ghidra `LoadImage` 구현 — 메모리 블록 기반 |
| `SectionAwareLoadImage` | 섹션 정보를 포함한 확장 LoadImage |
| `SymbolLoader` | IAT 심볼, export 테이블 적재 |

**`BinaryInfo` 구조 (C-1 이후)**

```cpp
struct SectionInfo {
    std::string name;      // ".text", ".rodata", "__TEXT" …
    uint64_t    va_addr;
    uint64_t    va_size;
    bool        is_executable;
};

struct BinaryInfo {
    BinaryFormat format;   // PE | ELF | MACHO
    ArchType     arch;     // X86 | X86_64 | ARM | ARM64
    bool         is_64bit;
    uint64_t     image_base;
    uint64_t     entry_point;
    std::string  sleigh_id;    // "x86:LE:64:default"
    std::string  compiler_id;  // "windows" | "gcc" | "clang"
    std::vector<SectionInfo> sections;
};
```

---

### 4.5 `fission::core` — 아키텍처 컨텍스트

| 클래스 | 역할 |
|--------|------|
| `DecompilerContext` | 배치 모드 영구 상태 — arch 객체, 심볼맵, 타입 레지스트리, 섹션 범위 |
| `CliArchitecture` | Ghidra `Architecture` 서브클래스 (Sleigh 기반) |
| `ContextServices` | Ghidra `ContextDatabase` 확장 |
| `DataSymbolRegistry` | `.rdata`/`.rodata`/`__const` 부동소수점 상수 심볼 등록 |
| `ScopeFission` | Ghidra `Scope` 확장 — Fission 전용 심볼 조회 |

---

### 4.6 `fission::ffi` — libdecomp C ABI

| 파일 | 역할 |
|------|------|
| `libdecomp_ffi.h` | 공개 C ABI 헤더 (`DECOMP_API` 마크 함수 전체) |
| `libdecomp_ffi.cpp` | 구현 진입점 — 하위 Manager로 위임 |
| `DecompContext.h/.cpp` | FFI용 불투명 컨텍스트 구조체 |
| `MemoryManager.cpp` | `decomp_add_memory_block()` |
| `SymbolManager.cpp` | `decomp_add_symbol()`, `decomp_add_symbols_batch()` |
| `FidManager.cpp` | `decomp_load_fid_db()`, `decomp_get_fid_match()` |
| `DecompilerCore.cpp` | `decomp_function()` — 실 디컴파일 실행 |
| `SymbolProviderManager.cpp` | 콜백 기반 심볼 제공자 등록 |

---

### 4.7 `fission::processing` — 텍스트 후처리

| 클래스 | 역할 |
|--------|------|
| `StringScanner` | 바이너리 섹션에서 ASCII/UTF-16 문자열 추출 |
| `PostProcessors` | IAT 호출 치환, 상수 치환 |
| `Constants` | 상수 → 이름 매핑 |

---

## 5. 요청 처리 흐름

### 5.1 `load_bin` 요청

```
Rust (stdin JSON)
  ↓  cmd: "load_bin"
DecompilationCore::run()
  ↓
DecompilationPipeline::handle_load_bin()
  ├─ Phase 0: BinaryDetector::detect()  → BinaryInfo (format, arch, sections)
  ├─ Phase 1: CliArchitecture::setup_architecture()  → Ghidra arch 객체
  ├─ Phase 2: SymbolLoader  → IAT/export 심볼 적재
  ├─ Phase 3: RttiAnalyzer::recover_class_names()
  ├─ Phase 4: VTableAnalyzer::scan_vtables()
  ├─ Phase 5: GlobalDataAnalyzer::set_data_section()  (BinaryInfo.sections 사용)
  ├─ Phase 6: PatternLoader::match_functions()
  ├─ Phase 7: StringScanner 스트링 사전 스캔
  ├─ Phase 8: DataSymbolRegistry
  ├─ Phase 9: PrototypeEnforcer
  └─ Phase 10: FidDatabase 적재

  state.executable_ranges ← bin_info.sections[is_executable]
  → JSON {"status":"ok","functions":[...]}
```

### 5.2 `decompile` 요청

```
Rust (stdin JSON)
  ↓  cmd: "decompile", addr: 0xXXXX
DecompilationPipeline::handle_decompile()
  ├─ Step 1: VTable/vcall 이름 주입
  ├─ Step 2: CallingConvDetector
  ├─ Step 3: PrototypeEnforcer
  ├─ Step 4: arch->allacts.getCurrent()->perform(*fd)  ← Ghidra 핵심 디컴파일
  ├─ Step 4b: run_analysis_passes(BatchAnalysisContext&, ...)
  │     ├─ Stage 1: StructureAnalyzer → TypePropagator → GlobalDataAnalyzer
  │     │   └─ [Barrier 1] rerun_action() if any changed
  │     ├─ Stage 2: CallGraphAnalyzer → TypeSharing → PcodeOptimizationBridge
  │     │   └─ [Barrier 2] rerun_action() if any changed
  │     └─ TypePropagator::propagate_call_return_types()
  ├─ Step 4c: EmulationAnalyzer
  ├─ Step 5: arch->print->docFunction()  → C 코드 문자열
  └─ Step 6: PostProcessPipeline
        ├─ IAT 심볼 치환
        ├─ 상수 치환
        ├─ 문자열 인라이닝 (.rdata/.rodata/__cstring/__const/.data.rel.ro)
        ├─ GUID 치환
        └─ Unicode 문자열 복원
  → JSON {"status":"ok","code":"..."}
```

---

## 6. 분석 패스 순서

`run_analysis_passes()` 내부의 실행 순서 및 재실행 트리거:

```
[배치 경로 BatchAnalysisContext]

1. StructureAnalyzer::analyze_function_structures()
   → Pcode의 PTRSUB/PTRADD 패턴으로 구조체 멤버 추론
   → 성공 시 needs_rerun_stage1 = true

2. TypePropagator::propagate_struct_types()   (역방향)
3. TypePropagator::propagate()                (전방향)
4. TypePropagator::propagate_struct_types()   (재확인)
   → 변화 있으면 needs_rerun_stage1 = true

5. GlobalDataAnalyzer::analyze_function() + create_types()
   → 전역 변수 구조체 심볼 등록
   → 성공 시 needs_rerun_stage1 = true

━━━━━━━━━━━━ Barrier 1: rerun_action() (최대 1회) ━━━━━━━━━━━━

6. CallGraphAnalyzer::extract_calls()      (CPUI_CALL + CPUI_CALLIND)
7. CallGraphAnalyzer::propagate_types()
8. pending reanalysis 루프 (max_rounds=2)
   → CPUI_CALLIND 포함, 16KB followFlow (k_callee_follow_limit)
   → 변화 있으면 needs_rerun_stage2 = true

9. TypeSharing::register_function_types() + share_types()

10. PcodeOptimizationBridge::extract_and_optimize()
    → Rust Pcode 최적화기 (push-registered 함수 포인터)
    → 주입 성공 시 needs_rerun_stage2 = true

11. TypePropagator::propagate_call_return_types()

━━━━━━━━━━━━ Barrier 2: rerun_action() (최대 1회) ━━━━━━━━━━━━
```

**재실행 횟수 보장**: 이전 최대 6회 → 現 최대 2회 (B-1 최적화)

---

## 7. FFI 경계 (C++ ↔ Rust)

### 7.1 libdecomp C ABI

```
libdecomp_ffi.h  (DECOMP_API = __attribute__((visibility("default"))))
  decomp_create(sla_dir)        → DecompContext*
  decomp_load_binary(...)       → DecompError
  decomp_add_memory_block(...)  → DecompError
  decomp_add_symbol(addr, name)
  decomp_add_symbols_batch(...) → 배치 등록 (FFI 오버헤드 감소)
  decomp_function(addr)         → char*  (호출자가 decomp_free_string 호출)
  decomp_set_gdt(path)
  decomp_load_fid_db(path)
  decomp_register_struct_type(...)
  decomp_init_pcode_bridge(optimize_fn, free_fn)  ← push-registration
  decomp_destroy(ctx)
```

### 7.2 Pcode 최적화 브릿지

```
[시작 시]
Rust DecompilerNative::new()
  └─ decomp_init_pcode_bridge(
         fission_optimize_pcode_json,   // Rust #[no_mangle]
         fission_free_string            // Rust #[no_mangle]
     )
  └─ PcodeOptimizationBridge::register_rust_fn_ptrs()
         → rust_optimize_fn / rust_free_fn 저장
         → ffi_attempted = true (dlsym 재시도 방지)

[디컴파일 시]
PcodeOptimizationBridge::extract_and_optimize(fd)
  ├─ PcodeExtractor::extract_pcode_json(fd)  → JSON
  └─ rust_optimize_fn(json, len)             → 최적화된 JSON
```

**이전 방식 (dlsym 기반)의 문제:**  
macOS에서 Rust 실행파일 심볼은 `RTLD_DEFAULT` 검색 범위에 없음 —  
`push-registration`으로 완전 해결.

### 7.3 심볼 제공자 (콜백 기반)

```cpp
// Rust가 C 함수 포인터를 등록
decomp_set_symbol_provider(ctx, &DecompSymbolProvider{
    .find_symbol   = symbol_provider_find_symbol,
    .find_function = symbol_provider_find_function,
    .userdata      = rust_state_ptr,
    .drop          = cleanup_fn,
});
```

---

## 8. 핵심 자료구조

### 8.1 `core::DecompilerContext` — 배치 영구 상태

```cpp
struct DecompilerContext {
    // Ghidra 아키텍처 (is_64bit별 캐시)
    CliArchitecture* arch_64bit;
    CliArchitecture* arch_32bit;

    // 심볼 맵
    std::map<uint64_t, std::string>  iat_symbols;     // IAT 주소 → 이름
    std::map<uint64_t, std::string>  fid_function_names;

    // VTable / vcall 힌트
    std::map<uint64_t, std::map<int, std::string>>  vtable_virtual_names;
    std::map<int, std::string>                       vcall_slot_name_hints;

    // 타입 레지스트리 (콜그래프 전파용)
    fission::types::GlobalTypeRegistry  type_registry;

    // 구조체 레지스트리: func_addr → (param_idx → struct_name)
    std::map<uint64_t, std::map<int, std::string>>  struct_registry;

    // 섹션 정보 (C-1 이후)
    uint64_t  data_section_start;
    uint64_t  data_section_end;
    std::vector<std::pair<uint64_t,uint64_t>>  executable_ranges;
};
```

### 8.2 `decompiler::BatchAnalysisContext` — 패스간 컨텍스트

```cpp
struct BatchAnalysisContext {
    ghidra::Architecture*                            arch;
    fission::types::GlobalTypeRegistry*              type_registry;
    std::map<uint64_t, std::string>*                 symbols;
    std::map<uint64_t,std::map<int,std::string>>*    struct_registry;
    std::vector<std::pair<uint64_t,uint64_t>>        executable_ranges;
    uint64_t  data_start;
    uint64_t  data_end;
};
```

### 8.3 `types::GlobalTypeRegistry` — O(1) 중복 방지

```cpp
struct GlobalTypeRegistry {
    std::map<uint64_t, FunctionSignature>  function_signatures;
    std::unordered_set<uint64_t>           pending_reanalysis;  // B-2: O(1)

    void mark_for_reanalysis(uint64_t addr) { pending_reanalysis.insert(addr); }
    std::vector<uint64_t> consume_pending_reanalysis();
};
```

---

## 9. 설정 및 경로

### 9.1 `config::PathConfig`

```
fission.toml (프로젝트 루트)
  └─ sla_dir      → Sleigh ISA 정의 파일 위치
  └─ gdt_dir      → .gdt 타입 데이터베이스 위치
  └─ fid_dir      → .fidb 함수 시그니처 DB 위치
```

### 9.2 분석 피처 (`decomp_set_feature`)

| 피처 이름 | 기본값 | 설명 |
|-----------|--------|------|
| `infer_pointers` | on | 포인터 반환 타입 추론 |
| `analyze_loops` | on | 루프 패턴 분석 |
| `readonly_propagate` | on | 읽기 전용 값 전파 |
| `disable_toomanyinstructions_error` | off | 대형 함수 오류 억제 |

### 9.3 Limits (`decompiler/Limits.h`)

| 상수 | 값 | 용도 |
|------|----|------|
| `k_callee_follow_limit` | `0x4000` (16KB) | followFlow 상한 |
| `MAX_FUNCTION_SIZE` | `10000` (10KB) | 구조체 복원 건너뜀 기준 |
| `MAX_PTRSUB_OPS` | `100` | PTRSUB 분석 상한 |

---

## 10. 확장 가이드

### 새 분석 패스 추가

1. `include/fission/analysis/MyAnalyzer.h` — 클래스 선언
2. `src/analysis/MyAnalyzer.cc` — 구현
3. `CMakeLists.txt` `FISSION_SOURCES`에 추가
4. `AnalysisPipeline.cpp` → `run_analysis_passes()` 내 적절한 배리어 앞/뒤에 삽입

```cpp
// Stage 1 (Barrier 1 전) — 구조체/타입에 영향 주는 분석
MyAnalyzer my;
if (my.analyze(fd)) needs_rerun_stage1 = true;

// Stage 2 (Barrier 2 전) — 콜그래프/최적화 후 분석
```

### 새 플랫폼 타입 추론 추가

`TypePropagator::infer_posix_api_types()` 또는 `infer_windows_api_types()` 내에  
함수명 기반 분기를 추가합니다(`func_name == "myfunc"`) .

### 새 바이너리 형식 지원

1. `BinaryDetector::is_xxx()` 매직 바이트 체크 추가
2. `BinaryDetector::parse_xxx()` — `BinaryInfo.sections` 채우기
3. `BinaryDetector::detect()` 분기 추가

### FFI 함수 추가

1. `libdecomp_ffi.h` — `DECOMP_API` 선언
2. `libdecomp_ffi.cpp` — 구현 (Manager로 위임 권장)
3. `crates/fission-ffi/src/decomp.rs` `unsafe extern "C"` 블록에 선언

---

## 부록 — 모듈 의존 그래프 (주요 경로)

```
DecompilationPipeline
  ├── BinaryDetector          (loader)
  ├── CliArchitecture         (core)
  ├── VTableAnalyzer          (analysis)
  ├── RttiAnalyzer            (types)
  ├── run_analysis_passes()   (decompiler)
  │     ├── StructureAnalyzer       (types)
  │     ├── TypePropagator          (analysis)
  │     ├── GlobalDataAnalyzer      (analysis)
  │     ├── CallGraphAnalyzer       (analysis)
  │     │     └── GlobalTypeRegistry  (types)
  │     ├── TypeSharing             (analysis)
  │     └── PcodeOptimizationBridge (decompiler)
  │           └── [Rust fission-pcode via push-registered fn ptr]
  └── PostProcessPipeline
        ├── StringScanner     (processing)
        ├── PostProcessors    (processing)
        └── GuidParser        (types)
```
