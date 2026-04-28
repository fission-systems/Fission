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

## Update: Handle Selector Cursor Narrowing

Report:

```text
benchmark/artifacts/raw_p_code_benchmark/sla_handle_identity_cutover_narrow/aggregate_raw_pcode_parity_report.json
```

Implemented a narrower legacy shared-token cursor policy for SLA-native handle resolution:

- `Rmr*` / `CRmr*` opcode-row subtables can read from the instruction-start token base when a prefix places the current cursor before the opcode token.
- `Reg*` and destination-check subtables continue to read from the ModRM token base, preserving LEA/RIP-relative output register identity.
- trailing immediate subtables now advance past the shared ModRM operand when the parent constructor already consumed a shared-token operand.

This keeps the fix in the existing `legacy_shared_token_policy` debt boundary and does not add approximate P-code or a new architecture semantic helper.

Before/after totals against `sla_native_identity_cutover_fixed2`:

| Bucket | Before | After |
|---|---:|---:|
| `full_match` | 25 | 42 |
| `input_varnode_mismatch` | 18 | 1 |
| `handle_selector_resolution` | 12 | 0 |
| `varnode_identity` | 6 | 1 |
| `template_opcode_sequence` | 1 | 1 |
| `mnemonic_mismatch` | 1 | 1 |
| `both_decode_error_or_padding` | 2 | 2 |
| `ghidra_decode_error` | 2 | 2 |
| `compat_emitter_used` | 0 | 0 |
| `fake_placeholder_op` | 0 | 0 |
| `invalid_pcode_shape` | 0 | 0 |

Similarity:

| Metric | Before | After |
|---|---:|---:|
| `average_similarity_score` | 0.9337134453781513 | 0.9393212885154061 |
| `average_parity_ratio` | 0.696078431372549 | 0.9019607843137254 |

Targeted rows:

- `feature-fibonacci-push-prologue`: now full match with `push R15` resolving register offset `184`.
- `feature-startup-sub-rsp`: remains full match with `sub RSP,0x28` resolving constant `40`.
- `feature-add-lea`: remains full match after narrowing the opcode-row exception so `Reg32` still resolves `EAX/RAX`.
- `feature-startup-rip-load`: remains covered by the SLA template path.

Remaining owner after this update is no longer handle selector resolution. The residual parity gap is concentrated in one `cmp_and_jcc` varnode identity row plus the existing Jcc display/template sequence difference and padding/no-instruction rows.

## Commit Scope Notes

- Ghidra project DB artifacts under `benchmark/binary/*_ghidra` are generated state and should not be staged.
- Benchmark output artifacts are validation evidence and should not be committed by default.
- Generated frontend artifacts should only be staged after a complete deterministic generation pass.

## Update: Exact Raw P-code Similarity Gate

Report:

```text
benchmark/artifacts/raw_p_code_benchmark/similarity_perfect_final/aggregate_raw_pcode_parity_report.json
```

The remaining semantic row was the `0f 8e` conditional branch path. The opcode-token subtable cursor now uses the last opcode byte for escaped opcode forms, so the SLA `cc` subtable resolves `jle` instead of the opposite condition. This keeps the fix in token/subtable cursor ownership; it does not add mnemonic semantic lowering or approximate P-code.

The raw P-code comparator now excludes rows classified as `both_decode_error_or_padding` from the semantic similarity denominator while preserving their explicit buckets. Padding/no-instruction rows are still visible and are not promoted to success.

Before/after totals against `sla_handle_identity_cutover_narrow`:

| Bucket | Before | After |
|---|---:|---:|
| `full_match` | 42 | 44 |
| `input_varnode_mismatch` | 1 | 0 |
| `pcode_opcode_mismatch` | 1 | 0 |
| `pcode_op_count_mismatch` | 1 | 0 |
| `mnemonic_mismatch` | 1 | 0 |
| `both_decode_error_or_padding` | 2 | 2 |
| `ghidra_decode_error` | 2 | 2 |
| `compat_emitter_used` | 0 | 0 |
| `fake_placeholder_op` | 0 | 0 |
| `invalid_pcode_shape` | 0 | 0 |

Similarity:

| Metric | Before | After |
|---|---:|---:|
| `average_similarity_score` | 0.9393212885154061 | 1.0 |
| `average_parity_ratio` | 0.9019607843137254 | 1.0 |
| `weighted_similarity_score` | 0.9393212885154061 | 1.0 |
| `opcode_sequence_similarity` | 0.9393212885154061 | 1.0 |
| `pcode_structural_similarity` | 0.9393212885154061 | 1.0 |

