# Changelog: Stabilizing Fission-Sleigh Runtime and SLA Handle Remapping

**Date:** 2026-04-25  
**Scope:** `fission-sleigh` runtime and P-code parity benchmark

## Overview
Stabilized the `fission-sleigh` instruction decoding runtime by addressing out-of-bounds handle panics caused by implicit/invisible SLA operands, and unblocking execution for subconstructor-dependent instructions (e.g. `Jcc`) via a fallback to native templates. Achieved significant improvement in P-code parity benchmark matching.

## Major Changes

### `fission-sleigh` (Rust)
*   **Dynamic SLA Handle Remapping**: Replaced static operand index swapping with dynamic remapping based on `ELEM_OPPRINT` parsed from Ghidra's `.sla` files. This correctly aligns Ghidra's internal template operand indices with Fission's display-based operand ordering.
*   **Native Fission Template Fallback**: Added `CompiledTemplateSource::NativeFission` to bypass Ghidra SLA templates when they contain unresolvable subconstructor handles (like the `cc` subtable in `J^cc`). This safely preserves Fission's native semantic templates (`ConditionalJump`, `SetCc`) while avoiding fail-closed validation checks.
*   **Condition Code P-code Evaluation**: Fully implemented x86 condition code flag evaluations (`emit_condition_predicate`) directly inside `CompiledTableEmitter`. This maps the abstract `ConditionPredicate` varnode into realistic Ghidra-equivalent P-code sequences (`INT_NOTEQUAL`, `BOOL_OR`, etc.) for all 16 condition codes (CF, ZF, SF, OF, PF).
*   **Branch Target Resolution**: Added support for resolving `Handle` varnode templates into memory offsets (RAM space) for branch instructions matching `BoundOperand::Relative`.
*   **Invisible Operand Defense**: Sub-constructors or instructions with `opprint=[]` (e.g. `MOV m32, imm32`) often reference internal SLA handles (e.g. handle 2) that Fission's frontend does not track. We added defensive logic to skip remapping on empty opprints and gracefully downgrade unresolvable handle references to `UnsupportedPcodeTemplate` rather than panicking.
*   **Test Suite Stabilization**: Resolved critical crashes in x86-64 test cases (`mov_mem32_imm32`, `jcc_rel8`, `zero_extends_reg32`). Achieved a **100% pass rate** for the `fission-sleigh` crate (54/54 tests).

### Benchmark & Parity (Python)
*   **Parity Benchmark Execution**: Executed `canonical_rows.json` through the parity pipeline. 
*   **Jcc Parity Gap Resolved**: The implementation of `NativeFission` templates and condition flag evaluations resolved the `JLE` disparity. 
*   **Benchmark Improvement**: The total number of `unsupported_template` errors dropped from 5 to 2, and the `full_match` score successfully increased from 29 to 30.

## Next Steps
*   Investigate and resolve the remaining `input_varnode_mismatch` and `pcode_op_count_mismatch` errors in `raw_pcode_parity_report.json`.
*   Expand Fission's handle tracking to capture all SLA sub-table operands (even invisible ones) to eliminate the remaining `UnsupportedPcodeTemplate` fallbacks for complex templates.
