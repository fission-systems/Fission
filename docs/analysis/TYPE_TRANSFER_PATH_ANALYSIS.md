# 타입 전달 경로 분석 (Type Transfer Path Analysis)

## 요약: 발견된 "끊어진 다리"

**C++ StructureAnalyzer / TypePropagator가 추론한 타입 정보가 Rust 쪽 `PassContext.inferred_types`로 전달되는 경로가 존재하지 않습니다.**

| 구간 | 데이터 | 상태 |
|------|--------|------|
| C++ 생성 | `StructureAnalyzer::inferred_structs`, `get_type_replacements()` | ✅ 정상 |
| C++ 직렬화 | `decomp_function` 반환값 | **C 코드 문자열만** 반환 (타입 정보 없음) |
| Rust 역직렬화 | `DecompilerNative::decompile()` | C 문자열만 수신 |
| Rust 매핑 | `PassContext.inferred_types` | **항상 `binary.inferred_types`** (로더 전용) |
| Rust 적용 | `replace_field_offsets` | `inferred_types`가 비어 있으면 **아무 작업 안 함** |

---

## 1. 현재 데이터 플로우 상세

### 1.1 C++ 측 (생성 & 사용)

```
AnalysisPipeline::run_analysis_passes()
  └─ StructureAnalyzer::analyze_function_structures(fd)
       └─ inferred_structs, get_type_replacements()  ← offset → "struct.field" 맵 생성
  └─ artifacts.type_replacements = struct_analyzer.get_type_replacements()
  └─ artifacts.captured_structs = struct_analyzer.get_inferred_structs()

DecompilationCore::run_decompilation()
  └─ analysis = run_analysis_passes(...)
  └─ result = docFunction(fd)  ← Ghidra C 출력
  └─ result = run_post_processing(ctx, fd, result, analysis, options)
       └─ annotate_structure_offsets(result, analysis.type_replacements)
            └─ *(ptr+0x18) → ptr->field_name 변환 (C++ 내부 완료)
  └─ return result  ← C 문자열만 반환
```

- C++는 `type_replacements`를 사용해 `annotate_structure_offsets`에서 `*(ptr+0x18)` → `ptr->field_name` 변환을 수행합니다.
- 이 작업은 C++ 내부에서만 이루어지며, 추론된 타입 정보는 FFI를 통해 Rust로 전달되지 않습니다.

### 1.2 FFI 경계 (libdecomp_ffi.cpp)

```cpp
// decomp_function: C 코드 문자열만 반환
char* decomp_function(DecompContext* ctx, uint64_t addr) {
    std::string result = run_decompilation(ctx, addr);  // C++ 전체 파이프라인
    char* output = malloc(result.size() + 1);
    memcpy(output, result.c_str(), ...);
    return output;  // ← 타입 메타데이터 없음
}
```

- 반환값은 순수 C 코드 문자열뿐입니다.
- 구조체/타입 메타데이터를 내보내는 API가 없습니다.

### 1.3 Rust 측 (수신 & 사용)

```rust
// legacy-ffi-bridge-crate wrapper.rs
decomp.decompile(addr)  // → String (C 코드만)

// fission-analysis decomp
// inferred_types는 항상 binary(Loader)에서 옴
let processor = PostProcessor::new()
    .with_inferred_types(self.inferred_types.clone());  // ← binary.inferred_types

// binary.inferred_types 출처 (fission-loader)
// - DWARF debug info
// - C++ RTTI (RttiAnalyzer)
// - Go type info
// - Mach-O Swift types
// → 디컴파일러(StructureAnalyzer) 결과와 무관
```

- `PassContext.inferred_types`는 항상 `LoadedBinary.inferred_types`를 사용합니다.
- 이 값은 로더 단계(DWARF, C++ RTTI 등)에서만 채워지며, Ghidra 디컴파일러의 `StructureAnalyzer` 결과와 연결되지 않습니다.

---

## 2. 두 소스의 역할 비교

