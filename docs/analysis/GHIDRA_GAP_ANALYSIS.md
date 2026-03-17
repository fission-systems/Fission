# Ghidra vs Fission Gap Analysis (2026-03-04)

This document was written to quantify the practical feature gap between Ghidra 11.4.2 and Fission from a codebase perspective.

Important context: Fission directly embeds the Ghidra C++ decompiler core as a shared library. That means many core Ghidra analyses already execute inside `decomp_function()`. The real question is not “what does Ghidra have that Fission does not?” in the abstract, but rather:

- what already comes from the embedded Ghidra core
- what Fission adds on top
- what still remains unimplemented in Fission-owned layers

## Pipeline View

### Ghidra Core (already running inside Fission)

The embedded `libdecomp` path already covers large parts of the classic decompiler pipeline:

- algebraic transformation rules
- core action passes
- CFG transformation passes
- SSA / heritage construction
- type propagation
- jump-table analysis
- C printing

### Fission Native Layer

Fission adds a large native-side layer on top of that core, including:

- `TypePropagator`
- `StructureAnalyzer`
- calling-convention detection
- FID/signature matching
- no-return detection
- vtable analysis
- emulation analysis
- text postprocessing
- CFG structurization and switch reconstruction

### Fission Rust Layer

On the Rust side, the project adds:

- FFI infrastructure
- the Rust-owned P-code model and optimizer
- binary loading
- analysis orchestration
- CLI and Tauri frontend layers

## What Ghidra Already Covers Inside Fission

These were already present through the embedded Ghidra core:

| Capability | Status in Fission via embedded Ghidra |
|-----------|----------------------------------------|
| type propagation | ✅ already executing |
| SSA / heritage analysis | ✅ already executing |
| algebraic rule/action pipeline | ✅ already executing |
| jump-table analysis | ✅ already executing |
| dead-code removal | ✅ already executing |
| parameter reconstruction | ✅ already executing |
| C printing | ✅ already executing |
| loop recovery | ✅ already executing |
| constant propagation | ✅ already executing |
| common subexpression elimination | ✅ already executing |

One practical consequence of this analysis was that some earlier “missing feature” estimates understated the real functional baseline, because they counted only what Fission had added itself, not what the embedded Ghidra core was already providing.

## OptionDatabase Features Not Yet Fully Used

Some Ghidra option-database features existed but were not fully surfaced or configured in Fission-owned initialization code.

Examples:

| Option | Effect | Priority |
|--------|--------|----------|
| `OptionNullPrinting` | print pointer zero as `NULL` | high |
| `OptionInPlaceOps` | prefer `x += 1`-style output | high |
| `OptionHideExtensions` | hide some unnecessary extension operations | high |
| `OptionNoCastPrinting` | omit safe casts | medium |
| `OptionCommentHeader` | emit function header comments | medium |
| `OptionCommentInstruction` | emit address-based inline comments | low |
| `OptionBraceFormat` | brace-style control | low |
| `OptionToggleRule` | per-rule enable/disable control | low |

These were relatively low-risk improvements compared with deeper architectural gaps.

## Fission-Owned Gaps That Still Mattered

### 1. Debug Symbol Import

Missing or incomplete:

- DWARF parsing
- PDB / CodeView parsing

Potential impact:

- stronger recovery of types, local names, and inline-function boundaries on debug builds

### 2. Union Recovery

At the time, struct analysis mostly mapped offsets to fields but did not robustly detect overlapping accesses that should become unions.

### 3. Enum Inference

Missing at the time:

- case-set analysis from `switch`
- promotion of clustered integer values into enum-like types

### 4. Bitfield Recovery

Patterns such as:

```c
(x >> 3) & 0x1F
```

were not yet being promoted into bitfield-style structure surfaces.

### 5. More Complete C++ Exception Recovery

SEH had some support, but full MSVC and GCC exception-structure recovery was still incomplete.

### 6. Call-Graph-Based Type Propagation

Analysis was still too function-local. Richer caller ↔ callee feedback loops were still missing.

### 7. Architecture-Specific Extensions

The strongest support was on x86/x64. Other architectures remained comparatively shallow.

## What Was Miscounted In Earlier Analyses

Earlier “gap” analyses sometimes treated the following as not implemented, even though the embedded Ghidra core was already doing them:

| Item | Actual state |
|------|--------------|
| type propagation | ✅ already running through `ActionInferTypes` |
| P-code optimization | ✅ already covered by the Ghidra rule/action pipeline |
| CFG loop recovery | ✅ already present in block actions |
| parameter type recovery | ✅ already present in Ghidra core actions |

This was an important correction: the meaningful gap was smaller and more specific than a naive feature checklist suggested.

## Real Fission Differentiators

Even where Ghidra core overlap existed, Fission still had real differentiation in:

| Area | Fission-specific advantage |
|------|----------------------------|
| runtime form factor | native binary embedding instead of JVM-only workflows |
| UI | Tauri + React desktop direction |
| automation | CLI + Rust-driven workflows |
| customization | direct C++ / Rust ownership without Java plugin dependency |
| FFI model | direct Rust ↔ Ghidra C++ integration |
| postprocessing | dedicated Fission-owned postprocess and structuring layers |
| signatures/FID | custom parsers and matching infrastructure |

## Priority Order From This Analysis

The most pragmatic next steps identified at the time were:

| Priority | Item | Expected impact | Difficulty |
|----------|------|-----------------|------------|
| 1 | enable high-value Ghidra output options | immediate output-quality gain | low |
| 2 | enum inference | better readability | medium |
| 3 | union recovery | better structure accuracy | medium |
| 4 | DWARF parsing | large gains on debug builds | high |
| 5 | call-graph-based type propagation | broader analysis quality gains | high |

## Conclusion

The core conclusion of this note was:

- Fission already inherited a large amount of decompiler capability from the embedded Ghidra core
- the most important remaining gaps were **not** “rebuild all of Ghidra,” but rather
  - better use of Ghidra's configurable output features
  - deeper Fission-owned semantic recovery
  - better cross-function and debug-info integration

That made the roadmap far more actionable: focus on the layers Fission actually owns, instead of misclassifying already-embedded Ghidra features as missing.
