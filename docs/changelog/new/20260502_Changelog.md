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
