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
