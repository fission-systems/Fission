# 2026-05-01 Changelog

## SLEIGH Raw P-code Constant Varnode Parity Restore

- Reverted the broad heuristic-purge regression and restored the canonical x86-64 raw P-code lane to perfect structural parity.
- Fixed the remaining `feature-fibonacci-lea-dec` failure at `0x14000148e` without instruction-family or architecture-specific fallback.
- Root cause: `.sla` `ConstTpl` produced a const-space value of `0xffffffffffffffff`; Fission rejected it as exceeding `i64`, while Ghidra emits the same bit pattern as constant `-1`.
- Change: const-space `VarnodeTpl` / `HandleTpl` offsets are now converted through the existing `Varnode::constant(i64)` representation by preserving the 64-bit bit pattern.
- Added a targeted runtime regression test for `8d 41 ff` LEA negative displacement emission.

## Validation

- `cargo check -p fission-sleigh`
- `cargo test -p fission-sleigh generated_runtime_decodes_lea_negative_displacement_const_without_decode_error -- --test-threads=1`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- Raw P-code gate:
  - Report: `benchmark/artifacts/raw_p_code_benchmark/const_signed_restore/aggregate_raw_pcode_parity_report.json`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 46`

## Notes

- No approximate P-code path was added.
- No mnemonic-family semantic emitter was added.
- No architecture-name branch was added for the fix.
- Benchmark artifacts and Ghidra project DB state remain uncommitted.
