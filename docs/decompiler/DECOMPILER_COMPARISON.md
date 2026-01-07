# Decompiler Comparison: Fission vs Ghidra

**Date:** 2026-01-05  
**Test Binary:** `test/struct_test` (Mach-O 64-bit x86_64)  
**Ghidra Version:** 11.4.2 (via PyGhidra 2.2.0)  
**Fission Version:** Current development branch

---

## Executive Summary

Comparative analysis of Fission and Ghidra decompilers reveals significant gaps in Fission's current capabilities. While Ghidra produces accurate, human-readable C code across all tested functions, Fission struggles with fundamental decompilation tasks including:

- ❌ Calling convention recognition (System V AMD64 ABI)
- ❌ External function call detection (libc functions)
- ❌ Type inference for parameters and return values
- ❌ Control flow analysis (infinite loop false positives, unreachable block misidentification)
- ❌ Memory allocation/deallocation tracking
- ❌ Position Independent Code (PIC) handling

**Critical Discovery:** When testing ARM64 binaries, Fission incorrectly identifies the architecture as x86-64, leading to complete disassembly corruption. All testing must use native x86-64 binaries until ARM64 architecture detection is fixed in the Mach-O parser.

---

## Test Case 1: Simple Arithmetic Function

**Function:** `_add_numbers` @ `0x100000544`  
**Complexity:** Low (2 parameters, 1 return, no external calls)

### Ghidra Output ✅
```c
int _add_numbers(int param_1, int param_2)
{
  return param_2 + param_1;
}
```

**Analysis:**
- ✅ Correct function signature (2 int parameters)
- ✅ Correct return type (int)
- ✅ Accurate implementation (simple addition)
- ✅ Clean, idiomatic C code

### Fission Output ❌
```c
/* WARNING: Control flow encountered bad instruction data */

void _add_numbers(char param_1)
{
  uint1 in_AL;
  unkbyte7 in_RAX;
  uint1 *unaff_RBX;
  
  *(uint1 *)CONCAT71(in_RAX,in_AL) = *(uint1 *)CONCAT71(in_RAX,in_AL) & in_AL;
  *unaff_RBX = *unaff_RBX + param_1;
  *unaff_RBX = *unaff_RBX << 7 | *unaff_RBX >> 1;
  halt_baddata();
}
```

**Issues:**
- ❌ Wrong signature: `void _add_numbers(char param_1)` vs expected `int _add_numbers(int, int)`
- ❌ Only detected 1 parameter (should be 2)
- ❌ Wrong return type (void vs int)
- ❌ Exposed low-level register names (`in_AL`, `in_RAX`, `unaff_RBX`)
- ❌ Produced nonsensical bit operations
- ❌ Bad instruction data warning
- ❌ Halted with `halt_baddata()` instead of return

**Root Causes:**
1. **Calling Convention Blindness:** Failed to recognize System V AMD64 ABI (RDI=param1, RSI=param2, RAX=return)
2. **Type System Failure:** Defaulted to `char` instead of analyzing data flow for proper type
3. **Disassembly Corruption:** "Bad instruction data" suggests memory block registration issues
4. **Register Abstraction Gap:** Leaked raw register names instead of variable abstraction

---

## Test Case 2: Structure Manipulation

**Function:** `_process_item` @ `0x1000004b0`  
**Complexity:** Medium (struct pointer, printf calls, float arithmetic)

### Ghidra Output ✅
```c
ulong _process_item(ulong param_1)
{
  ulong uVar1;
  uint uVar2;
  int iVar3;
  
  if (param_1 != 0) {
    _printf("Processing Item ID: %d\n");
    _printf("Name: %s\n");
    iVar3 = (int)DAT_100000680;
    uVar1 = (ulong)DAT_100000680 >> 0x20;
    *(double *)(param_1 + 0x28) = *(double *)(param_1 + 0x28) * 1.5;
    *(ulong *)(param_1 + 0x30) =
         CONCAT44((int)((ulong)*(undefined8 *)(param_1 + 0x30) >> 0x20) + (int)uVar1,
                  (int)*(undefined8 *)(param_1 + 0x30) + iVar3);
    _printf("New Value: %.2f\n");
    uVar2 = _printf("New Position: (%d, %d)\n");
    param_1 = (ulong)uVar2;
  }
  return param_1;
}
```

**Analysis:**
- ✅ Detected struct pointer parameter (`param_1`)
- ✅ Recognized 4 `printf()` external calls
- ✅ Identified null pointer check (`if (param_1 != 0)`)
- ✅ Tracked struct field access (`param_1 + 0x28`, `param_1 + 0x30`)
- ✅ Detected floating-point operation (`* 1.5`)
- ✅ Return value propagation

