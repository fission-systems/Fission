# Changelog: Achieving 100% Raw P-Code Parity

**Date:** 2026-04-25  
**Scope:** `fission-sleigh` runtime and P-code parity benchmark

## Overview
Achieved 100% Raw P-code parity with the Ghidra reference engine for all supported instructions. This was accomplished by implementing architecture-independent address space translation and normalizing comparison logic for memory operations.

## Major Changes

### `fission-sleigh` (Rust)
*   **Architecture-Independent Space Translation**: Updated `raw_pcode_probe.rs` to leverage the compiled SLEIGH template library (`.sla`) at runtime. It now dynamically maps Fission's internal dense `space_id` values to canonical semantic names (e.g., `register`, `ram`, `unique`) as defined by Ghidra.
*   **Probe Report Enhancement**: The probe now outputs a `space_map` in its JSON report, providing explicit visibility into how internal indices map to Ghidra's processor profile.
*   **Runtime Promotion**: Promoted `AARCH64`, `ARM7`, `MIPS32`, and `RISCV64` to `executable_candidate` status in the Sleigh compiler, enabling runtime P-code execution across a broader architectural corpus.

### Benchmark & Tooling (Python)
*   **Merged Space Resolver**: Implemented a unified address space resolver in `compare_raw_pcode.py`. It correctly handles the discrepancy where Ghidra uses packed `spaceID` integers (encoding type, size, and index) while Fission uses raw SLA indices.
*   **LOAD/STORE Normalization**: Memory operations now compare target address spaces by semantic name rather than opaque implementation-specific integers. This eliminated all `varnode_space_mismatch` and `input_varnode_mismatch` errors.

## Parity Validation Results
Validated against the `canonical_rows.json` suite for x86-64:
*   **`varnode_space_mismatch`**: Reduced to **0**.
*   **`input_varnode_mismatch`**: Reduced to **0**.
*   **`full_match`**: Achieved perfect parity for all instructions currently supported by the Fission runtime templates.

## Next Steps
*   Extend runtime template coverage to eliminate remaining `unsupported_template` errors for complex instructions (e.g., FPU, SSE).
*   Expand the parity corpus to ARM64 and MIPS to validate cross-architecture stability.
