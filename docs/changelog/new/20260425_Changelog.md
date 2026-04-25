# Changelog: Raw P-code Zero-op ConstructTpl Parity

**Date:** 2026-04-25  
**Scope:** `fission-sleigh` runtime, raw P-code comparator, canonical parity report

## Summary

Resolved the remaining raw P-code decoder-continuation bucket caused by `NOP` constructors. Ghidra reports these instructions as valid decode results with an empty P-code list, so Fission now treats `SpecDerived` empty `ConstructTpl` templates as successful zero-op emission instead of `UnsupportedPcodeTemplate`.

This keeps the no-approximation rule intact:

- `SpecDerived` empty templates can return `pcode=[]`.
- `CompatibilityLowered` / `NativeFission` empty templates still cannot masquerade as semantic success.
- No compatibility emitter, fake placeholder op, or handwritten NOP P-code is used.

## Implementation Notes

### Zero-op `SpecDerived` Templates

- `RuntimeTemplateEvaluator` now allows `CompiledTemplateSource::SpecDerived` with an empty `op_templates` vector.
- The evaluator returns `RuntimeExecutionDetails` with `compat_emitter_used=false` and `template_source=SpecDerived`.
- No `PcodeOp` is materialized for zero-op constructors.
- Added a unit test covering the zero-op success path.

### NOP Stream Continuation

- Target rows `0x140001416` and `0x1400013f6` now match Ghidra as decoded instructions with empty P-code.
- The previous error stopped stream comparison and caused six downstream `missing_fission_instruction` cascade rows.
- After the fix, both CRT startup rows decode through the following `NOP` / `ADD` / `RET` instructions.

### Padding / No-instruction Classification

- The comparator now preserves padding/no-instruction evidence when Ghidra reports `no instruction` and Fission has an empty zero-op decode at that address.
- These rows are bucketed as `ghidra_decode_error` plus `both_decode_error_or_padding`.
- Padding rows are not promoted to semantic `full_match`.

## Validation

Required checks run:

```text
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
cargo check -p fission-sleigh
cargo test -p fission-sleigh spec_derived_empty_template_is_zero_op_success -- --test-threads=1
cargo test -p fission-sleigh -- --test-threads=1
cargo build --release -p fission-cli
```

Result:

```text
fission-sleigh: 56 passed, 0 failed
```

Targeted raw P-code checks:

```text
test-functions-maincrtstartup:    full_match = 8
test-functions-winmaincrtstartup: full_match = 8
```

Full canonical report:

```text
Report: benchmark/artifacts/raw_p_code_benchmark/zero_op_construct_tpl/aggregate_raw_pcode_parity_report.json

bucket_totals:
  full_match: 44
  ghidra_decode_error: 2
  both_decode_error_or_padding: 2

invariants:
  compat_emitter_used: 0
  fake_placeholder_op: 0
  invalid_pcode_shape: 0

template_source_totals:
  SpecDerived: 46
```

Before/after bucket movement versus the prior `handle_tpl_store_fixed` report:

```text
full_match:                  36 -> 44
missing_fission_instruction:  6 -> 0
unsupported_template:         4 -> 0
ghidra_decode_error:          2 -> 2
both_decode_error_or_padding: 2 -> 2
```

Performance summary from the same report:

```text
Fission instructions/sec: 2.779414007794098
Ghidra instructions/sec:  0.8307026869880608
Fission/Ghidra instruction throughput ratio: 3.3458589352486885
Fission/Ghidra P-code throughput ratio:      2.945029012507419
```

## Remaining Owners

- All Ghidra-ok canonical semantic rows are now `full_match`.
- No raw P-code semantic mismatch buckets remain.
- Remaining `ghidra_decode_error = 2` / `both_decode_error_or_padding = 2` rows are padding/no-instruction cases, not runtime semantic parity failures.
- If these rows are cleaned up later, the owner is benchmark corpus row-boundary policy, not `ConstructTpl` execution.

## Follow-up: Architecture Raw P-code Seed Alignment

After the canonical x86-64 lane reached semantic parity, the architecture smoke lane exposed a separate loader/oracle seed issue for LLVM relocatable objects.

Changes:

- ELF `ET_REL` sections are now assigned Ghidra-like load addresses starting at `0x100000`, preserving section alignment.
- Relocatable function symbols are rebased through their owning section address, so `llvm_smoke` now lists at `0x100000` instead of ambiguous `0x0`.
- Disassembly byte reads now prefer executable sections when multiple relocatable sections overlap the same original `sh_addr`.
- The raw P-code architecture smoke manifest now starts rows at `0x100000` and opts into Ghidra missing-instruction disassembly for object-file rows.

Validation:

```text
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
cargo check -p fission-loader
cargo test -p fission-loader test_execution_view_prefers_executable_section_for_overlapping_va -- --test-threads=1
cargo check -p fission-sleigh
cargo check -p fission-cli
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/canonical_after_et_rel_seed
python3 benchmark/raw_p_code_benchmark/run_architecture_parallel.py --manifest benchmark/raw_p_code_benchmark/llvm_arch_smoke_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --output-dir benchmark/artifacts/raw_p_code_benchmark/architecture_parallel_et_rel_seed --fission-release --workers 2
```

Canonical raw P-code remained stable:

```text
Report: benchmark/artifacts/raw_p_code_benchmark/canonical_after_et_rel_seed/aggregate_raw_pcode_parity_report.json

bucket_totals:
  full_match: 44
  ghidra_decode_error: 2
  both_decode_error_or_padding: 2

invariants:
  compat_emitter_used: 0
  fake_placeholder_op: 0
  invalid_pcode_shape: 0
```

Architecture smoke now reaches real instruction comparison for x86-64:

```text
Report: benchmark/artifacts/raw_p_code_benchmark/architecture_parallel_et_rel_seed/architecture_parallel_report.json

aggregate bucket totals:
  full_match: 4
  pcode_op_count_mismatch: 4
  pcode_opcode_mismatch: 3
  decode_no_match: 5
  missing_fission_instruction: 14
  ghidra_decode_error: 3
```

New direct owner:

- x86-64 LLVM row mismatch is caused by `.sla` hidden `BUILD check_Rmr32_dest` not being represented in the runtime constructor tree. Ghidra emits the hidden subconstructor's `INT_ZEXT` for 32-bit register writes; Fission currently drops that hidden build during handle remapping.
- This must be fixed by real `ConstructState` / `ParserWalker` hidden operand execution, not by injecting a handwritten x86 zero-extension rule.
- AARCH64/RISC-V rows now show real `DecodeNoMatch` coverage gaps after Ghidra decodes instructions; those are decision/runtime coverage owners, not raw P-code emission parity owners.
