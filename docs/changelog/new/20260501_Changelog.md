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

## SLEIGH Canonical Gate Audit

- Added reporting-only legacy path audit fields to raw P-code probe/report output.
- The audit records debt involvement without changing semantic success: `BoundOperand -> FixedHandle` fallback, no-export subtable fallback, legacy shared-token policy, direct token parser, compatibility template source, and source-line/opprint remap.
- Promoted the x86-64 canonical perfect gate as the regression command: `--require-perfect-canonical --expected-full-match 44`.
- Preserved the strict success rule: successful rows must still report real `.sla ConstructTpl` source and cannot count compatibility or approximate P-code as success.

## Canonical Gate Audit Validation

- Report: `benchmark/artifacts/raw_p_code_benchmark/canonical_gate_audit/aggregate_raw_pcode_parity_report.json`
- `full_match = 44`
- `average_similarity_score = 1.0`
- `average_parity_ratio = 1.0`
- `compat_emitter_used = 0`
- `fake_placeholder_op = 0`
- `invalid_pcode_shape = 0`
- `template_source_totals.sla_construct_tpl = 46`
- `legacy_path_audit_totals.bound_operand_fixed_handle_fallback = 12`
- `legacy_path_audit_totals.legacy_shared_token_policy = 46`
- `legacy_path_audit_totals.no_export_subtable_fallback = 14`

## Raw P-code Multi-binary Manifest Expansion

- Added `binaries[]` manifest support to `run_raw_pcode_parity.py`.
- The runner now flattens binary-level metadata into the existing row execution path, preserving the legacy `rows[]` manifest contract.
- Added suite filters for larger manifests: `--binary-id`, `--language-filter`, and `--max-rows-per-binary`.
- Added aggregate `binary_count`, `language_count`, `binary_totals`, and `language_totals` fields for suite-level reporting.
- Added `benchmark/raw_p_code_benchmark/multi_binary_smoke.json` as a checked-in example manifest.

## Multi-binary Validation

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- `python3 -m json.tool benchmark/raw_p_code_benchmark/multi_binary_smoke.json`
- Filtered smoke report: `benchmark/artifacts/raw_p_code_benchmark/multi_binary_smoke_filtered/aggregate_raw_pcode_parity_report.json`
  - `row_count = 2`
  - `binary_count = 1`
  - `full_match = 2`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
- Schema smoke report: `benchmark/artifacts/raw_p_code_benchmark/multi_binary_smoke_filtered_schema/aggregate_raw_pcode_parity_report.json`
  - `row_count = 1`
  - `binary_count = 1`
  - `language_count = 1`
  - `full_match = 1`
- Canonical gate report: `benchmark/artifacts/raw_p_code_benchmark/multi_manifest_canonical_gate/aggregate_raw_pcode_parity_report.json`
  - `row_count = 17`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `template_source_totals.sla_construct_tpl = 46`
