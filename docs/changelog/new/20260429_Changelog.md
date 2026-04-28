# Changelog: SLA Native Identity Cutover Boundary

**Date:** 2026-04-29
**Scope:** `fission-sleigh` SLA constructor identity, ConstructTpl execution guardrails, raw P-code parity diagnostics

## Summary

Started moving the raw P-code path away from Fission-local constructor identity and toward Ghidra compiled `.sla` identity as the canonical owner. This wave does not claim the full 1:1 migration is complete. It adds native SLA identity fields, removes the instruction-root orphan-tree fallback from the canonical decoder, tightens fail-closed behavior, and improves benchmark owner hints so remaining high-similarity mismatches point at subtable/handle identity instead of generic semantic failure.

The raw P-code policy remains strict:

- no approximate P-code success
- no `NativeFission` or `CompatibilityLowered` success counted in parity
- no fake placeholder op
- no invalid P-code shape entering downstream IR
- successful rows report `sla_construct_tpl` as the template source in benchmark output

## Implementation Notes

### SLA Native Identity

- Added SLA identity metadata to executable constructors:
  - `sla_subtable_name`
  - `sla_constructor_id`
  - `sla_constructor_slot`
  - `source_file`
  - `source_line`
- Added decode status tracking for constructors so decode-failed SLA entries remain typed unsupported rather than being inferred from source-key placeholders.
- Preserved subtable id/name and constructor slot on decoded SLA constructor templates.
- Removed the canonical "largest orphan tree is instruction" fallback. If the `.sla` instruction root cannot be decoded explicitly, runtime selection now fails closed with `selection_no_instruction_root`.

### Runtime Guardrails

- `ConditionPredicate` is now rejected as display/compatibility-only in raw P-code template execution.
- Removed the dead synthetic condition-predicate emitter from the canonical template executor.
- Tightened dynamic memory target detection to use fixed handle space identity rather than `BoundOperand::Memory`.
- Fixed the RIP-relative direct fixed-handle case so `mov RAX, [rip+disp]` resolves as `COPY ram(...) -> register` through the `.sla` template path, without reintroducing memory operand synthesis.

### Benchmark Diagnostics

- Extended raw P-code owner hints to distinguish:
  - `sla_constructor_identity`
  - `sla_subtable_identity`
  - `handle_selector_resolution`
  - `exported_handle_resolution`
  - `dynamic_pointer_identity`
  - `template_opcode_sequence`
  - `display_only_mnemonic`
  - `padding_or_no_instruction`
- Raw benchmark reports now expose `raw_template_source` while normalizing `SpecDerived` success rows to `sla_construct_tpl`.

## Raw P-code Evidence

Baseline reference:

```text
benchmark/artifacts/raw_p_code_benchmark/similarity_weighted_full/aggregate_raw_pcode_parity_report.json
```

New report:

```text
benchmark/artifacts/raw_p_code_benchmark/sla_native_identity_cutover_fixed2/aggregate_raw_pcode_parity_report.json
```

Before/after totals:

| Bucket | Before | After |
|---|---:|---:|
| `full_match` | 25 | 25 |
| `input_varnode_mismatch` | 18 | 18 |
| `pcode_opcode_mismatch` | 1 | 1 |
| `pcode_op_count_mismatch` | 1 | 1 |
| `mnemonic_mismatch` | 1 | 1 |
| `both_decode_error_or_padding` | 2 | 2 |
| `ghidra_decode_error` | 2 | 2 |
| `compat_emitter_used` | 0 | 0 |
| `fake_placeholder_op` | 0 | 0 |
| `invalid_pcode_shape` | 0 | 0 |

Similarity:

| Metric | Before | After |
|---|---:|---:|
| `average_similarity_score` | 0.9337134453781513 | 0.9337134453781513 |
| `average_parity_ratio` | 0.696078431372549 | 0.696078431372549 |

Template source totals:

```text
sla_construct_tpl = 46
```

The main result is architectural tightening without parity regression. The remaining parity gap is still concentrated in varnode identity and constructor/subtable/handle resolution, not approximate semantic emission.

## Validation

Completed:

```text
cargo check -p fission-sleigh
cargo check -p fission-cli
cargo build --release -p fission-cli
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/sla_native_identity_cutover_fixed2
```

Partially completed / stopped:

```text
cargo test -p fission-sleigh -- --test-threads=1
```

Observed failures before stopping the long-running test job:

- `compiler::codegen::tests::checked_in_generated_artifacts_match_renderer_output`
- `compiler::ir::tests::compile_frontend_collects_pcode_ops_and_patterns`

The same test run then stayed in `compiler::tests::compiles_all_checked_in_entry_specs` for several minutes and was stopped to avoid blocking this wave. The generated artifact drift is expected around this schema/identity boundary and should be handled in a dedicated deterministic regeneration pass.

Attempted but stopped:

```text
cargo run -p fission-sleigh --example generate_sleigh_frontends
```

The generator progressed through multiple processor families and confirmed explicit `.sla` instruction roots for x86-adjacent, AArch64, ARM, BPF, LoongArch, and others, but stalled during MIPS-family generation. No generated artifact set is treated as complete evidence from this interrupted run.

## Remaining Work

- Replace remaining Fission-local constructor vector assumptions with direct `.sla` `(subtable identity, constructor id)` lookups.
- Remove or quarantine source-line/opprint remap helpers from runtime success paths.
- Finish deterministic full frontend regeneration after the MIPS generation stall is diagnosed.
- Reduce the remaining `input_varnode_mismatch = 18` by resolving handle selector and exported handle identity from the native SLA model.
- Keep display-only mnemonic differences separate from raw P-code parity.

## Commit Scope Notes

- Ghidra project DB artifacts under `benchmark/binary/*_ghidra` are generated state and should not be staged.
- Benchmark output artifacts are validation evidence and should not be committed by default.
- Generated frontend artifacts should only be staged after a complete deterministic generation pass.
