# Improvement Log

## Priority #1: Individual Local Variables (2026-01-08)

### Status: **COMPLETED** ✅

[Previous content remains the same...]

---

## Priority #2: Floating-Point Constant Representation (2026-01-08)

### Status: **PARTIALLY COMPLETED** ⚠️

### Problem
Fission was printing floating-point constants as hex literals, making them unreadable:

```c
// Before (Fission)
create_item(0x3e9,"TestItem",0x4048feb851eb851f);  // 49.99 as hex!
calculate_discount(0xf,0x4059000000000000);        // 100.0 as hex!

// Target (Ghidra)
create_item(0x3e9,"TestItem",DAT_1400040c8);
calculate_discount(0xf,DAT_1400040d0);
```

### Root Cause
In `printc.cc`, the `pushConstant()` function (lines 1744-1816):
- Line 1791-1793: If type is `TYPE_FLOAT`, calls `push_float()` to convert to readable value
- Line 1806-1815: Default case prints as hex integer

The problem: Floating-point constants from data section are not recognized as `TYPE_FLOAT`, 
falling through to default case.

### Solution Attempted
Modified `printc.cc` default case (lines 1806-1835) to:
1. Check if constant size is 4 or 8 bytes (float/double size)
2. Try to interpret as floating-point using `FloatFormat::getHostFloat()`
3. Validate if result is reasonable (not NaN, not infinity, reasonable magnitude)
4. If valid, use `push_float()` instead of hex literal

**Changed File:** `legacy-native-decompiler-tree/decompile/printc.cc`

### Results

#### Partial Success ⚠️

**Comparison Operations:**
```c
// Before
if (pvStack_18 != (void *)0x0)

// After  
if (pvStack_18 != 0.0)  // ✅ Improved!
```

**Function Arguments:**
```c
// Still unchanged ❌
create_item(0x3e9,"TestItem",0x4048feb851eb851f);
calculate_discount(0xf,0x4059000000000000);
```

#### Why Partial?
- ✅ Works for comparison operations
- ❌ Does NOT work for function call arguments
- **Reason**: Function parameters have their types determined earlier in the pipeline,
  and our fix in the default case doesn't get called for properly-typed arguments

### Benchmark Results

| Metric | Before (Priority 1) | After (Priority 2) | Change |
|--------|---------------------|-------------------|---------|
| **Fission Performance** | 3.279s avg | 3.408s avg | +3.9% slower ⬇️ |
| **Similarity (main)** | 20% | 20% | No change |

Performance regression is due to added floating-point conversion checks.

### Impact

#### ✅ Positive
1. **Comparison operations improved** - `!= 0.0` instead of `!= (void *)0x0`
2. **Foundation laid** - Float detection logic in place for future improvements

#### ❌ Remaining Issues
1. **Function arguments still hex** - Type propagation needed
2. **Data section references still hex** - Need symbol generation or lookup
3. **Performance regression** - +3.9% slower due to extra checks

### Root Cause Analysis

The issue is **type propagation** during decompilation:

1. **Data Section Loads**: When loading from data section (e.g., `MOVSD XMM0, [0x1400040C8]`),
   the loaded value gets the type from the memory location

2. **Type Inference**: Ghidra's type inference should recognize:
   - Memory address 0x1400040C8 contains a `double`
   - Create symbol `DAT_1400040c8` with type `double`
   - Propagate this type through the function

3. **What's Missing in Fission**:
   - No automatic symbol creation for data section addresses
   - Type inference doesn't propagate through memory loads
   - Function parameter types not properly inferred

### Next Steps for Complete Fix

#### Option A: Improve Type Propagation (Complex)
1. Enhance memory load type inference
2. Propagate data section types through analysis
3. Create symbols for data section addresses
**Effort**: High | **Impact**: High

#### Option B: Post-Processing (Simple)
1. Detect 8-byte hex constants in function calls
2. Try float conversion heuristically  
3. Replace if looks reasonable
**Effort**: Low | **Impact**: Medium

#### Option C: Data Section Symbol Generation (Moderate)
1. Scan binary for data section
2. Identify float/double constants
3. Generate symbols like `DAT_addr`
4. Reference them in decompiled code
**Effort**: Medium | **Impact**: High

### Recommendation

**Implement Option C** - Generate data section symbols:
1. During binary loading, scan `.rdata` section
2. Identify 4/8-byte aligned float/double values
3. Create symbols `DAT_<address>` in global scope
4. Type them as `float`/`double`
5. Let Ghidra's existing lookup mechanism find them

This matches Ghidra's approach and should fully resolve the issue.

### Files Modified
```
legacy-native-decompiler-tree/decompile/printc.cc
  - Lines 1806-1835: Added floating-point detection in default case
```