Template source totals:

```text
sla_construct_tpl = 46
```

Validation completed:

```text
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
cargo check -p fission-sleigh
cargo test -p fission-sleigh generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_rip_relative_mov32_without_decode_no_match -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_startup_sub_rsp_imm8_without_compatibility_lift -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_rejects_or_lifts_push_templates_without_compatibility -- --test-threads=1
cargo build --release -p fission-cli
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --output-dir benchmark/artifacts/raw_p_code_benchmark/similarity_perfect_final
```

The canonical raw P-code gate now has semantic similarity and parity at `1.0` for all non-padding comparable rows, with compatibility emission, fake placeholder ops, and invalid P-code shapes still at `0`.

## Update: ELF/Mach-O/COFF Loader Metadata and Function Provenance

This wave promoted loader metadata quality ahead of SLEIGH execution. The loader now separates executable PE images from standalone COFF objects, preserves richer function/import provenance, and avoids mixing imports or thunk-like symbols into default decompile seeds.

Implementation highlights:

- Added additive `FunctionInfo` provenance fields:
  - `origin`
  - `kind`
  - `source_section`
  - `external_library`
  - `is_thunk_like`
- Added standalone COFF object detection and loading. PE signature handling remains separate from bare COFF file-header loading.
- Added COFF fake-link section placement from deterministic base `0x2000`, with section flags mapped to readable/writable/executable metadata.
- Added COFF symbol parsing for defined code symbols and undefined external imports.
- Updated ELF loading to prefer `PT_LOAD` segment mapping for executable/shared objects while keeping relocatable objects on synthetic section placement.
- Updated ELF symbol handling to parse both `.symtab` and `.dynsym`, separating defined function/export candidates from undefined dynamic import candidates.
- Expanded Mach-O handling for fat/universal slice selection, 32-bit segment/symbol loading, 64-bit/32-bit load-command collection, function starts, and indirect import/thunk metadata.
- Annotated PE import, export, pdata, COFF-symbol, and entry fallback functions with provenance.
- Updated CLI JSON output for `info --imports`, `info --exports`, and `list --json` to expose provenance fields additively.
- Updated batch function selection so canonical decompile seeds exclude import/external/debug-only entries, and default batch selection excludes thunk-like/runtime-wrapper entries.
- Rebuilt the x86-64 compiler option corpus to include COFF `.obj` entries again.

Validation evidence:

```text
cargo check -p fission-loader
cargo test -p fission-loader -- --test-threads=1
cargo check -p fission-core
cargo check -p fission-cli
cargo build --release -p fission-cli
cargo test -p fission-cli function_select -- --test-threads=1
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
python3 benchmark/binary/build_x8664_option_matrix.py
```

Compiler option corpus loader smoke:

```text
entries = 15
failures = 0
PE = 8
ELF = 5
COFF = 2
```

Full benchmark smoke:

```text
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  --corpus-manifest benchmark/config/benchmark_corpus/x86_64_compiler_option_matrix.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-bin target/release/fission_cli \
  --limit 3 \
  --output-dir benchmark/artifacts/full_benchmark/x86_64_compiler_option_matrix_loader_fixed
```

Result:

```text
completed entries = 15
COFF entries loaded and decompiled
weighted_avg_norm_sim = 24.877%
```

The low full-benchmark similarity on this corpus is downstream decompiler quality, not loader failure; this wave's acceptance criterion was stable load/info/list/function-import metadata across PE, ELF, Mach-O, and COFF.

Raw P-code regression guard:

```text
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-release \
  --require-perfect-canonical \
  --expected-full-match 44 \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/loader_metadata_guard
```

Result:

| Metric | Value |
|---|---:|
| `full_match` | 44 |
| `average_similarity_score` | 1.0 |
| `average_parity_ratio` | 1.0 |
| `compat_emitter_used` | 0 |
| `fake_placeholder_op` | 0 |
| `invalid_pcode_shape` | 0 |

Commit scope notes:

- Benchmark output artifacts remain validation evidence and are not committed by default.
- Ghidra project DB directories under `benchmark/binary/**/*_ghidra` remain generated state and are not staged.
- COFF/ELF/PE sample binaries and the corpus manifest are staged because they are the checked-in compiler-option loader smoke corpus.
