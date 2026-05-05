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
