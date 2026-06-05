# CFG Parity Benchmark

This benchmark compares Fission's address-keyed CFG against Ghidra's CFG oracle.

## Parity level

The default gate compares:

- **Fission**: `pcode_instruction_cfg` (instruction-level CFG on SLEIGH-lifted raw pcode; includes zero-pcode nop leaders; call fall-through without splitting blocks)
- **Ghidra**: `ghidra_basic_block_model` (`BasicBlockModel` instruction-level raw CFG)

Both sides canonicalize to:

- sorted unique `block_starts`
- sorted unique `(from, to)` edges keyed by block start addresses
- sorted unique `exit_blocks`

**CALL flow edges are excluded** on both sides. Ghidra `BasicBlockModel` records call destinations, but they are not part of the decompiler CFG edge set we compare.

Production `build_cfg_blocks` in `fission-sleigh` now uses the same instruction-level boundary rules as `pcode_instruction_cfg` (ops preserved inside each block; successors keyed by block index).

Alternate models remain available for diagnostics:

- `pcode_structuring` / `ghidra_high_pcode` (optimized pcode-block layer)
- `ghidra_instruction_flow` (manual instruction-flow builder)

## One-function run

```bash
python3 benchmark/cfg_parity/run_cfg_parity.py \
  --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --addr 0x140001450 \
  --fission-release \
  --output-dir benchmark/artifacts/cfg_parity/test-functions-add
```

## Canonical manifest

```bash
python3 benchmark/cfg_parity/run_cfg_parity.py \
  --manifest benchmark/cfg_parity/canonical_rows.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-release \
  --require-full-match \
  --output-dir benchmark/artifacts/cfg_parity/canonical-latest
```

## Full-binary sweep (gap inventory)

Compare every Ghidra function against Fission in one binary:

```bash
python3 benchmark/cfg_parity/sweep_cfg_parity.py \
  --fission-release \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --output-dir benchmark/artifacts/cfg_parity/sweep
```

Outputs per binary under `benchmark/artifacts/cfg_parity/sweep/<binary-stem>/gap_inventory.json` plus aggregate `gap_inventory.md`.

Known sweep buckets outside the canonical gate:

- `missing_fission_function`: import/thunk symbols Ghidra lists but Fission sweep skips (lift-only path).
- CRT/Mingw startup (`WinMainCRTStartup`, `__tmainCRTStartup`, …): tail-call / noreturn edge differences under investigation.

## Rust CI fixtures

Checked-in Ghidra oracle snapshots live under `benchmark/cfg_parity/fixtures/`.
The crate test `cfg_parity_matches_ghidra_fixtures` lifts the same binaries and
asserts Fission instruction-level exports match those fixtures without requiring
Ghidra at test time.

Regenerate fixtures after intentional CFG changes:

```bash
python3 benchmark/cfg_parity/run_cfg_parity.py \
  --manifest benchmark/cfg_parity/canonical_rows.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-release \
  --output-dir benchmark/artifacts/cfg_parity/fixture-regen

python3 benchmark/cfg_parity/update_fixtures.py \
  --source benchmark/artifacts/cfg_parity/fixture-regen
```

## Outputs

Per row:

- `ghidra_cfg.json`
- `fission_cfg.json`
- `cfg_parity_report.json`

Manifest aggregate:

- `aggregate_cfg_parity_report.json`

Comparison buckets:

- `full_match`
- `block_set_mismatch`
- `edge_set_mismatch`
- `exit_set_mismatch`
