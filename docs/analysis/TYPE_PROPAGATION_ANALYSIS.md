# Type Propagation Analysis: Ghidra vs Fission

This document explains one specific historical gap: why Fission used to render certain floating-point constants as unreadable hexadecimal literals while Ghidra produced cleaner symbolic or typed output.

## Problem Summary

Representative example:

```c
// Ghidra (target)
local_18 = create_item(0x3e9, "TestItem", DAT_1400040c8);

// Fission (older behavior)
pvStack_18 = create_item(0x3e9, "TestItem", 0x4048feb851eb851f);
```

The hexadecimal value is technically correct, but much less readable:

- `0x4048feb851eb851f` = `49.99` as a `double`
- `0x4059000000000000` = `100.0` as a `double`

## How Ghidra Makes This Work

### High-Level Flow

Ghidra's type propagation path looks roughly like this:

```
ActionInferTypes
├─ buildLocaltypes()
├─ propagateOneType()
│  └─ propagateTypeEdge()
│     └─ TypeOp::propagateType()
└─ writeBack()
```

For `LOAD`, the critical point is that propagation only succeeds cleanly if the address-like value is already understood as a **pointer to a typed object**.

### Why That Matters

For a memory load like:

```asm
movsd xmm0, [0x1400040C8]
```

Ghidra effectively treats the constant address as:

- a global data symbol
- with a known type, such as `double`
- then wrapped as `ptr<double>`

That makes `LOAD` type propagation succeed and allows the output to become something like `DAT_1400040c8` instead of a raw hex literal.

## Older Fission Behavior

Historically, Fission could run a Ghidra-style propagation step but still fail to get the readable result because the data-side setup was incomplete.

### What Happened

For the same memory load:

```
%xmm0 = LOAD(ram, 0x1400040C8)
```

the older behavior was:

1. the constant `0x1400040C8` remained just a constant varnode
2. no typed global data symbol was available
3. the value was not treated as `TYPE_PTR`
4. `LOAD` propagation had no typed pointer to propagate from
5. output fell back to a raw literal

## Root Cause

### 1. Missing Data-Section Symbolization

Ghidra could:

- scan `.rdata` / `.data`
- create `DAT_<addr>` symbols
- attach an inferred type
- register them in the global scope

Older Fission behavior lacked enough of that pipeline, so constants stayed untyped.

### 2. No Pointer-Like Typing At The Start Of The Chain

The propagation chain needed to look like this:

```
[constant 0x1400040C8] --LOAD--> [loaded value]
      ↓ type
  ptr<double>                    double
```

Instead, it looked more like:

```
[constant 0x1400040C8] --LOAD--> [loaded value]
      ↓ type
  undefined8 / qword             propagation fails
```

## Proposed Solutions Considered

### Option A: Automatic Data-Section Symbol Generation

This was the most direct and Ghidra-like fix.

Planned steps:

1. scan `.rdata` / `.data`
2. detect likely typed data (for example `float` / `double`)
3. create `DAT_<addr>` symbols
4. register them in the global scope before propagation

This would let the existing type-propagation chain work with better inputs.

### Option B: Inject Constant Pointer Hints

A lighter-weight approach:

- find `LOAD` operations with constant addresses
- detect whether the pointed-to bytes look like `float` / `double`
- inject a temporary pointer type hint directly

This was easier to prototype but less systematic than true data-symbol registration.

### Option C: Output-Side Postprocessing

This was lower impact and only partially successful.

It could improve some compare expressions or formatting cases, but it was weaker for function arguments because by then the core typing decisions had already been made.

## Comparison

| Area | Ghidra | Older Fission | Gap |
|------|--------|---------------|-----|
| Data-section scan | ✅ automatic | ❌ missing / incomplete | no data symbol source |
| `DAT_` symbol creation | ✅ | ❌ | no stable symbolic name |
| Type inference on data | ✅ | ⚠️ partial | weak float detection |
| Pointer creation | ✅ automatic | ❌ manual or missing | load propagation breaks |
| `LOAD` propagation | ✅ works | ⚠️ conditional | needs a typed pointer input |
| Output surface | symbolic / typed | raw hex literal | readability loss |

## Recommended Implementation Order

### Phase 1: Data Scan and Symbol Generation

1. add a data-section scanner
2. detect `float` / `double` patterns
3. register typed global data symbols
4. verify the output surface

### Phase 2: Integrate With The Load Path

1. run the scanner during binary loading
2. include `.rdata` and `.data`
3. log the created symbol count

### Phase 3: Re-Run Comparison

1. rerun the benchmark
2. verify `DAT_`-style output or typed constants
3. measure similarity improvement

## Expected Effect

### Before

```c
pvStack_18 = create_item(0x3e9, "TestItem", 0x4048feb851eb851f);
uStack_1c = calculate_discount(0xf, 0x4059000000000000);
```

### Better

```c
pvStack_18 = create_item(0x3e9, "TestItem", DAT_1400040c8);
uStack_1c = calculate_discount(0xf, DAT_1400040d0);
```

### Even Better

```c
pvStack_18 = create_item(0x3e9, "TestItem", 49.99);
uStack_1c = calculate_discount(0xf, 100.0);
```

## Expected Similarity Gain

At the time, the expectation was:

- simple functions: little to no change
- complex functions such as `main`: potentially a **large** readability and similarity gain

The important point was not just benchmark score. It was restoring the missing link between:

- data-section discovery
- pointer typing
- and `LOAD`-based type propagation

## Conclusion

The core propagation logic was not the main problem by itself. The real issue was that the input side of the propagation chain was missing a typed data model.

In short:

- Fission already had much of the propagation machinery
- but it lacked enough data-section handling to feed that machinery correctly
- once that input side was repaired, the propagation path could produce much more readable results
