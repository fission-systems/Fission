# Fission Project Instructions

## Current Priorities & Direction
1. **1st Priority: x86 / x86-64 Decompilation Accuracy & Pseudocode Quality**
   - Focus on accurate control flow recovery (if/else/switch/loop/break/continue).
   - Improve pointer, array, struct, and field access representation.
   - Improve calling convention, parameter, and local variable recovery.
   - Remove unnecessary temporary variables.
   - Restore pointer arithmetic to C-friendly array/index/field expressions.
   - Clean up return values, accumulators, and loop induction variables.
   - Goal: High-quality, human-readable pseudocode compared to Ghidra.

2. **2nd Priority: Type/Data Abstraction**
   - Resolve structs, pointers, arrays, field accesses, and calling conventions at the NIR/HIR semantic layer, not via simple output substitution.

3. **3rd Priority: Large/Hard Function Structuring**
   - Scale readable pseudocode to complex x86/x86-64 functions using CFG, dominance, post-dominance, SCC, dataflow, and fixed-point based analysis.

4. **4th Priority: SLEIGH Lift Accuracy & Regression Prevention**
   - **NO MANUAL MAPPING** in the SLEIGH engine.
   - `.sla` ConstructTpl execution is the success source. Do not increase legacy token cursor, BoundOperand fallback, or compatibility classifier debt.
   - Validate row-level raw p-code parity first, then check benchmarks after canonical gate.

5. **5th Priority: FID/Name Recovery**
   - Incrementally improve over Ghidra's ecosystem (packed `.fidb`, exact hash inputs, program seeker coverage).

6. **6th Priority: Architecture/File-Format Breadth**
   - Expand to ARM, MIPS, PPC, ELF/Mach-O only after x86/x86-64 quality is sufficiently high.

## Core Principles
1. Elevate sample binary quality before real-world binaries.
2. Approach as a Ghidra 1:1 clean-room migration.
3. Design/implement with **Zero-Dependency** by default.
4. Prefer CFG, dominance, dataflow, fixed-point computation, and constraint-based inference over simple pattern matching or temporary heuristics.
5. Avoid ISA/compiler overfitting, but keep the current optimization target as x86/x86-64.
6. Consider Rust libraries only for long-term bottlenecks that are hard to resolve internally. **C++ bindings are strictly forbidden.**
7. Prioritize long-term maintainability and generalizable structure improvements over short-term output patching.
8. Proposals and implementations must include architectural perspective, testability, and observability valid beyond 2-3 cycles.
9. **No approximations or estimates.** Base improvements on actual measurements and verifiable analysis.
10. Ultimate success criteria: measurable improvement in semantic accuracy and pseudocode quality in `benchmark/source_semantic_benchmark`.

## Path/Resource Guidelines
- **`/Users/sjkim1127/Fission/utils`**: Contains Fission-internal resources, types, signatures, and benchmark helper data.
  - Prioritize using these to avoid hardcoding or duplication.
  - Usage must connect to a maintainable structure, not act as a semantic layer bypass.
  - Resolve paths via PathConfig, resource roots, or existing load paths; **no absolute path hardcoding.**
- **`/Users/sjkim1127/Fission/vendor`**: External reference code.
  - **`/Users/sjkim1127/Fission/vendor/ghidra/ghidra-Ghidra_12.0.4_build`**: Key reference for Ghidra 1:1 clean-room migration.
  - Use strictly for understanding design, algorithms, semantics, and edge cases.
  - **DO NOT** add runtime/build dependencies on vendor code. **C++ bindings are forbidden.**

## Operational Rules
- **NO MANUAL MAPPING** in the SLEIGH engine.
- Commit and push to GitHub frequently during development.
- Only commit/push to the **Main branch**.
- After changes, validate via targeted tests, crate check/test, and the source semantic benchmark.