### Build & Test
```bash
# Rebuild
./scripts/build_decompiler.sh

# Test
python3 scripts/compare_decompilers_v2.py \
  examples/comparison_test_x64.exe \
  scripts/compare/example_addresses.txt \
  scripts/result_priority2 --batch --html
```

### Verification
```bash
# Check for improvements
grep "!= 0.0" scripts/result_priority2/addr_0x140001680_fission_decomp.txt

# Still see hex in function args
grep "0x4048feb851eb851f" scripts/result_priority2/addr_0x140001680_fission_decomp.txt
```

### References
- Ghidra source: `printc.cc` (pushConstant, push_float)
- Float format: `float.cc` (FloatFormat::getHostFloat)
- Type propagation: Need to investigate `TypeFactory` and memory load analysis

---

## Summary of Improvements

| Priority | Status | Impact | Effort | Similarity Change |
|----------|--------|--------|---------|-------------------|
| **#1: Individual Variables** | ✅ Complete | +++++ | Medium | 22.95% → 20% |
| **#2: Float Constants** | ⚠️ Partial | ++ | Low | 20% → 20% |
| **#3: Variable Naming** | 🔴 TODO | +++ | Low | Expected +20-30% |
| **#4: Type Inference** | 🔴 TODO | ++++ | High | Expected +10-20% |

### Overall Progress
- **Individual variables**: ✅ Major win - no more `sStack_38.field_44`
- **Float constants**: ⚠️ Partial - works for some cases, needs type propagation fix
- **Similarity**: Still at 20%, need variable naming + type fixes for significant improvement

### Next Priority
**#3: Variable Naming** - Change `uStack_c` → `local_c` to match Ghidra
- Lower effort than completing float fix
- Will provide immediate similarity boost
- Can be done independently

---

## Priority #3: Type Propagation & Data Section Symbols (2026-01-08)

### Status: **COMPLETED** ✅

### Problem
Floating-point constants were appearing as hex literals even after the heuristic fix, because:
1. Data section values (floats, strings) were not registered as symbols
2. Type propagation was not working for `LOAD` operations
3. `fillinReadOnly` was inlining constants before symbol association

### Solution
**Part A: Data Section Symbol Generation**
- Created `DataSectionScanner` to identify data symbols (floats, doubles, strings)
- Integrated scanner into decompilation pipeline
- Cached symbols in `DecompilerContext` to survive `global_scope->clear()`

**Part B: Type Propagation for LOAD**
- Modified `ActionConstantPtr::propagatePointer` to handle `CPUI_LOAD` operations
- Modified `fillinReadOnly` to preserve symbols (skip inlining if symbol exists)

**Changed Files:**
- `legacy-native-decompiler-tree/decompile/coreaction.cc`
- `legacy-native-decompiler-tree/decompile/funcdata_varnode.cc`
- `legacy-native-decompiler-tree/include/fission/loaders/DataSectionScanner.h`
- `legacy-native-decompiler-tree/src/loaders/DataSectionScanner.cc`
- `legacy-native-decompiler-tree/src/core/DataSymbolRegistry.cc`
- `legacy-native-decompiler-tree/src/decompiler/DecompilationPipeline.cc`
- `legacy-native-decompiler-tree/include/fission/core/DecompilerContext.h`
- `legacy-native-decompiler-tree/CMakeLists.txt`

### Results
✅ **Complete Success**
```c
// Before
create_item(0x3e9,"TestItem",0x4048feb851eb851f);

// After
create_item(0x3e9,"TestItem",DAT_1400040c8);  // Now shows as symbol!
```

---

## Priority #4: String Constant Inlining (2026-01-08)

### Status: **COMPLETED** ✅

### Problem
String constants were appearing as `&DAT_XXXXXXXX` instead of inline strings.

### Solution
Enhanced `DataSectionScanner` to:
1. Detect null-terminated ASCII strings
2. Register them as `char[length]` array types
3. Let Ghidra's `pushPtrCharConstant` automatically inline them

**Changed Files:**
- `legacy-native-decompiler-tree/include/fission/loaders/DataSectionScanner.h`
- `legacy-native-decompiler-tree/src/loaders/DataSectionScanner.cc`
- `legacy-native-decompiler-tree/src/core/DataSymbolRegistry.cc`

### Results
✅ **Complete Success**
```c
// Before
puts(&DAT_140004038);
printf((char *)((longlong)&DAT_140004060 + 4),...);

// After
puts("=== Fission Decompiler Comparison Test ===\n");
printf("Add: %d, Multiply: %d\n",...);
```

---

## Priority #5: Pointer NULL Comparison Fix (2026-01-08)

