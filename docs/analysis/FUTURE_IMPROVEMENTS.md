# Additional Decompiler Improvement Directions

This note summarizes the next areas that were identified after the early high-similarity cleanup work. At the time of writing, the baseline looked strong on simple cases but still left room for improvement in practical readability, complex control flow, and deeper type recovery.

## Snapshot At The Time

- average similarity: **97.86%** against the selected Ghidra baseline
- exact-match functions: **3/4**
- strongest improvements already landed:
  - style standardization
  - type propagation
  - string inlining

## Remaining Improvement Areas

### Priority A: Fine-Tuning Toward 98-99%

#### 1. Better Pointer Type Inference

**Current issue**

```c
// Ghidra
uint *local_18;

// Fission
void *local_18;
```

**Root cause**

- the return type of `create_item()` was inferred as `void*`
- struct-pointer information was not being propagated far enough

**Suggested direction**

- detect allocator + initialization patterns in `TypePropagator`
- backtrack field-access patterns to infer struct pointer types
- compare missing behavior against Ghidra's `ActionInferTypes`

**Expected effect:** raise a representative `main` similarity from about `91.43%` to roughly `94%`

#### 2. More Explicit Cast Insertion

**Current issue**

```c
// Ghidra
sum_array((longlong)&local_38,5);

// Fission
sum_array(&local_38,5);
```

**Root cause**

- Ghidra's `ActionSetCasts` inserted casts more aggressively
- Fission under-emitted casts when the prototype and argument type differed

**Suggested direction**

- review `ActionSetCasts`
- strengthen argument-type checking at call sites
- explicitly emit pointer→integer casts where needed

**Expected effect:** another small readability/similarity gain on complex functions

#### 3. Optional Header-Comment Suppression

**Current issue**

```c
// ============================================
// Function: main @ 0x140001680
// ============================================
```

**Suggested direction**

- make function-header comment emission optional
- gate it behind a compatibility / formatting flag

**Expected effect:** better benchmark compatibility and cleaner output

### Priority B: Support for More Complex Code

#### 4. Complex Control Flow

Patterns that needed broader verification:

- nested loops
- large `switch` statements
- `do-while`
- mixed `goto` / `break` / `continue`
- recursion

The main goal here was not to replace Ghidra's core CFG work, but to validate and strengthen the post-structuring and readability pipeline on harder real-world shapes.

#### 5. C++ Support

Main gaps:

- vtable identification
- constructor / destructor pattern handling
- template-instance naming
- namespace recovery

Suggested improvement areas:

- stronger C++ structure analysis
- vtable pointer detection and virtual-call interpretation
- RTTI parsing
- better demangling

#### 6. Better Struct Recovery Accuracy

**Current state**

- basic struct-field detection worked
- nested structs were partially supported
- accuracy was still inconsistent

**Target**

Move from coarse field ranges like:

```c
struct unknown_struct {
    int field_0;
    undefined field_4[28];
};
```

toward more specific layouts such as:

```c
struct Item {
    int id;
    char name[32];
    double value;
};
```

Suggested directions:

- collect more field-access patterns
- use stronger size hints
- analyze initialization patterns such as `strcpy` / `memcpy`

### Priority C: Longer-Term Research

#### 7. Reverse High-Level Optimizations

Goal: reconstruct simpler source-like forms from optimized instruction patterns.

Example:

```c
// current decompilation
return (x + x * 2) << 2;

// desired high-level simplification
return x * 12;
```

#### 8. Semantic Variable Naming

Move beyond local naming patterns toward more context-aware variable naming:

```c
// current
int local_c;
int local_10;

// aspirational
int result;
int count;
```

Potential inputs:

- variable-use patterns
- call context
- constant-value patterns
- optional ML/LLM assistance

#### 9. Broader Architecture Coverage

At the time:

- x86 / x86-64 were the strongest paths
- ARM / ARM64 existed only partially
- MIPS, PowerPC, and RISC-V were still future work

### Priority D: Performance

#### 10. Faster Decompilation

At the time of this note:

- Ghidra average: `2.744s`
- Fission average: `3.457s` (about `26%` slower)

Suspected bottlenecks:

1. repeated data-section scans
2. repeated type-propagation reruns
3. FFI overhead

Suggested directions:

- keep data-section scans to once per binary
- optimize type-propagation passes
- strengthen caching
- reduce Rust↔C++ overhead

## Recommended Priority Order

### Short-Term (1-2 Weeks)

1. optional header-comment suppression
2. explicit cast insertion improvements

### Mid-Term (1-2 Months)

3. pointer type inference improvements
4. broader complex-control-flow validation
5. decompilation speed optimization

### Longer-Term (3-6 Months)

6. stronger struct recovery
7. better C++ support

### Research Topics

8. optimization reverse-tracking
9. semantic variable naming

## Practicality vs Benchmarking

### If the goal is benchmark maximization

Focus on the fine-tuning items first:

- pointer inference
- explicit casts
- formatting compatibility

### If the goal is practical tooling quality

Focus on:

- complex real-world control flow
- struct recovery
- C++ support

## Conclusion

At the time of this note, Fission already looked strong:

- simple-function quality was effectively solved
- Ghidra-style surface compatibility was high
- remaining work was concentrated in harder real-world cases and deeper semantic recovery

The main tradeoff was no longer “good vs bad,” but “excellent benchmark polish vs broader practical capability.”
