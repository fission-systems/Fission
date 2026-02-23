# ESSENTIAL IMPROVEMENTS (2026-01-11)

> ⚠️ **Status:** 이 문서는 2026-01-11 시점의 아이디어 메모입니다.  
> 최신 우선순위/계획은 `docs/ROADMAP.md`를 기준으로 보고,  
> 여기 내용은 세부 배경/레퍼런스로만 사용하는 것을 권장합니다.

## 1. Function Identification (FID) Integrity Check

- **Current Issue**: The current FID implementation relies purely on byte-pattern matching (function body hashing). This causes **False Positives** (collisions), especially for small functions like constructors or wrappers.
  - *Example*: A simple constructor was misidentified as `D3DXSHPRTCompSplitMeshSC` due to identical byte patterns.
- **Solution (Ghidra's Approach)**:
  - Ghidra's `FidProgramSeeker.java` implements **Call Graph Relation Matching**.
  - It utilizes `Child` (Callee) and `Parent` (Caller) relations.
  - Functions marked with `forceRelation` in the FID database are ONLY matched if their neighbors (callees/callers) also match the expected signature pattern.
- **Action Plan**:
  1. Implement a **Call Graph Builder** in Fission Analysis.
  2. Update `fission-signatures` to support and check for "Relation Constraints" (match children/parents).
  3. Discard matches that fail the relation check.

## 2. Emulation-Based Deobfuscation

- **Current State**: We rely on static analysis (Prologue & CALL target scanning).
- **Issue**: Obfuscated code often uses indirect jumps/calls or modifies return addresses, which static analysis misses.
- **Improvement**: Implement **Micro-Emulation**.
  - Use P-Code or a lightweight CPU emulator to execute code paths symbolically.
  - Resolve indirect branch targets by tracking register values during emulation.
  - This is similar to Ghidra's `EmulatorHelper` usage in scripting.

## 3. FID Database Expansion

- **Task**: Parse and import Ghidra's `.fidbf` files found in `utils/signatures/fid`.
- **Benefit**: Access to a massive database of standard library signatures (VS2012-2019, GCC, etc.) without needing to manually convert them to `msvc_sigs.rs`.

## 4. UI Enhancements

- **Task**: Visualize the "Confidence Score" of an FID match in the UI.
- **Task**: Allow users to manually "Reject" an FID match and revert to the original name (or `sub_XXXX`).
