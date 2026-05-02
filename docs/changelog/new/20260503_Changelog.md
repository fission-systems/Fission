# 2026-05-03 Changelog

## Decompiler quality cycle: sqlite3 sample row admission

- Fixed `full_benchmark` seeded function selection so explicit `canonical_quality_rows`
  are admitted even when a `seed_limit` is active.
- Fixed row-fidelity target normalization in the single-binary benchmark path; tuple
  watchlist rows now become actual row-fidelity targets instead of being dropped.
- Replaced stale sqlite3 export/thunk canary addresses with implementation function
  rows that Fission and Ghidra both decompile:
  `0x1800085d0`, `0x180008fd0`, `0x180009d00`.
- Added a benchmark unit test covering required row admission beyond the configured
  seed limit.

## Validation

- `python3 -m unittest benchmark.full_benchmark.grand_finale_support.test_corpus_benchmark`
  passed.
- `python3 -m py_compile benchmark/full_benchmark/*.py benchmark/full_benchmark/grand_finale_support/*.py benchmark/asm_benchmark/*.py`
  passed.
- `cargo build --release -p fission-cli` passed.
- `cargo test -p fission-pcode call_target -- --test-threads=1` passed.
- `cargo test -p fission-decompiler-core call_target -- --test-threads=1` passed.
- `cargo check -p fission-pcode -p fission-decompiler-core -p fission-static -p fission-cli`
  passed.

## Benchmark

- Command:
  `python3 benchmark/full_benchmark/full_decomp_benchmark.py --corpus-manifest benchmark/config/benchmark_corpus/sqlite3_decompiler_v4_similarity_attribution.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-bin target/release/fission_cli --timeout 120 --ghidra-func-timeout 20 --pairwise-similarity-mode shared-full --output-dir benchmark/artifacts/full_benchmark/sqlite3_cycle_20260503_after`
- Artifact:
  `benchmark/artifacts/full_benchmark/sqlite3_cycle_20260503_after`
- Result:
  coverage recovered to `100.000%`, `20/20` shared successful rows,
  `avg_normalized_similarity=27.610%`, median `29.970%`, aggregate weighted
  similarity `28.250%`.
- Row-fidelity snapshot now reports all three sqlite3 implementation canaries as
  present.

## Notes

- Ghidra 11.4.2 was used as the structure reference, but the local benchmark oracle
  run used Ghidra 12.0.4 because the existing local Ghidra project data was created
  by a newer Ghidra and 11.4.2 failed with `data created with newer version and can
  not be read`.
- Exact call-target owner counters remain `0` on this sqlite3 seed set. The next
  cycle should target indirect/import call target proof instead of broad stack-local
  promotion.
- No benchmark output artifacts, Ghidra DB files, or sample binaries are intended to
  be staged.
