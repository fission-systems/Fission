# Assembly Parity Benchmark

This lane compares Ghidra and Fission instruction listings before decompiler
quality is measured. It is intentionally narrower than raw P-code parity and
full decompilation:

- no semantic repair
- no benchmark-side thunk following
- no pseudocode comparison
- no architecture-specific fixups

The first use case is separating export jump-thunk seed problems from actual
decompiler quality regressions.

Example:

```bash
python3 benchmark/asm_benchmark/run_asm_parity.py \
  --manifest benchmark/asm_benchmark/sqlite3_export_thunks.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-bin target/release/fission_cli \
  --output-dir benchmark/artifacts/asm_benchmark/sqlite3_export_thunks
```
