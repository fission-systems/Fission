# CFG Fact Coverage Benchmark

Measures how faithfully Fission's `ControlFlowFacts` reproduce Ghidra's **input**
signals (labels, non-call flow references, noreturn metadata) before BBM/CFG parity.

This is intentionally separate from `benchmark/cfg_parity/`, which compares final
instruction-level CFG snapshots.

## Signals

| Signal | Ghidra source | Fission source |
| --- | --- | --- |
| Labels | `SymbolTable` symbols inside function body | loader `cfg_label_leaders`, function entries, global/IAT symbols |
| Flow edges | `ReferenceManager` flow refs + `Instruction.getFlows()`, call excluded | `XrefDatabase` jump rows only |
| Noreturn | `Function.hasNoReturn()` | Ghidra noreturn XML via call-site xref resolution |

## Layout

```text
benchmark/cfg_facts/
├── canonical_rows.json
├── ghidra_facts.py
├── fission_facts.py
├── compare_facts.py
├── run_fact_coverage.py
├── sweep_fact_coverage.py
├── fixtures/                 # checked-in oracle slices (Rust CI)
└── README.md
```

Artifacts land in `benchmark/artifacts/cfg_facts/`.

## Quick start

Single row from the manifest:

```bash
python3 benchmark/cfg_facts/run_fact_coverage.py \
  --manifest benchmark/cfg_facts/canonical_rows.json \
  --row test-functions-add
```

Fission-only dump:

```bash
python3 benchmark/cfg_facts/fission_facts.py \
  --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --addr 0x140001450
```

Rust probe (used by `fission_facts.py`):

```bash
cargo run -p fission-static --example facts_probe -- \
  --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --addr 0x140001450
```

## Metrics

- `label_recall` = |Fission labels ∩ Ghidra labels| / |Ghidra labels|
- `flow_edge_recall` / `flow_edge_precision` on `(from_instr, to)` jump/jcc edges (calls excluded)
- `noreturn_match` compares Ghidra `hasNoReturn()` with Fission noreturn call-site inference

## CI fixture test

`crates/fission-static/tests/fact_coverage_fixtures.rs` compares `ControlFlowFacts`
slices against checked-in JSON under `fixtures/` without requiring Ghidra at runtime.
