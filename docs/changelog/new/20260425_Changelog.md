# Changelog: Raw P-code HandleTpl and BUILD Export Parity

**Date:** 2026-04-25  
**Scope:** `fission-sleigh` runtime, SLA template handle remapping, raw P-code parity reporting

## Summary

Advanced the x86-64 raw P-code path from compatibility fallback behavior toward Ghidra-shaped `ConstructTpl` execution. The main fix resolves the remaining `mov [reg], imm32` (`c7 00 imm32`) row by materializing fixed handles for operands and letting the `SpecDerived` template emit the exact Ghidra `COPY, STORE` sequence without compatibility P-code.

This wave also fixed two exposed follow-on gaps:

- `lea` now reuses the effective-address varnode produced by `BUILD`, eliminating the stale handle-selector varnode mismatch.
- `Jcc` now resolves the negative exported handle produced by `BUILD cc`, so `cmp+jcc` emits the Ghidra-equivalent `INT_NOTEQUAL, BOOL_OR, CBRANCH` sequence without native semantic fallback.

Approximate P-code remains disallowed. The runtime still fails closed for unresolved templates, unsupported dynamic forms, and padding/no-instruction rows.

## Implementation Notes

### Runtime Fixed Handles

- Added `RuntimeFixedHandle` to mirror the Ghidra `FixedHandle` shape used by template resolution.
- Preserved `space`, `size`, pointer-offset fields, temp-space fields, and a `fixable` flag for checked emission.
- Kept `BoundOperand` as decode/display input, but moved raw P-code template resolution to fixed-handle data.

### `mov [reg], imm32`

- Added simple dynamic-memory target resolution for fixable memory handles.
- `COPY` into a dynamic memory handle now stages constants through the fixed temp varnode before emitting `STORE`.
- Target row `memory_store_imm` now matches Ghidra exactly:
  - `COPY const(imm32) -> unique(0xd400, 4)`
  - `STORE const(ram-space-id), register(rax, 8), unique(0xd400, 4)`

### BUILD and Exported Handles

- `BUILD` now records effective-address varnodes for parent template reads.
- Parent templates that reference handle selector varnodes can reuse the varnode emitted by the subconstructor instead of fabricating a placeholder.
- Negative exported handles are resolved for `Jcc` condition-code subconstructors, removing the previous `handle -1 is missing or unresolved` failure.

### Compiler Remapping

- Improved SLA handle remapping for constructors with leading hidden `BUILD` handles.
- Added detection for unresolvable handle references before runtime so unsupported templates remain typed and fail-closed.
- Regenerated the affected x86 parsed inventories and manifest deterministically.

### Benchmark Reporting

- Added a padding classification path for rows where Ghidra also cannot decode and Fission reports an empty/no-op spec-derived template.
- This keeps padding/no-instruction rows out of semantic mismatch buckets while preserving the original decode-error evidence.

## Validation

Required checks run:

```text
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
cargo check -p fission-sleigh
cargo test -p fission-sleigh generated_runtime_decodes_startup_store_mov_mem32_imm32_without_compatibility_lift -- --test-threads=1
cargo build --release -p fission-cli
cargo test -p fission-sleigh -- --test-threads=1
```

Result:

```text
fission-sleigh: 55 passed, 0 failed
```

Targeted raw P-code checks:

```text
memory_store_imm: full_match = 1
lea:              full_match = 2
cmp_and_jcc:      full_match = 2
```

Full canonical report:

```text
Report: benchmark/artifacts/raw_p_code_benchmark/handle_tpl_store_fixed/aggregate_raw_pcode_parity_report.json

bucket_totals:
  full_match: 36
  missing_fission_instruction: 6
  unsupported_template: 4
  ghidra_decode_error: 2
  both_decode_error_or_padding: 2

invariants:
  compat_emitter_used: 0
  fake_placeholder_op: 0
  invalid_pcode_shape: 0

template_source_totals:
  SpecDerived: 36
```

Performance summary from the same report:

```text
Fission instructions/sec: 2.0408834352675123
Ghidra instructions/sec:  1.064634893718624
Fission/Ghidra instruction throughput ratio: 1.9169796587626258
Fission/Ghidra P-code throughput ratio:      1.995867710563557
```

## Remaining Owners

- `missing_fission_instruction = 6`: decoder coverage, not P-code semantic parity.
- `unsupported_template = 4`: typed unsupported templates; keep fail-closed until the real template/handle form is implemented.
- `ghidra_decode_error = 2` and `both_decode_error_or_padding = 2`: benchmark padding/no-instruction cases, not runtime semantic failures.

No semantic mismatch buckets remain in the latest canonical raw P-code report.