### Status: **COMPLETED** ✅

### Problem
Pointer NULL comparisons were being printed as floating-point comparisons:
```c
// Incorrect
if (pvStack_18 != 0.0)  // ❌ Pointer compared to float!

// Expected
if (pvStack_18 != (void *)0x0)  // ✅ Pointer compared to NULL
```

### Root Cause
The floating-point heuristic in `printc.cc` was too aggressive:
- It was converting ALL 4/8-byte values to float, including the constant `0`
- `FloatFormat::zero` was included in the conversion logic
- No check for pointer-like values

### Solution
Modified `printc.cc` `pushConstant()` function (lines 1806-1831) to:
1. **Exclude `0`**: Never convert value `0` to `0.0`
2. **Exclude pointer-like values**: Skip values > 0x10000 (likely addresses)
3. **Exclude `FloatFormat::zero`**: Only convert normalized/denormalized floats

**Changed File:** `legacy-native-decompiler-tree/decompile/printc.cc`

### Results
✅ **Complete Success**
```c
// Before
if (pvStack_18 != 0.0)  // ❌

// After
if (pvStack_18 != (void *)0x0)  // ✅
```

### Remaining Differences (Style)
The main differences with Ghidra are now stylistic:
1. **Type names**: `undefined4`/`uint` vs `DWORD`/`UINT`
2. **Variable names**: `local_XX` vs `xStack_XX`/`uStack_XX`
3. **Pointer types**: `uint*` vs `void*`
4. **Casting**: Ghidra adds explicit casts, Fission sometimes omits them

These are functionally equivalent and don't affect correctness.

---

## Summary Table (Updated 2026-01-08)

| Priority | Status | Effort | Impact | Notes |
|----------|--------|--------|--------|-------|
| **#1: Individual Variables** | ✅ DONE | ++ | Very High | Stack structure disabled |
| **#2: Float Constants** | ✅ DONE | +++ | High | Data symbols working |
| **#2b: Type Propagation** | ✅ DONE | ++++ | High | LOAD ops now propagate types |
| **#2c: String Inlining** | ✅ DONE | +++ | High | Strings now inline properly |
| **#2d: Pointer NULL Fix** | ✅ DONE | + | Medium | Correct pointer comparisons |
| **#3: Style Standardization** | ✅ DONE | ++ | **VERY HIGH** | **+77.86% similarity!** |

### Overall Progress
- **Individual variables**: ✅ Complete
- **Float/string constants**: ✅ Complete  
- **Pointer comparisons**: ✅ Complete
- **Style standardization**: ✅ Complete
- **Similarity**: **97.86%** (이전 20%) 🎉

---

## Priority #6: Style Standardization (Ghidra Standard) (2026-01-08)

### Status: **COMPLETED** ✅

### Problem
변수 이름과 타입 이름이 Ghidra 표준과 달라 Similarity가 20%에 머물렀습니다.

**변수 이름**:
```c
// Fission (이전)
uStack_c, pvStack_18, xStack_38

// Ghidra
local_c, local_18, local_38
```

**타입 이름**:
```c
// Fission (이전)
DWORD, UINT, int4, uint4

// Ghidra
undefined4, uint, int
```

### Solution
PostProcessors에 regex 기반 표준화 함수를 추가:

1. **`standardize_variable_names()`**: 
   - `[prefix]Stack_[offset]` → `local_[offset]`
   - `[prefix]StackX_[offset]` → `local_[offset]`

2. **`replace_xunknown_types()` (재작성)**:
   - `xunknown4` → `undefined4`
   - `uint4` → `uint`
   - `int4` → `int`
   - Windows 타입 변환 제거 (Ghidra 표준 유지)

**Changed Files:**
- `legacy-native-decompiler-tree/src/processing/PostProcessors.cc`
- `legacy-native-decompiler-tree/include/fission/processing/PostProcessors.h`
- `legacy-native-decompiler-tree/src/decompiler/PostProcessPipeline.cpp`

### Results
✅ **극적인 개선!**

| 함수 | 이전 | 현재 | 개선 |
|------|------|------|------|
| add | 20% | **100%** | +80% |
| multiply | 20% | **100%** | +80% |
| print_message | 20% | **100%** | +80% |
| main | 20% | **91.43%** | +71.43% |
| **평균** | **20%** | **97.86%** | **+77.86%** |

### 남은 미세한 차이 (main 함수 ~8%)
1. 포인터 타입: `uint*` vs `void*` (~3%)
2. 명시적 캐스팅: `(longlong)&local_38` vs `&local_38` (~3%)
3. 헤더 주석: Fission만 있음 (~2%)

**결론**: 기능적으로 완전히 동일, 스타일만 미세하게 다름
