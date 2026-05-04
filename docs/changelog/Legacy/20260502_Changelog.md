# 2026-05-02 Changelog

## Decompiler Quality V1: Exact Call Prototype Arity

### Summary

- Started the decompiler-quality lane after the SLEIGH raw P-code gates stabilized.
- Used Ghidra decompiler owner structure as the reference boundary: `FuncProto`/`ParamID`, `Heritage`, `Merge`/`HighVariable`, `LocalSymbolMap`/`Varnode`, and `PrintC`.
- Added a focused sqlite3 sentinel manifest at `benchmark/config/benchmark_corpus/sqlite3_decompiler_v1.json`.
- Kept the implementation inside NIR callsite type propagation. The printer, CLI, GUI, and benchmark scripts remain downstream consumers and do not perform semantic repair.

### Implementation

- Added exact API prototype arity pruning in `fission-pcode` callsite type propagation.
- The pruning rule only applies when an API signature from the existing signature provider resolves the target or wrapper target exactly.
- Calls without a known API signature keep their observed argument list unchanged.
- Wrapper targets are first resolved through call summaries, then checked against the exact API signature.
- Added regression tests proving:
  - `MessageBoxA` extra arguments are removed only because a known fixed API signature exists.
  - Unknown call targets are not pruned.
  - Wrapper-to-import calls use the resolved import signature.

### Benchmark Baseline

- Baseline command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/commercial_binary/binary/sqlite3.dll --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 5 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode sampled --pairwise-sample-size 5 --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v1_baseline`
- Baseline result:
  - `avg_norm_sim=19.910%`
  - `coverage=100.000%`
  - Fission functions: `5`
  - Ghidra functions: `5`

### After Result

- Focused manifest command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v1.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 5 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode sampled --pairwise-sample-size 5 --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v1_after`
- After result:
  - `weighted_avg_norm_sim=19.910%`
  - `coverage=100.000%`
  - No focused sqlite3 similarity regression.
- This wave is intentionally conservative: it adds exact prototype ownership and tests first. Broader stack-local promotion remains future work because prior stack-address trials showed that internal replacement evidence is not enough without benchmark improvement.

### Raw P-code Guard

- Command:
  - `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/decompiler_v1_x86_64_guard`
- Result:
  - `perfect_canonical_gate=passed`
  - `full_match=44`
  - `average_similarity_score=1.0`
  - `average_parity_ratio=1.0`
  - `compat_emitter_used=0`
  - `fake_placeholder_op=0`
  - `invalid_pcode_shape=0`
  - success source: `sla_construct_tpl`

### Validation