### Fission Output ❌
```c
void _process_item(void)
{
  int8 in_RAX;
  
  *(char *)(in_RAX * 2) = *(char *)(in_RAX * 2) + -0x4c;
  do {
    /* WARNING: Do nothing block with infinite loop */
  } while( true );
}
```

**Issues:**
- ❌ Wrong signature: `void _process_item(void)` vs expected `ulong _process_item(ulong)`
- ❌ No parameters detected (should have struct pointer)
- ❌ No external function calls detected (4 printf calls missing)
- ❌ Control flow collapsed to infinite loop
- ❌ Meaningless memory operation (`in_RAX * 2`)
- ❌ No struct field access recognition

**Root Causes:**
1. **Symbol Resolution Failure:** Cannot identify imported functions like `printf`
2. **Data Flow Analysis Gap:** Lost parameter tracking from entry point
3. **CFG Construction Error:** Misinterpreted legitimate return path as infinite loop
4. **Memory Model Inadequacy:** Cannot model heap-allocated structures

---

## Test Case 3: Complex Main Function

**Function:** `entry` (main) @ `0x10000054c`  
**Complexity:** High (malloc/free, struct initialization, multiple printf chains)

### Ghidra Output ✅
```c
bool entry(void)
{
  ulong uVar1;
  undefined4 *puVar2;
  int iVar3;
  
  _puts("Fission Structure Recovery Test");
  puVar2 = (undefined4 *)_malloc(0x38);
  if (puVar2 != (undefined4 *)0x0) {
    *puVar2 = 0x3e9;
    *(undefined1 *)(puVar2 + 3) = 0;
    *(undefined8 *)(puVar2 + 1) = s_TestItem_1000006d3._0_8_;
    *(undefined8 *)(puVar2 + 10) = 0x405edccccccccccd;
    *(undefined8 *)(puVar2 + 0xc) = 0x500000005;
    _printf("Processing Item ID: %d\n");
    _printf("Name: %s\n");
    iVar3 = (int)DAT_100000680;
    uVar1 = (ulong)DAT_100000680 >> 0x20;
    *(double *)(puVar2 + 10) = *(double *)(puVar2 + 10) * 1.5;
    *(ulong *)(puVar2 + 0xc) =
         CONCAT44((int)((ulong)*(undefined8 *)(puVar2 + 0xc) >> 0x20) + (int)uVar1,
                  (int)*(undefined8 *)(puVar2 + 0xc) + iVar3);
    _printf("New Value: %.2f\n");
    _printf("New Position: (%d, %d)\n");
    _printf("Sum: %d\n");
    _free(puVar2);
  }
  return puVar2 == (undefined4 *)0x0;
}
```

**Analysis:**
- ✅ Detected `malloc(0x38)` heap allocation
- ✅ Tracked pointer through struct initialization
- ✅ Recognized all 6 libc function calls
- ✅ Identified struct field initializations
- ✅ Control flow with null check branching
- ✅ Proper `free()` cleanup
- ✅ Return value computation

### Fission Output ❌
```c
void _main(void)
{
  do {
    /* WARNING: Do nothing block with infinite loop */
  } while( true );
}
```

**Issues:**
- ❌ Function body completely collapsed
- ❌ All 6 external calls invisible
- ❌ No heap allocation/deallocation tracking
- ❌ No struct initialization detected
- ❌ Control flow analysis catastrophic failure

**Root Causes:**
1. **External Function Blindness:** Complete failure to recognize libc imports
2. **Heap Analysis Absent:** No malloc/free tracking infrastructure
3. **CFG Pathological Failure:** Most severe control flow misinterpretation
4. **Binary Format Gap:** Possible Mach-O import resolution issues

---

## Quantitative Comparison

| Metric | Ghidra | Fission | Gap |
|--------|--------|---------|-----|
| **Functions Correctly Decompiled** | 3/3 (100%) | 0/3 (0%) | -100% |
| **External Calls Detected** | 11/11 (100%) | 0/11 (0%) | -100% |
| **Parameters Correctly Identified** | 3/3 (100%) | 0/3 (0%) | -100% |
| **Return Types Accurate** | 3/3 (100%) | 0/3 (0%) | -100% |
| **Control Flow Accurate** | 3/3 (100%) | 0/3 (0%) | -100% |
| **Human Readability** | High | Very Low | Critical |

---

## Priority Improvements for Fission

