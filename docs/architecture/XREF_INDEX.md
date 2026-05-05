# Canonical XrefIndex

This note captures **ownership**, **surface APIs**, and the intended **p-code attachment point** for an additional xref layer. Implementation detail lives in Rust (`crates/fission-static/src/analysis/xref_index/`).

## Ownership

- **`fission-static`** owns [`xref_index`](../../crates/fission-static/src/analysis/xref_index/mod.rs): `XrefRecord`, `XrefIndexBuilder`, merge helpers, and queries (`refs_from_address`, `refs_to_address`, summaries).
- **`fission-cli`** / **`fission-tauri`** consume the public API only (no duplicated decode logic).

## Layers today

| Layer | Source | Notes |
|-------|--------|--------|
| Loader | `LoadedBinary` (`iat_symbols`, exports, `string_map`, `global_symbols`) | High confidence where symbols are authoritative |
| Disassembly | [`XrefDatabase`](../../crates/fission-static/src/analysis/xrefs/mod.rs) built via `RuntimeSleighFrontend::decode_window` | Operand references + decoded flow targets; merged as `XrefSourceLayer::Disassembly` |
| Relocation | — | Reserved; PE/ELF relocation tables are not yet first-class on `LoadedBinary` (counts stay `0` with `relocation_note` in summaries) |

Confidence follows [`fission_loader::Confidence`](../../crates/fission-loader/src/detector/mod.rs); Low-confidence facts remain eligible for omission from downstream “confirmed” surfaces.

## CLI surfaces

- `fission_cli xrefs <binary> [--json] [--no-disassembly] [--function ADDR]` emits the merged index (full `refs` in JSON).
- `fission_cli info <binary> --xrefs [--json]` embeds `{ "summary": … }` under `xrefs` without dumping every record.

## P-code layer (planned hook)

**Goal:** emit xref records whose evidence cites pcode ops / VARNODE flows once lifted artifacts exist, without rescuing semantics via pretty-printed decompiler text.

**Natural seam:** [`decompile_with_rust_sleigh`](../../crates/fission-decompiler/src/rust_sleigh/pipeline.rs) (invoked from [`rust_decomp/mod.rs`](../../crates/fission-cli/src/cli/oneshot/rust_decomp/mod.rs) on the non-native CLI path). After pcode is produced for a function slice but **before** NIR normalization consumes irreversible summaries, walk pcode ops that denote memory/register flows with absolute addresses:

1. Restrict promotions using existing guards (executable range checks, confidence, symbol correlation); never reinterpret arbitrary immediates as pointers.
2. Map each qualifying pcode tuple to `XrefKind` / `XrefEvidence { layer: Pcode, pcode_op: Some(...), … }`.
3. Merge via `XrefIndexBuilder::push_record` or a dedicated `push_pcode_layer` helper mirroring [`push_disassembly_layer`](../../crates/fission-static/src/analysis/xref_index/build.rs).

**Parallel cue:** [`DecodedInstruction`](../../crates/fission-sleigh/src/runtime/mod.rs) already exposes `references` consumed by [`XrefDatabase::analyze_code`](../../crates/fission-static/src/analysis/xrefs/mod.rs); pcode xref emission should reuse the same address-resolution discipline rather than introducing ad hoc immediate parsing.

## Benchmark / oracle alignment

Stage parity JSON adds `stages.xrefs` counters via [`benchmark/stage_parity_benchmark/stage_metrics.py`](../../benchmark/stage_parity_benchmark/stage_metrics.py). Rows may populate top-level `xref_metrics` when Rust exporters exist; Ghidra joins remain under [`benchmark/ghidra_oracle_benchmark/`](../../benchmark/ghidra_oracle_benchmark/README.md).