- `cargo test -p fission-pcode callsite_type_prop_prunes -- --test-threads=1`
- `cargo test -p fission-pcode type_hints_imports -- --test-threads=1`
- `cargo test -p fission-pcode type_hints_stack_slots -- --test-threads=1`
- `cargo check -p fission-pcode -p fission-static -p fission-cli`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`

### Remaining Work

- Promote stack locals only when the owner proof and row-fidelity result both improve.
- Add the `UnknownKiller.sys @ 0x140003360` malware sentinel once the sample is present locally.
- Extend call/prototype telemetry so future benchmark reports can show exact prototype hits, rejected unknown targets, and stack-local merge outcomes separately.

## Decompiler Quality V2: Call/Prototype Telemetry Gate

### Summary

- Added measurement-first call/prototype owner telemetry without changing call rewrite semantics.
- `UnknownKiller.sys` remains an external qualitative reference only because the sample is not present locally.
- Kept sqlite3 as the focused decompiler sentinel and preserved the existing no-repair boundary: no printer, CLI, GUI, or benchmark semantic fixes.

### Implementation

- Added additive `NirBuildStats` counters:
  - `call_prototype_exact_api_arity_pruned_count`
  - `call_prototype_unknown_target_kept_count`
  - `call_prototype_wrapper_resolved_count`
  - `call_prototype_signature_missing_count`
- Wired the counters through normalize wave stats and `merge_assign`.
- Recorded exact API arity pruning only when the existing signature provider proves the API signature.
- Recorded wrapper resolution, missing signature, and unknown-target keep cases without pruning or dropping arguments.
- Exposed the new counters through `benchmark/full_benchmark` owner metrics:
  - `call_proto_exact_arity_pruned`
  - `call_proto_unknown_target_kept`
  - `call_proto_wrapper_resolved`
  - `call_proto_signature_missing`

### Validation

- Targeted tests:
  - `cargo test -p fission-pcode callsite_type_prop_prunes -- --test-threads=1`
  - `cargo test -p fission-pcode callsite_type_prop_keeps_args_when_summary_signature_missing -- --test-threads=1`
  - `cargo test -p fission-pcode type_hints_imports -- --test-threads=1`
  - `cargo test -p fission-pcode type_hints_stack_slots -- --test-threads=1`
- Build/check:
  - `cargo check -p fission-pcode -p fission-static -p fission-cli`
  - `cargo build --release -p fission-cli`
  - `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/raw_p_code_benchmark/*.py`

### Decompiler Sentinel

- Command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v1.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 5 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode sampled --pairwise-sample-size 5 --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v2_telemetry`
- Result:
  - `avg_norm_sim=19.910%`
  - `coverage=100.000%`
  - failed binary rows: `0`
  - new owner metrics present in the report
  - sqlite3 focused counters were all `0.000`, which is expected for this measurement-only smoke when no exact prototype pruning is exercised by the selected rows.

### Raw P-code Guard

- Command:
  - `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/decompiler_v2_x86_64_guard`
- Result:
  - `perfect_canonical_gate=passed`
  - `full_match=44`
  - `average_similarity_score=1.0`
  - `average_parity_ratio=1.0`
  - `compat_emitter_used=0`
  - `fake_placeholder_op=0`
  - `invalid_pcode_shape=0`
  - success source: `sla_construct_tpl`

### Remaining Work

- Use the new owner counters to decide whether the next decompiler wave should target prototype coverage, wrapper summaries, or stack-local materialization.
- Keep stack-local promotion gated by row-fidelity improvement, not internal trace success alone.

## Decompiler Quality V3: Import Call Target Resolution Gate

### Summary

- Connected decompiler call target identity to exact loader import metadata and existing `NirTypeContext.call_target_refs`.
- Kept `UnknownKiller.sys` as external qualitative evidence only; it is not present locally and is not a benchmark fixture.
- Preserved the no-repair boundary: no printer-only API name substitution, no benchmark semantic repair, no binary-specific rule, and no heuristic argument deletion.

### Implementation

- Added additive `NirBuildStats` counters:
  - `call_target_import_resolved_count`
  - `call_target_direct_symbol_resolved_count`
  - `call_target_unresolved_sub_fallback_count`
  - `call_target_context_missing_count`
- Exposed the counters through full-benchmark owner metrics:
  - `call_target_import_resolved`
  - `call_target_direct_symbol_resolved`
  - `call_target_unresolved_sub_fallback`
  - `call_target_context_missing`
- Updated call lowering so direct constant call targets first consult `NirTypeContext.call_target_refs`.
- Import provenance resolves to the loader/API symbol in `HirExpr::Call.target`; the printer only renders that semantic target.
- Missing context or unresolved exact target keeps the existing `sub_<addr>` fallback and records telemetry.
- Updated `fission-decompiler-core` context construction so loader imports and IAT symbols are inserted as `CallTargetRef { provenance: Import, edge_kind: Import }` before direct/global symbols.
- Import symbols now win over same-address function names, avoiding cases where an import thunk name like `sub_401000` masks an exact loader import such as `CloseHandle`.
- Added `benchmark/config/benchmark_corpus/sqlite3_decompiler_v3_calls.json` as a focused sqlite3 reporting lane for call-target owner metrics.

### Validation

- Targeted tests:
  - `cargo test -p fission-pcode call_target -- --test-threads=1`
  - `cargo test -p fission-decompiler-core loader_imports_drive_preview_call_target_refs_before_function_names -- --test-threads=1`
  - `cargo test -p fission-pcode callsite_type_prop_prunes -- --test-threads=1`
  - `cargo test -p fission-pcode type_hints_imports -- --test-threads=1`
- Benchmark/reporting tests:
  - `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_extract_owner_metrics_from_engine_summary`
  - `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/raw_p_code_benchmark/*.py`
- Build/check:
  - `CARGO_INCREMENTAL=0 cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  - `CARGO_INCREMENTAL=0 cargo build --release -p fission-cli`

### Decompiler Sentinel

- V1 guard command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v1.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 5 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode sampled --pairwise-sample-size 5 --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v3_v1_guard`
- V1 guard result:
  - `weighted_avg_norm_sim=19.910%`
  - `coverage=100.000%`
  - failed binary rows: `0`
- V3 calls command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v3_calls.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 20 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode sampled --pairwise-sample-size 5 --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v3_calls`
- V3 calls result:
  - `weighted_avg_norm_sim=14.940%`
  - `coverage=100.000%`
  - failed binary rows: `0`
  - new call-target owner metrics were present in the report.
  - current sqlite3 selected rows did not exercise call lowering, so the new call-target counters remained `0.000`; exact resolution is covered by unit tests and the next benchmark lane should select rows that actually lower call expressions.

### Raw P-code Guard

- Command:
  - `python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --require-perfect-canonical --expected-full-match 44 --output-dir benchmark/artifacts/raw_p_code_benchmark/decompiler_v3_x86_64_guard`
- Result:
  - `perfect_canonical_gate=passed`
  - `full_match=44`
  - `average_similarity_score=1.0`
  - `average_parity_ratio=1.0`
  - `compat_emitter_used=0`
  - `fake_placeholder_op=0`
  - `invalid_pcode_shape=0`
  - success source: `sla_construct_tpl`

### Remaining Work

- Choose a decompiler sentinel row that contains real lowered call expressions so corpus metrics can show nonzero import/direct/fallback target counters.
- Continue stack-local and argument materialization only when exact owner proof and row-fidelity both improve.

## Loader Accuracy: PE Export Thunks and Assembly Parity Lane

### Summary

- Fixed the sqlite3 decompiler sentinel seed mismatch at the loader owner.
- PE exports that are exact x86 relative jump thunks are now classified as `export_thunk` instead of user-facing code seeds.
- Added an additive `FunctionInfo.thunk_target` field and surfaced it in CLI JSON output.
- Added a Python assembly parity benchmark so instruction listing regressions can be measured separately from raw p-code and decompiler quality.

### Implementation

- Added exact PE export thunk target detection for `E9 rel32` and `EB rel8` entries.
- Classification is gated by the resolved Ghidra-style load spec language id and requires the target VA to land in an executable section.
- No p-code, HIR, printer, or benchmark-side semantic repair was added.
- Export thunks remain visible through `info --exports --json` with:
  - `kind = "export_thunk"`
  - `is_thunk_like = true`
  - `thunk_target = <exact target VA>`
- Canonical function listing excludes these thunk-like exports through the existing function view filter.

### sqlite3.dll Evidence

- Command:
  - `target/release/fission_cli info benchmark/binary/x86-64/window/commercial_binary/binary/sqlite3.dll --exports --json`
- Result:
  - total exports: `378`
  - export thunks: `375`
  - example: `0x18000104b sqlite3_backup_init -> 0x180017170`
- Canonical `list --json` now starts at implementation/code seeds such as `_start` and `.pdata`/code functions instead of sqlite3 export jump stubs.

### Assembly Parity Benchmark

- Added:
  - `benchmark/asm_benchmark/run_asm_parity.py`
  - `benchmark/asm_benchmark/sqlite3_export_thunks.json`
  - `benchmark/asm_benchmark/README.md`
- Added assembly similarity metrics:
  - `average_similarity_score`
  - `average_address_score`
  - `average_bytes_score`
  - `average_text_score`
- Command:
  - `python3 benchmark/asm_benchmark/run_asm_parity.py --manifest benchmark/asm_benchmark/sqlite3_export_thunks.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --output-dir benchmark/artifacts/asm_benchmark/sqlite3_export_thunks`
- Result:
  - `full_match=3/3`
  - `average_similarity_score=1.0`
  - `average_address_score=1.0`
  - `average_bytes_score=1.0`
  - `average_text_score=1.0`
  - rows: `0x18000104b`, `0x1800013fc`, `0x18000140b`

### Full Benchmark After Loader Fix

- Command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v3_calls.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 20 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v3_calls_loader_thunks`
- Result:
  - `avg_norm_sim=27.610%`
  - `coverage=100.000%`
  - failed binary rows: `0`
- Interpretation:
  - The prior low similarity was partly caused by comparing Fission export thunk skeletons against Ghidra-followed implementation bodies.
  - After thunk classification, both sides are seeded on implementation functions; remaining similarity gap belongs to decompiler/NIR/HIR/structuring quality, not loader seed mismatch.

### Validation

- `cargo check -p fission-loader -p fission-core -p fission-cli`
- `cargo check -p fission-loader -p fission-core -p fission-cli -p fission-tauri -p fission-static -p fission-dynamic`
- `cargo test -p fission-loader pe_export_relative_jump_thunk -- --test-threads=1`
- `cargo test -p fission-loader -- --test-threads=1`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/asm_benchmark/*.py`

## Decompiler Quality V4: Similarity Attribution

### Summary

- Added report-only row-level decompiler similarity attribution for the sqlite3 focused lane.
- Kept this wave diagnostic-only: no stack rewrite, call rewrite expansion, printer repair, benchmark repair, or binary-specific rule was added.
- Added a new focused manifest at `benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json`.
- The attribution connects rendered-code surface scores and existing owner metrics to owner buckets so low similarity can be triaged before semantic changes.

### Implementation

- Added additive per-row scores to full-benchmark pairwise rows:
  - `call_surface_score`
  - `stack_local_score`
  - `control_flow_score`
  - `name_type_score`
  - `literal_score`
- Added report-only owner buckets:
  - `call_target_missing`
  - `prototype_arity_missing`
  - `stack_local_unmerged`
  - `control_flow_goto_heavy`
  - `type_name_surface`
  - `literal_or_const_surface`
- Added corpus/single benchmark summary aggregation for bucket counts and average scores.
- Updated compact artifacts and markdown templates so attribution appears in generated reports.
- Added unit coverage for attribution fields and owner bucket extraction.

### Decompiler Sentinel

- Command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 20 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v4_similarity_attribution`
- Result:
  - `avg_norm_sim=27.610%`
  - `coverage=100.000%`
  - failed binary rows: `0`
- Attribution bucket counts:
  - `call_target_missing=20`
  - `prototype_arity_missing=16`
  - `stack_local_unmerged=16`
  - `control_flow_goto_heavy=15`
  - `type_name_surface=20`
  - `literal_or_const_surface=19`
- Average surface scores:
  - `call_surface_score=0.000`
  - `stack_local_score=15.000`
  - `control_flow_score=44.089`
  - `name_type_score=1.137`
  - `literal_score=13.819`
- Interpretation:
  - The remaining gap is now clearly decompiler-quality work, not loader seed mismatch.
  - The first exact owner candidates are call-target/prototype materialization and stack-local merging, but they need semantic evidence before promotion.

### Assembly Guard

- Command:
  - `python3 benchmark/asm_benchmark/run_asm_parity.py --manifest benchmark/asm_benchmark/sqlite3_export_thunks.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --output-dir benchmark/artifacts/asm_benchmark/sqlite3_export_thunks_v4_guard`
- Result:
  - `full_match=3/3`
  - `average_similarity_score=1.0`
  - `average_address_score=1.0`
  - `average_bytes_score=1.0`
  - `average_text_score=1.0`

### Validation

- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py benchmark/asm_benchmark/*.py`
- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_pairwise_similarity_attribution_adds_owner_buckets benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_extract_owner_metrics_from_engine_summary`
- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_pairwise_similarity_attribution_adds_owner_buckets benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_build_corpus_compact_summary_keeps_capped_binary_rows benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark.CorpusBenchmarkTests.test_render_corpus_markdown_and_console_smoke`
- `cargo test -p fission-pcode call_target -- --test-threads=1`
- `cargo test -p fission-pcode callsite_type_prop_prunes -- --test-threads=1`
- `cargo test -p fission-pcode type_hints_imports -- --test-threads=1`
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
- `cargo build --release -p fission-cli`

## Decompiler Quality V5: Exact Call Target Identity

### Summary

- Added an exact `CallTargetIndex` owner in `fission-decompiler-core` so HIR call targets are promoted only from loader/fact-proven identities.
- Matched the Ghidra owner boundary inspected in `Features/Decompiler/src/decompile/cpp`:
  - `Scope::queryFunction`
  - `Scope::queryExternalRefFunction`
  - `ActionDeindirect`
  - `ActionDefaultParams`
- Kept printer, benchmark, and CLI surfaces out of semantic repair. `HirExpr::Call.target` must already be exact before output improves.

### Implementation

- Exact call-target provenance priority is now:
  - `Import/IAT`
  - `Export thunk/export`
  - `Fact/debug-style resolved name`
  - `Direct loader function`
  - `Global symbol`
- Export thunk identity is connected to both thunk VA and exact thunk target VA when `FunctionInfo.thunk_target` is present.
- Same-rank conflicting names are recorded as ambiguous and are not promoted to HIR call targets.
- Generic loader names such as `sub_*`, `FUN_*`, and `tmp_*` are not treated as exact identities.
- HIR call lowering now uses exact `call_target_refs` hits for promotion. Legacy `call_targets` no longer promotes names.
- `CALLIND` can resolve through a COPY-only constant chain. Load-derived or non-constant indirect calls remain unresolved.
- Added additive telemetry:
  - `call_target_exact_index_hit_count`
  - `call_target_exact_index_ambiguous_count`
  - `call_target_export_thunk_target_resolved_count`
  - `call_target_indirect_const_resolved_count`
  - `call_target_unresolved_no_exact_identity_count`

### Benchmarks

- Command:
  - `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --limit 20 --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_decompiler_v5_call_target_identity`
- Result:
  - `avg_norm_sim=27.610%`
  - `coverage=100.000%`
  - failed binary rows: `0`
- Note:
  - The current sqlite3 first-20 focused rows did not exercise call-target lowering counters; owner metrics are exposed and unit-gated, but benchmark counters remained `0`.

### Assembly Guard

- Command:
  - `python3 benchmark/asm_benchmark/run_asm_parity.py --manifest benchmark/asm_benchmark/sqlite3_export_thunks.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --output-dir benchmark/artifacts/asm_benchmark/sqlite3_export_thunks_v5_guard`
- Result:
  - `full_match=3/3`
  - `average_similarity_score=1.0`
  - `average_address_score=1.0`
  - `average_bytes_score=1.0`
  - `average_text_score=1.0`

### Validation

- `cargo test -p fission-pcode call_target --target-dir /tmp/fission-target-v5 -- --test-threads=1`
- `cargo test -p fission-pcode preview_callind_copy_only_constant_chain_resolves_exact_target --target-dir /tmp/fission-target-v5 -- --test-threads=1`
- `cargo test -p fission-pcode type_hints_imports --target-dir /tmp/fission-target-v5 -- --test-threads=1`
- `cargo test -p fission-pcode callsite_type_prop_prunes --target-dir /tmp/fission-target-v5 -- --test-threads=1`
- `cargo test -p fission-decompiler-core call_target --target-dir /tmp/fission-target-v5 -- --test-threads=1`
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli --target-dir /tmp/fission-target-v5`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/asm_benchmark/*.py`

### Notes

- The default debug target directory had a stale rustc process during this run, so targeted Rust tests/checks used `/tmp/fission-target-v5`. Release build to `target/release/fission_cli` succeeded and was used for benchmarks.
- No stack-local, FID, byte-pattern, DIE, GDT, printer-only, or binary-specific repair was added.