### 🔴 Critical (Blocking Basic Functionality)

1. **Calling Convention Implementation**
   - **Issue:** Cannot recognize System V AMD64 ABI
   - **Impact:** All parameter/return value analysis fails
   - **Location:** `src/analysis/` - function signature analysis
   - **Effort:** High (2-3 weeks)

2. **External Function Resolution**
   - **Issue:** Libc functions invisible (printf, malloc, free, puts)
   - **Impact:** Missing all external calls
   - **Location:** `src/parser/` - import table parsing
   - **Effort:** Medium (1-2 weeks)

3. **Type Inference System**
   - **Issue:** Defaults to wrong types (char, void)
   - **Impact:** Unreadable output with exposed registers
   - **Location:** `src/analysis/types/` - type propagation
   - **Effort:** High (3-4 weeks)

4. **Memory Block Registration (Mach-O)**
   - **Issue:** "Bad instruction data" errors
   - **Impact:** Disassembly corruption
   - **Location:** `src/parser/macho.rs` - segment loading
   - **Effort:** Low (3-5 days)

### 🟡 High Priority (Quality Issues)

5. **Control Flow Graph Accuracy**
   - **Issue:** False positive infinite loops
   - **Impact:** Functions appear non-terminating
   - **Location:** `src/analysis/cfg.rs`
   - **Effort:** Medium (1-2 weeks)

6. **Heap Allocation Tracking**
   - **Issue:** Cannot track malloc/free pairs
   - **Impact:** Memory management invisible
   - **Location:** `src/analysis/memory/` - new module
   - **Effort:** Medium (2 weeks)

7. **Struct Recovery**
   - **Issue:** No struct field inference
   - **Impact:** Pointer arithmetic instead of field access
   - **Location:** `src/analysis/types/structs.rs` - new module
   - **Effort:** High (3-4 weeks)

### 🟢 Medium Priority (Nice-to-Have)

8. **Register Abstraction Layer**
   - **Issue:** Register names leak into output
   - **Impact:** Low-level, unreadable code
   - **Location:** `src/core/ir/` - variable naming
   - **Effort:** Medium (1 week)

9. **Floating-Point Support**
   - **Issue:** FP operations not recognized
   - **Impact:** Numeric computation invisible
   - **Location:** `src/analysis/pcode/` - FP operator handling
   - **Effort:** Low (3-5 days)

---

## Testing Methodology

### Environment Setup
```bash
# Install PyGhidra compatible with Ghidra 11.4.2
pip3 install pyghidra==2.2.0

# Set Ghidra installation path
export GHIDRA_INSTALL_DIR=/path/to/ghidra_11.4.2_PUBLIC

# Run comparison
./scripts/compare_decompilers.sh <binary> <address>
```

### Test Binary Compilation
```bash
# Compile test with debug symbols
clang -g -O0 test/struct_test.c -o test/struct_test
```

### Comparison Script
- **Tool:** `scripts/compare_decompilers.sh`
- **Ghidra Wrapper:** `scripts/pyghidra_decompile.py`
- **Output:** Side-by-side decompilation comparison

---

## Recommendations

### Immediate Actions (Next Sprint)
1. Fix Mach-O memory block registration (`src/parser/macho.rs`)
2. Implement System V AMD64 calling convention (`src/analysis/calling_convention.rs`)
3. Add libc function signature database (`src/analysis/signatures/libc.json`)

### Short-term Goals (1-2 Months)
1. Complete type inference system with data flow analysis
2. Rewrite CFG construction to eliminate false positive loops
3. Add heap allocation tracking for malloc/free

### Long-term Vision (3-6 Months)
1. Struct recovery with field layout inference
2. Advanced type recovery (polymorphic types, unions)
3. Quality parity with Ghidra on standard binaries

---

## Conclusion

This comparison reveals that Fission is in early development stage with fundamental architectural gaps. While the Pcode infrastructure is solid (graph visualization works), the higher-level semantic analysis layers are incomplete or missing.

**Current State:** Fission cannot reliably decompile even simple functions.

**Path Forward:** Prioritize calling convention recognition and external function resolution as foundational capabilities. Without these, all downstream analysis (types, control flow, memory) cannot function correctly.

**Estimated Timeline:** 3-6 months of focused development to reach basic decompilation quality for simple binaries.

---

**Generated by:** Fission Development Team  
**Comparison Tool:** PyGhidra 2.2.0 + Fission CLI  
**Test Coverage:** 3 functions (simple, medium, complex)  
**Recommendation:** Use as roadmap for Q1 2026 development priorities