| 소스 | offset→field 맵 | 시점 | Stripped 바이너리 |
|------|------------------|------|-------------------|
| **로더** (DWARF, RTTI) | `binary.inferred_types` | 바이너리 로드 시 | 대부분 비어 있음 |
| **C++ StructureAnalyzer** | `get_type_replacements()` | 함수별 디컴파일 시 | 동작함 (P-code 패턴 분석) |

- Stripped 바이너리에서는 로더 타입이 거의 없고, StructureAnalyzer만 offset→field를 추론합니다.
- 하지만 StructureAnalyzer 결과는 Rust로 전달되지 않아, Rust `replace_field_offsets`가 빈 맵으로 동작합니다.

---

## 3. 영향

1. **Stripped 바이너리**: `replace_field_offsets`가 사실상 동작하지 않음 (`inferred_types`가 비어 있음).
2. **C++에서 놓친 패턴**: C++ `annotate_structure_offsets`의 정규식이 매칭하지 못하는 `*(ptr+0x18)` 형태가 남을 수 있음. Rust에서 보완하려 해도 offset 맵이 없어 불가능.
3. **중복 추론**: 로더와 StructureAnalyzer가 각각 타입을 추론하지만, Rust는 로더 결과만 사용함.

---

## 4. 권장 해결 방안

### 4.1 Option A: FFI로 타입 메타데이터 반환 (권장) — ✅ 구현 완료

1. **새 FFI 함수** `decomp_function_with_metadata(ctx, addr)`:
   - 반환 JSON: `{"code":"...","inferred_types":[{...}]}`
   - `inferred_types`는 `StructureAnalyzer::captured_structs`를 `InferredTypeInfo` 형식으로 직렬화

2. **Rust 수신**:
   - `DecompilerNative::decompile_with_metadata()` → `DecompilationResult { code, inferred_types }`
   - `CachingDecompiler`, oneshot decompile: `[decompiler.inferred_types, binary.inferred_types]` 병합 후 `PostProcessor`에 전달

3. **수정 파일**: `DecompilationCore.cpp`, `libdecomp_ffi`, `wrapper.rs`, `decompile.rs`, `mod.rs` (CachingDecompiler)

### 4.2 Option B: 디컴파일 직후 타입 콜백

- `decomp_register_type_callback(ctx, callback)` 형태로  
  디컴파일 완료 시 `(addr, type_replacements)`를 콜백으로 전달.
- Rust에서 이 콜백을 구현해 `inferred_types`를 갱신.

### 4.3 Option C: C++ 후처리 강화

- C++ `annotate_structure_offsets`의 정규식을 넓혀  
  가능한 한 모든 `*(ptr+0x18)` 패턴을 C++ 단에서 처리.
- Rust `replace_field_offsets`는 주로 DWARF/RTTI 기반 보조로만 사용.

- **한계**: C++와 Rust의 패턴이 달라 Rust 쪽에서 추가로 보완하고 싶을 때 타입 정보 부족 문제는 그대로 남음.

---

## 5. 관련 파일 참조

| 역할 | 파일 |
|------|------|
| C++ 타입 추론 | `legacy-native-decompiler-tree/src/types/StructureAnalyzer.cc` |
| C++ 후처리 (offset→field) | `legacy-native-decompiler-tree/src/processing/passes/CodeCleanupPasses.cc` (`annotate_structure_offsets`) |
| FFI 반환 | `legacy-native-decompiler-tree/src/ffi/libdecomp_ffi.cpp` (`decomp_function`) |
| Rust 수신 | `crates/legacy-ffi-bridge-crate/src/decomp/wrapper.rs` (`decompile`) |
| Rust 타입 사용 | `crates/fission-analysis/src/analysis/decomp/postprocess/naming.rs` (`replace_field_offsets`) |
| PassContext 정의 | `crates/fission-analysis/src/analysis/decomp/postprocess/pass.rs` |
| 로더 inferred_types | `crates/fission-loader/src/loader/mod.rs`, `types.rs` |

---

*분석일: 2026-03*
