# 2026-05-06 Changelog

## Windows small C PE function extent provenance

- Fixed PE loader merging for x64 `.pdata` function records that share an
  address with COFF symbol-table functions. Fission now keeps the real COFF
  function name while filling an unknown `FunctionInfo.size` from `.pdata`
  unwind extents.
- The change is zero-dependency and stays in the loader provenance layer. It
  does not add binary-specific decompiler heuristics, printer rewrites, or
  downstream benchmark/reporting repair.
- Sample validation on
  `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe`:
  `fibonacci @ 0x140001470` changed from `size=0` to `size=942`, while nearby
  sample functions such as `add`, `max`, and `sum_array` also retain COFF names
  with `.pdata` extents.

## Validation

- `CARGO_TARGET_DIR=/tmp/fission-codex-target cargo test -p fission-loader pe_pdata_merge_preserves_coff_name_and_adds_extent`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-codex-target cargo check -p fission-loader`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-codex-target cargo build -p fission-cli --release`
  passed.

## Benchmark

- Before:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --limit 20 --timeout 300 --ghidra-func-timeout 30 --fission-bin target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build --use-ghidra-cache --ghidra-cache-dir benchmark/artifacts/ghidra_cache --output-dir benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-before`
- After:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --limit 20 --timeout 300 --ghidra-func-timeout 30 --fission-bin /tmp/fission-codex-target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build --use-ghidra-cache --ghidra-cache-dir benchmark/artifacts/ghidra_cache --output-dir benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-after --baseline-dir benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-before`
- Artifacts:
  `benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-before`
  and
  `benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-after`.
- Result:
  the baseline gate passed. The limit-20 sample run stayed at
  `avg_normalized_similarity=36.910%`, median `38.320%`, aggregate weighted
  similarity `7.170%`, and `20/20` shared successful rows.
- Owner counters:
  `missing_merge` improved from `4323` to `4270`; `alias_unsafe` moved from
  `13458` to `13511`, so this is not a promoted decompiler-quality win by
  itself.

## Notes

- Live pyghidra execution was blocked because
  `vendor/ghidra/ghidra-Ghidra_12.0.4_build` is not a compiled launchable
  Ghidra tree in this checkout. The 2-way benchmark reused the existing
  checked artifact `test_functions-balanced-latest/ghidra_full.json` through
  the benchmark cache path.
- `rapidfuzz` was not installed, so the benchmark used the built-in `difflib`
  backend.
- The next quality owner remains NIR/structuring: `fibonacci @ 0x140001470`
  still reports `blockgraph_region_rejected_must_emit_label=6`, high alias
  residue, and only `3.11%` normalized similarity against Ghidra.

## Windows small C guarded-tail flag helper purity

- Extended the guarded-tail pure helper contract to cover Fission's synthetic
  flag intrinsics `__carry`, `__scarry`, and `__sborrow`, in addition to the
  existing `__popcount` helper.
- Centralized that helper allowlist in `guarded_tail/mod.rs` and reused it from
  suffix call-effect classification and alias-forward purity checks. This keeps
  the change in NIR structuring proof logic instead of adding a printer patch or
  a binary-specific heuristic.
- Sample diagnostics on
  `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe` showed the
  flag helpers are now internalized as pure known helper calls in the
  `fibonacci @ 0x140001470` guarded-tail suffix trace. The rendered row did not
  change yet; the remaining blockers are still alias fallthrough and join/label
  ownership.

## Validation

- `CARGO_TARGET_DIR=/tmp/fission-cycle2-target cargo test -p fission-pcode flag_intrinsic -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle2-target cargo build -p fission-cli --release`
  passed.

## Benchmark

- Before:
  `benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-after`
- After:
  `benchmark/artifacts/full_benchmark/windows-small-c-flag-helper-purity-after`
- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --limit 20 --timeout 300 --ghidra-func-timeout 30 --fission-bin /tmp/fission-cycle2-target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build --use-ghidra-cache --ghidra-cache-dir benchmark/artifacts/ghidra_cache --output-dir benchmark/artifacts/full_benchmark/windows-small-c-flag-helper-purity-after --baseline-dir benchmark/artifacts/full_benchmark/windows-small-c-test-functions-pdata-after`
- Result:
  the regression gate passed. The limit-20 sample run stayed at
  `avg_normalized_similarity=36.910%` and `100.0%` shared success. Aggregate
  normalized similarity moved from `7.170%` to `7.180%`.
- Structural counters:
  top-level labels improved from `25` to `24`; BlockGraph completed regions
  improved from `0` to `2`; `blockgraph_region_rejected_must_emit_label`
  improved from `20` to `16`. `alias_unsafe`, `missing_merge`,
  `materialization_stabilized`, generic local names, and gotos were unchanged.

## ABI subregister parameter aliasing

- Fixed ABI parameter-slot classification for x86-64 subregister names such as
  `ecx`, `cx`, `cl`, `edi`, `r8d`, and `r9b`. These now map to the same
  parameter slot as their 64-bit register family (`rcx`, `rdi`, `r8`, `r9`,
  and so on) under the active calling convention.
- Reused that ABI-family classification when removing redundant
  `param_k = <incoming register>` copies, so entry-spill promotion no longer
  leaves alias-width copies like `param_1 = ecx`.
- This is an ABI-provider fix, not an ISA-specific printer patch. It preserves
  the existing Windows x64 and System V AMD64 slot order while handling
  width-specific register aliases produced by the lifter.

## Validation

- `CARGO_TARGET_DIR=/tmp/fission-cycle3-target cargo test -p fission-pcode entry_param_promotion -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle3-target cargo test -p fission-pcode calling_convention -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle3-target cargo check -p fission-pcode`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle3-target cargo build -p fission-cli --release`
  passed.

## Benchmark

- Before:
  `benchmark/artifacts/full_benchmark/windows-small-c-flag-helper-purity-after`
- After:
  `benchmark/artifacts/full_benchmark/windows-small-c-abi-subregister-param-after`
- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --limit 20 --timeout 300 --ghidra-func-timeout 30 --fission-bin /tmp/fission-cycle3-target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build --use-ghidra-cache --ghidra-cache-dir benchmark/artifacts/ghidra_cache --output-dir benchmark/artifacts/full_benchmark/windows-small-c-abi-subregister-param-after --baseline-dir benchmark/artifacts/full_benchmark/windows-small-c-flag-helper-purity-after`
- Result:
  the regression gate passed, but sample quality metrics were unchanged:
  `avg_normalized_similarity=36.910%`, aggregate normalized similarity
  `7.180%`, `100.0%` shared success, `top_level_label_total=24`,
  `goto_total=34`, BlockGraph complete regions `2`, and
  `blockgraph_region_rejected_must_emit_label=16`.
- Row note:
  `fibonacci @ 0x140001470` still renders as `ulonglong fibonacci()` with
  `var_8`-based parameter surface. The ABI alias fix is a prerequisite for
  direct subregister entry spills, but this row's remaining parameter gap is
  downstream stack/local surface recovery rather than direct `ecx` naming.

## Runtime register-space ABI parameter recovery

- Taught the NIR ABI/entry-analysis layer to recognize Rust-Sleigh register
  varnodes that arrive in runtime register space `4` in addition to the legacy
  Ghidra JSON register space `1`.
- Kept the expansion deliberately narrow: runtime register space `4` is used
  for ABI parameter-slot and entry-alias recovery, but general register
  rendering and stack-base surfacing still follow the existing legacy register
  path. A wider register-space conversion was tested and rejected because it
  inflated labels, gotos, undefined return types, and materialization counts.
- Fixed `register_param()` ordering so entry aliases such as `edi <- ecx` are
  checked before returning a non-ABI hardware register name. This allows saved
  entry argument copies to surface as `param_k`.
- Kept callsite argument recovery on the legacy Ghidra register space only.
  Entry formal recovery can consume Rust-Sleigh register space `4`, but
  unknown indirect calls should not inherit weak runtime-space register
  carriers as synthetic call arguments.
- Suppressed entry-register formal parameter surfacing for compiler/runtime
  bootstrap helpers (`CRTStartup` and dynamic TLS helpers) while preserving it
  for normal user functions. The same helper family is also classified as
  `CompilerRuntimeHelper` in the function-provenance index.
- Tightened guarded-tail pure-helper suffix handling so known pure helper calls
  still go through the dedicated ownership/escape proof instead of being
  accepted by the generic pure-statement fast path.
- Fixed 64-bit Rust-Sleigh `RET` lowering so the p-code return target loaded
  from the stack is not treated as the function return value when a valid ABI
  return-register definition exists after the last side effect. This recovers
  leaf register-return expressions such as `add @ 0x140001450` while avoiding
  stale `RAX` promotion across calls and stores in startup/void helper shapes.

## Validation

- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo test -p fission-pcode bootstrap_x86::preview_ -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo test -p fission-static function_provenance -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo test -p fission-pcode suffix_accepts_known_pure_helper -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo test -p fission-pcode suffix_rejects_known_pure_helper -- --test-threads=1`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo test -p fission-pcode -- --test-threads=1`
  passed: `722 passed`.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo check -p fission-pcode`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo check -p fission-decompiler`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo check -p fission-static`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle5-target cargo build -p fission-cli --release`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle6-target cargo test -p fission-pcode -- --test-threads=1`
  passed: `723 passed`.
- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark`
  passed: `32 passed`.
- `CARGO_TARGET_DIR=/tmp/fission-cycle6-target cargo check -p fission-pcode`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle6-target cargo check -p fission-decompiler`
  passed.
- `CARGO_TARGET_DIR=/tmp/fission-cycle6-target cargo build -p fission-cli --release`
  passed.
- `git diff --check` passed.

## Benchmark

- Before:
  `benchmark/artifacts/full_benchmark/windows-small-c-abi-subregister-param-after`
- After:
  `benchmark/artifacts/full_benchmark/windows-small-c-runtime-register-space-callarg-narrow-after`
- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --limit 20 --timeout 300 --ghidra-func-timeout 30 --fission-bin /tmp/fission-cycle6-target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build --use-ghidra-cache --ghidra-cache-dir benchmark/artifacts/ghidra_cache --output-dir benchmark/artifacts/full_benchmark/windows-small-c-runtime-register-space-callarg-narrow-after --baseline-dir benchmark/artifacts/full_benchmark/windows-small-c-abi-subregister-param-after --regression-threshold 2.0 --pairwise-similarity-mode shared-full --aggregate-similarity-mode weighted`
- Result:
  average normalized similarity improved from `36.91%` to `38.24%`; median
  normalized similarity improved from `38.32%` to `42.78%`; aggregate weighted
  normalized similarity improved from `7.18%` to `7.49%`; shared success stayed
  `20/20`. `goto_total=34`, `top_level_label_total=24`,
  `blockgraph_region_complete_count=2`, `alias_unsafe=13511`, and
  `missing_merge=4270` stayed unchanged.
- Row note:
  `fibonacci @ 0x140001470` changed from `ulonglong fibonacci()` with `var_8`
  uses to `ulonglong fibonacci(uint param_1)` with `var_8` count `0`, and row
  similarity improved from `3.11%` to `3.16%` in the final cached Ghidra
  comparison run. `add @ 0x140001450` no longer returns the `ret` stack-load
  artifact (`return *var_20`) and now renders the recovered LEA dataflow as
  `return (ulonglong)(uint)(param_1 + param_2);`.
- Gate note:
  row fidelity passed after narrowing callsite recovery and suppressing runtime
  helper entry params: `__tmainCRTStartup @ 0x140001010` stayed `2.57%`,
  `fibonacci @ 0x140001470` improved `3.11% -> 3.16%`,
  `fill_matrix @ 0x140001870` improved `5.80% -> 5.87%`, and
  `__do_global_ctors @ 0x140001940` stayed `10.69%`. The baseline gate still
  failed because `generic_param_name_sum` increased from `0` to `14` and
  `generic_local_name_sum` increased from `276` to `278`. The param increase is
  the expected surface cost of recovering ABI formals for user functions such as
  `add`, `max`, `fibonacci`, `sum_array`, `fill_matrix`, and `swap`; the local
  increase remains a follow-up shape-cleanup item, so this run is reported as
  quality-improved but not mechanically gate-clean.
